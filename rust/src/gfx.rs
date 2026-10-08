//! GPU wrapper: device, surface, render targets and the per-frame pass chain.
use std::num::NonZeroU64;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use crate::params::flags::GI_SPLIT;
#[cfg(target_os = "android")]
use ndk::native_window::NativeWindow;
#[cfg(target_os = "android")]
use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};

/// Extra drawing on top of the presented frame (the desktop settings panel): runs after the present pass, before submit.
pub type Overlay<'a> = &'a mut dyn FnMut(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView);

/// Must match `struct Params` in shaders/common.wgsl (192 bytes; std140-compatible, no implicit padding).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct GpuParams {
    pub res: [f32; 2], pub out_size: [f32; 2],
    pub cam_pos: [f32; 3], pub time: f32,
    pub cam_target: [f32; 3], pub fov: f32,
    pub prev_pos: [f32; 3], pub frame: u32,
    pub prev_target: [f32; 3], pub seed: u32,
    pub jitter: [f32; 2], pub prev_jitter: [f32; 2],
    pub mode: u32, pub flags: u32, pub bounces: u32, pub spp: u32,
    pub exposure: f32, pub tonemap: u32, pub sharpen: f32, pub hist_floor: f32,
    pub sky_kind: u32, pub history_reset: u32, pub pad0: u32, pub pad1: u32,
    pub col_a: [f32; 3], pub light_k: f32,
    pub col_b: [f32; 3], pub pad3: f32,
    pub gi_block: u32, pub gi_floor: f32, pub pad4: u32, pub pad5: u32,
}

const _: () = assert!(std::mem::size_of::<GpuParams>() == 192);

#[allow(dead_code)]
struct Tex { tex: wgpu::Texture, view: wgpu::TextureView }

#[allow(dead_code)]
struct GiTargets { bs: u32, w: u32, h: u32, raw: Tex, hist: [Tex; 2], tmp: [Tex; 2] }

#[allow(dead_code)]
struct Targets {
    w: u32, h: u32,
    raw: Tex,
    gbuf: [Tex; 2],
    hist: [Tex; 2],
    tmp: [Tex; 2],
    alb: Tex,
    comp: Tex,
    gi: Option<GiTargets>,
}

/// GPU timestamp queries (only when the adapter supports TIMESTAMP_QUERY). Two query slots per pass slot.
struct Prof { qs: wgpu::QuerySet, resolve: wgpu::Buffer, read: wgpu::Buffer, period: f32 }

pub struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    modes: Vec<wgpu::PresentMode>,
    params_buf: wgpu::Buffer,
    scene_buf: wgpu::Buffer,
    step_bufs: [wgpu::Buffer; 3],
    gi_step_bufs: [wgpu::Buffer; 2],
    sampler: wgpu::Sampler,
    trace_pl: wgpu::ComputePipeline, trace_bgl: wgpu::BindGroupLayout,
    temporal_pl: wgpu::ComputePipeline, temporal_bgl: wgpu::BindGroupLayout,
    atrous_pl: wgpu::ComputePipeline, atrous_bgl: wgpu::BindGroupLayout,
    gi_trace_pl: wgpu::ComputePipeline, gi_trace_bgl: wgpu::BindGroupLayout,
    gi_temporal_pl: wgpu::ComputePipeline, gi_temporal_bgl: wgpu::BindGroupLayout,
    gi_atrous_pl: wgpu::ComputePipeline, gi_atrous_bgl: wgpu::BindGroupLayout,
    composite_pl: wgpu::ComputePipeline, composite_bgl: wgpu::BindGroupLayout,
    present_pl: wgpu::RenderPipeline, present_bgl: wgpu::BindGroupLayout,
    targets: Option<Targets>,
    prof: Option<Prof>,
    pass_ms: [f32; 8],
    _window: Box<dyn std::any::Any>, // keeps the native window alive; dropped after `surface` (field order)
}

fn shader(dev: &wgpu::Device, name: &str, src: String) -> wgpu::ShaderModule {
    dev.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(src.into()) })
}
fn uniform(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, count: None,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None } }
}
fn tex_in(binding: u32, vis: wgpu::ShaderStages, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, count: None,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2, multisampled: false } }
}
fn tex_out(binding: u32, fmt: wgpu::TextureFormat) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, count: None,
        ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: fmt,
            view_dimension: wgpu::TextureViewDimension::D2 } }
}
fn make_tex(dev: &wgpu::Device, w: u32, h: u32, fmt: wgpu::TextureFormat) -> Tex {
    let tex = dev.create_texture(&wgpu::TextureDescriptor {
        label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
    let view = tex.create_view(&Default::default());
    Tex { tex, view }
}
fn bg<'a>(dev: &wgpu::Device, l: &wgpu::BindGroupLayout, e: &[(u32, wgpu::BindingResource<'a>)]) -> wgpu::BindGroup {
    let entries: Vec<_> = e.iter().map(|(b, r)| wgpu::BindGroupEntry { binding: *b, resource: r.clone() }).collect();
    dev.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: l, entries: &entries })
}
fn bgl(dev: &wgpu::Device, name: &str, entries: &[wgpu::BindGroupLayoutEntry]) -> wgpu::BindGroupLayout {
    dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(name), entries })
}
fn layout(dev: &wgpu::Device, l: &wgpu::BindGroupLayout) -> wgpu::PipelineLayout {
    dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[l], push_constant_ranges: &[] })
}

impl Gfx {
    #[cfg(target_os = "android")]
    pub fn new(window: NativeWindow, w: u32, h: u32) -> Gfx {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: RawDisplayHandle::Android(AndroidDisplayHandle::new()),
                raw_window_handle: RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window.ptr().cast())),
            })
        }.expect("create_surface");
        Gfx::from_surface(instance, surface, Box::new(window), w, h)
    }

    /// `keep` owns whatever the surface was created from (native window); it lives as long as the Gfx.
    pub fn from_surface(instance: wgpu::Instance, surface: wgpu::Surface<'static>, keep: Box<dyn std::any::Any>, w: u32, h: u32) -> Gfx {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).expect("no GPU adapter");
        let info = adapter.get_info();
        log::info!("adapter: {} ({:?}) {}", info.name, info.backend, info.driver_info);
        let want = wgpu::Features::TIMESTAMP_QUERY;
        let has_ts = adapter.features().contains(want);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("device"),
            required_features: if has_ts { want } else { wgpu::Features::empty() },
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
        }, None)).expect("request_device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w.max(1),
            height: h.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        log::info!("surface {}x{} {:?} modes {:?}", config.width, config.height, format, caps.present_modes);

        let cs = wgpu::ShaderStages::COMPUTE;
        let fs = wgpu::ShaderStages::FRAGMENT;
        let trace_bgl = bgl(&device, "trace", &[uniform(0, cs), tex_out(1, wgpu::TextureFormat::Rgba16Float), tex_out(2, wgpu::TextureFormat::Rgba32Float), tex_out(3, wgpu::TextureFormat::Rgba16Float), uniform(4, cs)]);
        let temporal_bgl = bgl(&device, "temporal", &[
            uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_in(3, cs, false), tex_in(4, cs, false),
            tex_out(5, wgpu::TextureFormat::Rgba16Float),
        ]);
        let atrous_bgl = bgl(&device, "atrous", &[
            uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false),
            tex_out(3, wgpu::TextureFormat::Rgba16Float), uniform(4, cs),
        ]);
        let f16 = wgpu::TextureFormat::Rgba16Float;
        let gi_trace_bgl = bgl(&device, "gi_trace", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_out(3, f16), uniform(4, cs)]);
        let gi_temporal_bgl = bgl(&device, "gi_temporal", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_in(3, cs, false), tex_in(4, cs, false), tex_out(5, f16)]);
        let gi_atrous_bgl = bgl(&device, "gi_atrous", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_out(3, f16), uniform(4, cs)]);
        let composite_bgl = bgl(&device, "composite", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_in(3, cs, false), tex_in(4, cs, false), tex_out(5, f16)]);
        let mk = |name: &str, src: String, l: &wgpu::BindGroupLayout| {
            let m = shader(&device, name, src);
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name), layout: Some(&layout(&device, l)), module: &m,
                entry_point: Some("main"), compilation_options: Default::default(), cache: None,
            })
        };
        let gi_trace_pl = mk("gi_trace", crate::shaders::gi_trace(), &gi_trace_bgl);
        let gi_temporal_pl = mk("gi_temporal", crate::shaders::gi_temporal(), &gi_temporal_bgl);
        let gi_atrous_pl = mk("gi_atrous", crate::shaders::gi_atrous(), &gi_atrous_bgl);
        let composite_pl = mk("composite", crate::shaders::composite(), &composite_bgl);
        let present_bgl = bgl(&device, "present", &[
            uniform(0, fs), tex_in(1, fs, true),
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: fs, count: None,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering) },
        ]);

        let trace_mod = shader(&device, "trace", crate::shaders::trace());
        let temporal_mod = shader(&device, "temporal", crate::shaders::temporal());
        let temporal_pl = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("temporal"), layout: Some(&layout(&device, &temporal_bgl)), module: &temporal_mod,
            entry_point: Some("main"), compilation_options: Default::default(), cache: None,
        });
        let atrous_mod = shader(&device, "atrous", crate::shaders::atrous());
        let atrous_pl = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("atrous"), layout: Some(&layout(&device, &atrous_bgl)), module: &atrous_mod,
            entry_point: Some("main"), compilation_options: Default::default(), cache: None,
        });
        let present_mod = shader(&device, "present", crate::shaders::present());
        let trace_pl = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("trace"), layout: Some(&layout(&device, &trace_bgl)), module: &trace_mod,
            entry_point: Some("main"), compilation_options: Default::default(), cache: None,
        });
        let present_pl = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present"),
            layout: Some(&layout(&device, &present_bgl)),
            vertex: wgpu::VertexState { module: &present_mod, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &present_mod, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"), size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let scene_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene"), size: std::mem::size_of::<crate::scene::SceneU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let step = || device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("step"), size: NonZeroU64::new(16).unwrap().get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let step_bufs = [step(), step(), step()];
        let gi_step_bufs = [step(), step()];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let prof = if has_ts {
            let qs = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("ts"), ty: wgpu::QueryType::Timestamp, count: 16 });
            let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some("ts_resolve"), size: 8 * crate::profile::RESOLVE_ALIGN,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let read = device.create_buffer(&wgpu::BufferDescriptor { label: Some("ts_read"), size: 128,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            Some(Prof { qs, resolve, read, period: queue.get_timestamp_period() })
        } else { None };
        log::info!("gpu timestamps: {}", has_ts);

        Gfx {
            surface, device, queue, config, modes: caps.present_modes, params_buf, scene_buf, step_bufs, gi_step_bufs, sampler,
            gi_trace_pl, gi_trace_bgl, gi_temporal_pl, gi_temporal_bgl, gi_atrous_pl, gi_atrous_bgl, composite_pl, composite_bgl,
            trace_pl, trace_bgl, temporal_pl, temporal_bgl, atrous_pl, atrous_bgl, present_pl, present_bgl, targets: None, prof, pass_ms: [0.0; 8], _window: keep,
        }
    }

    /// Per-pass GPU ms of the last frame (slots: trace, gi_trace, gi_temporal, gi_atrous, temporal, atrous, composite, present); 0 = unsupported/unmeasured.
    pub fn pass_ms(&self) -> [f32; 8] { self.pass_ms }

    /// Uploads a scene (the next frame renders it).
    pub fn set_scene(&self, u: &crate::scene::SceneU) {
        self.queue.write_buffer(&self.scene_buf, 0, bytemuck::bytes_of(u));
    }

    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }
    pub fn format(&self) -> wgpu::TextureFormat { self.config.format }

    pub fn is_srgb(&self) -> bool { self.config.format.is_srgb() }

    pub fn resize_surface(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || (w == self.config.width && h == self.config.height) { return; }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn set_present_mode(&mut self, limit: bool) {
        use wgpu::PresentMode::*;
        let m = if limit { Fifo }
            else if self.modes.contains(&Mailbox) { Mailbox }
            else if self.modes.contains(&Immediate) { Immediate }
            else { Fifo };
        if m != self.config.present_mode {
            self.config.present_mode = m;
            self.surface.configure(&self.device, &self.config);
            log::info!("present mode {:?}", m);
        }
    }

    /// (Re)creates render-resolution targets; true if they were recreated.
    pub fn ensure_targets(&mut self, rw: u32, rh: u32, gi_block: u32) -> bool {
        if let Some(t) = &self.targets {
            if t.w == rw && t.h == rh && t.gi.as_ref().map(|g| g.bs).unwrap_or(0) == gi_block { return false; }
        }
        let d = &self.device;
        let f16 = wgpu::TextureFormat::Rgba16Float;
        let f32x4 = wgpu::TextureFormat::Rgba32Float;
        let gi = if gi_block > 0 {
            let (gw, gh) = crate::gi::gi_size(rw, rh, gi_block);
            for (i, b) in self.gi_step_bufs.iter().enumerate() {
                let v: [u32; 4] = [1 << i, gw, gh, 0];
                self.queue.write_buffer(b, 0, bytemuck::bytes_of(&v));
            }
            Some(GiTargets { bs: gi_block, w: gw, h: gh, raw: make_tex(d, gw, gh, f16),
                hist: [make_tex(d, gw, gh, f16), make_tex(d, gw, gh, f16)],
                tmp: [make_tex(d, gw, gh, f16), make_tex(d, gw, gh, f16)] })
        } else { None };
        self.targets = Some(Targets {
            w: rw, h: rh,
            raw: make_tex(d, rw, rh, f16),
            gbuf: [make_tex(d, rw, rh, f32x4), make_tex(d, rw, rh, f32x4)],
            hist: [make_tex(d, rw, rh, f16), make_tex(d, rw, rh, f16)],
            tmp: [make_tex(d, rw, rh, f16), make_tex(d, rw, rh, f16)],
            alb: make_tex(d, rw, rh, f16),
            comp: make_tex(d, rw, rh, f16),
            gi,
        });
        for (i, b) in self.step_bufs.iter().enumerate() {
            let v: [u32; 4] = [1 << i, rw, rh, 0];
            self.queue.write_buffer(b, 0, bytemuck::bytes_of(&v));
        }
        true
    }

    /// Encodes and submits one frame; returns GPU time in ms, or negative if the frame was skipped.
    pub fn render(&mut self, p: &GpuParams, denoise_iters: u32, parity: usize, overlay: Option<Overlay>) -> f32 {
        self.queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(p));
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return -1.0;
            }
            Err(e) => {
                log::warn!("surface error: {:?}", e);
                return -1.0;
            }
        };
        let Some(t) = self.targets.as_ref() else { return -1.0 };
        let view = frame.texture.create_view(&Default::default());
        let dev = &self.device;
        let pb = self.params_buf.as_entire_binding();

        let trace_bg = bg(dev, &self.trace_bgl, &[
            (0, pb.clone()),
            (1, wgpu::BindingResource::TextureView(&t.raw.view)),
            (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
            (3, wgpu::BindingResource::TextureView(&t.alb.view)),
            (4, self.scene_buf.as_entire_binding()),
        ]);
        let temporal_bg = bg(dev, &self.temporal_bgl, &[
            (0, pb.clone()),
            (1, wgpu::BindingResource::TextureView(&t.raw.view)),
            (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
            (3, wgpu::BindingResource::TextureView(&t.gbuf[1 - parity].view)),
            (4, wgpu::BindingResource::TextureView(&t.hist[1 - parity].view)),
            (5, wgpu::BindingResource::TextureView(&t.hist[parity].view)),
        ]);
        // A-trous chain: hist[parity] -> tmp[0] -> tmp[1] -> tmp[0]. History stays the unfiltered
        // temporal output. Each iteration has its own step buffer ([step, w, h, 0]) and bind group.
        let iters = (denoise_iters as usize).min(self.step_bufs.len());
        let mut atrous_bgs = Vec::with_capacity(iters);
        let mut out = &t.hist[parity].view;
        for k in 0..iters {
            let src = if k == 0 { &t.hist[parity].view } else { &t.tmp[(k + 1) % 2].view };
            let dst = &t.tmp[k % 2].view;
            atrous_bgs.push(bg(dev, &self.atrous_bgl, &[
                (0, self.step_bufs[k].as_entire_binding()),
                (1, wgpu::BindingResource::TextureView(src)),
                (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
                (3, wgpu::BindingResource::TextureView(dst)),
                (4, pb.clone()),
            ]));
            out = dst;
        }
        let gi_on = p.flags & GI_SPLIT != 0 && p.gi_block > 0 && t.gi.is_some();
        let mut gi_bgs = None;
        let mut comp_bg = None;
        if let (true, Some(g)) = (gi_on, t.gi.as_ref()) {
            let a = bg(dev, &self.gi_trace_bgl, &[
                (0, pb.clone()),
                (1, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
                (2, wgpu::BindingResource::TextureView(&t.alb.view)),
                (3, wgpu::BindingResource::TextureView(&g.raw.view)),
                (4, self.scene_buf.as_entire_binding()),
            ]);
            let b = bg(dev, &self.gi_temporal_bgl, &[
                (0, pb.clone()),
                (1, wgpu::BindingResource::TextureView(&g.raw.view)),
                (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
                (3, wgpu::BindingResource::TextureView(&t.gbuf[1 - parity].view)),
                (4, wgpu::BindingResource::TextureView(&g.hist[1 - parity].view)),
                (5, wgpu::BindingResource::TextureView(&g.hist[parity].view)),
            ]);
            let c0 = bg(dev, &self.gi_atrous_bgl, &[
                (0, self.gi_step_bufs[0].as_entire_binding()),
                (1, wgpu::BindingResource::TextureView(&g.hist[parity].view)),
                (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
                (3, wgpu::BindingResource::TextureView(&g.tmp[0].view)),
                (4, pb.clone()),
            ]);
            let c1 = bg(dev, &self.gi_atrous_bgl, &[
                (0, self.gi_step_bufs[1].as_entire_binding()),
                (1, wgpu::BindingResource::TextureView(&g.tmp[0].view)),
                (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
                (3, wgpu::BindingResource::TextureView(&g.tmp[1].view)),
                (4, pb.clone()),
            ]);
            gi_bgs = Some((a, b, c0, c1, (g.w, g.h)));
            comp_bg = Some(bg(dev, &self.composite_bgl, &[
                (0, pb.clone()),
                (1, wgpu::BindingResource::TextureView(out)),
                (2, wgpu::BindingResource::TextureView(&t.gbuf[parity].view)),
                (3, wgpu::BindingResource::TextureView(&t.alb.view)),
                (4, wgpu::BindingResource::TextureView(&g.tmp[1].view)),
                (5, wgpu::BindingResource::TextureView(&t.comp.view)),
            ]));
        }
        let shown = if gi_on { &t.comp.view } else { out };
        let present_bg = bg(dev, &self.present_bgl, &[
            (0, pb),
            (1, wgpu::BindingResource::TextureView(shown)),
            (2, wgpu::BindingResource::Sampler(&self.sampler)),
        ]);

        let prof = self.prof.as_ref();
        // Compute-pass timestamp writes: slot s uses query indices 2s (begin) and 2s+1 (end); either may be omitted.
        let cts = |b: Option<u32>, e: Option<u32>| prof.and_then(|p| crate::profile::pass_indices(b, e).map(|(b, e)| wgpu::ComputePassTimestampWrites {
            query_set: &p.qs, beginning_of_pass_write_index: b, end_of_pass_write_index: e }));
        let slot = |s: u32| cts(Some(2 * s), Some(2 * s + 1));
        let mut enc = dev.create_command_encoder(&Default::default());
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("trace"), timestamp_writes: slot(0) });
            cp.set_pipeline(&self.trace_pl);
            cp.set_bind_group(0, &trace_bg, &[]);
            cp.dispatch_workgroups(t.w.div_ceil(8), t.h.div_ceil(8), 1);
        }
        if let Some((a, _, _, _, (gw, gh))) = &gi_bgs {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("gi_trace"), timestamp_writes: slot(1) });
            cp.set_pipeline(&self.gi_trace_pl);
            cp.set_bind_group(0, a, &[]);
            cp.dispatch_workgroups(gw.div_ceil(8), gh.div_ceil(8), 1);
        }
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("temporal"), timestamp_writes: slot(4) });
            cp.set_pipeline(&self.temporal_pl);
            cp.set_bind_group(0, &temporal_bg, &[]);
            cp.dispatch_workgroups(t.w.div_ceil(8), t.h.div_ceil(8), 1);
        }
        if let Some((_, b, c0, c1, (gw, gh))) = &gi_bgs {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("gi_temporal"), timestamp_writes: slot(2) });
            cp.set_pipeline(&self.gi_temporal_pl);
            cp.set_bind_group(0, b, &[]);
            cp.dispatch_workgroups(gw.div_ceil(8), gh.div_ceil(8), 1);
            drop(cp);
            for (i, abg) in [c0, c1].into_iter().enumerate() {
                let (b, e) = crate::profile::gi_atrous_query_indices(i as u32);
                let tw = cts(b, e);
                let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("gi_atrous"), timestamp_writes: tw });
                cp.set_pipeline(&self.gi_atrous_pl);
                cp.set_bind_group(0, abg, &[]);
                cp.dispatch_workgroups(gw.div_ceil(8), gh.div_ceil(8), 1);
            }
        }
        let n_at = atrous_bgs.len();
        for (i, abg) in atrous_bgs.iter().enumerate() {
            let (b, e) = crate::profile::atrous_query_indices(i as u32, n_at as u32);
            let tw = cts(b, e);
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("atrous"), timestamp_writes: tw });
            cp.set_pipeline(&self.atrous_pl);
            cp.set_bind_group(0, abg, &[]);
            cp.dispatch_workgroups(t.w.div_ceil(8), t.h.div_ceil(8), 1);
        }
        if let Some(cbg) = &comp_bg {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("composite"), timestamp_writes: slot(6) });
            cp.set_pipeline(&self.composite_pl);
            cp.set_bind_group(0, cbg, &[]);
            cp.dispatch_workgroups(t.w.div_ceil(8), t.h.div_ceil(8), 1);
        }
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: prof.map(|p| wgpu::RenderPassTimestampWrites { query_set: &p.qs,
                    beginning_of_pass_write_index: Some(14), end_of_pass_write_index: Some(15) }),
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.present_pl);
            rp.set_bind_group(0, &present_bg, &[]);
            rp.draw(0..3, 0..1);
        }
        if let Some(o) = overlay { o(&self.device, &self.queue, &mut enc, &view); }
        // Resolve only slots whose passes were encoded this frame (never-written queries must not be resolved).
        let ran = [true, gi_bgs.is_some(), gi_bgs.is_some(), gi_bgs.is_some(), true, n_at > 0, comp_bg.is_some(), true];
        if let Some(p) = prof {
            for s in (0..8u32).filter(|s| ran[*s as usize]) {
                let (q, ro, rd) = crate::profile::slot_offsets(s);
                enc.resolve_query_set(&p.qs, q, &p.resolve, ro);
                enc.copy_buffer_to_buffer(&p.resolve, ro, &p.read, rd, 16);
            }
        }
        let t0 = Instant::now();
        self.queue.submit(Some(enc.finish()));
        self.device.poll(wgpu::Maintain::Wait);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        // The GPU is already waited on each frame, so reading the timestamps adds no new pipeline stall
        // (deviation from the spec's "one frame late").
        self.pass_ms = [0.0; 8];
        if let Some(p) = &self.prof {
            let ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let ok2 = ok.clone();
            p.read.slice(..).map_async(wgpu::MapMode::Read, move |r| ok2.store(r.is_ok(), std::sync::atomic::Ordering::SeqCst));
            self.device.poll(wgpu::Maintain::Wait);
            if ok.load(std::sync::atomic::Ordering::SeqCst) {
                {
                    let view = p.read.slice(..).get_mapped_range();
                    let ticks: &[u64] = bytemuck::cast_slice(&view);
                    for s in (0..8).filter(|s| ran[*s]) { self.pass_ms[s] = crate::profile::ticks_to_ms(ticks[2 * s], ticks[2 * s + 1], p.period); }
                }
                p.read.unmap();
            }
        }
        frame.present();
        ms
    }
}
