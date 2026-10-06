//! GPU wrapper: device, surface, render targets and the per-frame pass chain.
use std::num::NonZeroU64;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use ndk::native_window::NativeWindow;
use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};

/// Must match `struct Params` in shaders/common.wgsl (176 bytes; std140-compatible, no implicit padding).
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
    pub col_a: [f32; 3], pub pad2: f32,
    pub col_b: [f32; 3], pub pad3: f32,
}

const _: () = assert!(std::mem::size_of::<GpuParams>() == 176);

#[allow(dead_code)]
struct Tex { tex: wgpu::Texture, view: wgpu::TextureView }

#[allow(dead_code)]
struct Targets {
    w: u32, h: u32,
    raw: Tex,
    gbuf: [Tex; 2],
    hist: [Tex; 2],
    tmp: [Tex; 2],
}

pub struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    modes: Vec<wgpu::PresentMode>,
    params_buf: wgpu::Buffer,
    #[allow(dead_code)]
    step_bufs: [wgpu::Buffer; 3],
    sampler: wgpu::Sampler,
    trace_pl: wgpu::ComputePipeline, trace_bgl: wgpu::BindGroupLayout,
    // temporal_pl/temporal_bgl and atrous_pl/atrous_bgl are added in Tasks 6/7.
    present_pl: wgpu::RenderPipeline, present_bgl: wgpu::BindGroupLayout,
    targets: Option<Targets>,
    _window: NativeWindow, // dropped after `surface` (field order)
}

fn shader(dev: &wgpu::Device, name: &str, body: &str) -> wgpu::ShaderModule {
    let src = format!("{}\n{}", include_str!("shaders/common.wgsl"), body);
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
    pub fn new(window: NativeWindow, w: u32, h: u32) -> Gfx {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: RawDisplayHandle::Android(AndroidDisplayHandle::new()),
                raw_window_handle: RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window.ptr().cast())),
            })
        }.expect("create_surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).expect("no GPU adapter");
        let info = adapter.get_info();
        log::info!("adapter: {} ({:?}) {}", info.name, info.backend, info.driver_info);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("device"),
            required_features: wgpu::Features::empty(),
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
        let trace_bgl = bgl(&device, "trace", &[uniform(0, cs), tex_out(1, wgpu::TextureFormat::Rgba16Float), tex_out(2, wgpu::TextureFormat::Rgba32Float)]);
        let present_bgl = bgl(&device, "present", &[
            uniform(0, fs), tex_in(1, fs, true),
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: fs, count: None,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering) },
        ]);

        let trace_mod = shader(&device, "trace", include_str!("shaders/trace.wgsl"));
        let present_mod = shader(&device, "present", include_str!("shaders/present.wgsl"));
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
        let step = || device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("step"), size: NonZeroU64::new(16).unwrap().get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let step_bufs = [step(), step(), step()];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Gfx {
            surface, device, queue, config, modes: caps.present_modes, params_buf, step_bufs, sampler,
            trace_pl, trace_bgl, present_pl, present_bgl, targets: None, _window: window,
        }
    }

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
    pub fn ensure_targets(&mut self, rw: u32, rh: u32) -> bool {
        if let Some(t) = &self.targets {
            if t.w == rw && t.h == rh { return false; }
        }
        let d = &self.device;
        let f16 = wgpu::TextureFormat::Rgba16Float;
        let f32x4 = wgpu::TextureFormat::Rgba32Float;
        self.targets = Some(Targets {
            w: rw, h: rh,
            raw: make_tex(d, rw, rh, f16),
            gbuf: [make_tex(d, rw, rh, f32x4), make_tex(d, rw, rh, f32x4)],
            hist: [make_tex(d, rw, rh, f16), make_tex(d, rw, rh, f16)],
            tmp: [make_tex(d, rw, rh, f16), make_tex(d, rw, rh, f16)],
        });
        for (i, b) in self.step_bufs.iter().enumerate() {
            let v: [u32; 4] = [1 << i, rw, rh, 0];
            self.queue.write_buffer(b, 0, bytemuck::bytes_of(&v));
        }
        true
    }

    /// Encodes and submits one frame; returns GPU time in ms, or negative if the frame was skipped.
    pub fn render(&mut self, p: &GpuParams, _denoise_iters: u32, parity: usize) -> f32 {
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
        ]);
        // Tasks 6/7 insert temporal + atrous here and change the sampled texture.
        let present_bg = bg(dev, &self.present_bgl, &[
            (0, pb),
            (1, wgpu::BindingResource::TextureView(&t.raw.view)),
            (2, wgpu::BindingResource::Sampler(&self.sampler)),
        ]);

        let mut enc = dev.create_command_encoder(&Default::default());
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("trace"), timestamp_writes: None });
            cp.set_pipeline(&self.trace_pl);
            cp.set_bind_group(0, &trace_bg, &[]);
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
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.present_pl);
            rp.set_bind_group(0, &present_bg, &[]);
            rp.draw(0..3, 0..1);
        }
        let t0 = Instant::now();
        self.queue.submit(Some(enc.finish()));
        self.device.poll(wgpu::Maintain::Wait);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        frame.present();
        ms
    }
}
