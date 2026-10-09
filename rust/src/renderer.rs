//! Per-frame logic: params -> GpuParams, adaptive resolution, stats.
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::{adaptive::*, camera::{Camera, Mode}, gfx::*, halton, params::*};

pub struct Renderer {
    gfx: Gfx,
    pub store: Store,
    pub cam: Camera,
    pub sticks: [f32; 4],
    adaptive: AdaptiveRes,
    governor: Governor,
    warmup: u32,
    last_ms: f32,
    win: (u32, u32),
    frame: u32,
    seed: u32,
    time: f32,
    last: Instant,
    start: Instant,
    prev_pose: ([f32; 3], [f32; 3]),
    prev_jitter: [f32; 2],
    reset_history: bool,
    fps_t: Instant,
    fps_n: u32,
    log_t: Instant,
    pub stats: Arc<Mutex<[f32; 16]>>,
    custom: Option<crate::scene::SceneData>,
    scene_loaded: i32,
}

impl Renderer {
    pub fn new(gfx: Gfx, win: (u32, u32), stats: Arc<Mutex<[f32; 16]>>) -> Renderer {
        let cam = Camera::new();
        let pose = cam.pose();
        let mut r = Renderer { gfx, store: Store::new(), cam, sticks: [0.0; 4], adaptive: AdaptiveRes::new(0.25, 0.33, 0.33), governor: Governor::new(), warmup: 3, last_ms: 0.0, win,
            frame: 0, seed: 1, time: 0.0, last: Instant::now(), start: Instant::now(), prev_pose: pose,
            prev_jitter: [0.0; 2], reset_history: true, fps_t: Instant::now(), fps_n: 0, log_t: Instant::now(), stats, custom: None, scene_loaded: crate::scene::DEFAULT_SCENE as i32 };
        r.cam.apply(crate::scene::builtin(crate::scene::DEFAULT_SCENE).cam);
        r.prev_pose = r.cam.pose();
        r
    }

    fn apply(&mut self, d: &crate::scene::SceneData) {
        self.gfx.set_scene(d);
        self.cam.apply(d.cam);
        self.prev_pose = self.cam.pose();
        self.reset_history = true;
    }

    /// Loads the scene selected by the `scene` setting (built-in 0..2, or the last loaded file for 3).
    fn apply_scene(&mut self) {
        let i = self.store.get(id::SCENE) as i32;
        if i == self.scene_loaded { return; }
        if i >= 3 {
            let Some(d) = self.custom.take() else { return };
            self.apply(&d);
            self.custom = Some(d);
        } else {
            let d = crate::scene::builtin(i as usize);
            self.apply(&d);
        }
        self.scene_loaded = i;
        log::info!("scene {i} applied");
    }

    /// A scene parsed from a file: becomes scene 3.
    pub fn set_custom_scene(&mut self, d: crate::scene::SceneData) {
        self.custom = Some(d);
        self.scene_loaded = -1;
        self.store.set(id::SCENE as i32, 3.0);
        self.apply_scene();
    }

    pub fn set_param(&mut self, id: i32, v: f32) {
        if self.store.set(id, v) == Change::Changed && affects_history(id as usize) {
            self.reset_history = true;
        }
        if id as usize == id::CAM_MODE { self.cam.set_mode(Mode::from_param(self.store.get(id::CAM_MODE))); }
        if id as usize == id::FRAME_LIMIT { self.gfx.set_present_mode(self.store.on(id::FRAME_LIMIT)); }
        if id as usize == id::SCALE_IDX { self.reset_history = true; }
        if id as usize == id::SCENE { self.apply_scene(); }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.win = (w.max(1), h.max(1));
        self.gfx.resize_surface(self.win.0, self.win.1);
        self.reset_history = true;
    }

    pub fn gfx(&self) -> &Gfx { &self.gfx }

    pub fn frame(&mut self) { self.frame_with(None); }

    pub fn frame_with(&mut self, overlay: Option<Overlay>) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        let s = &self.store;
        let max_scale = SCALES[s.get(id::SCALE_IDX) as usize];
        let scale = if s.on(id::ADAPTIVE) {
            self.adaptive.set_max(max_scale);
            self.adaptive.scale()
        } else { max_scale };
        // Safety cap (heavy settings): never exceeds what the last frames could afford.
        let scale = scale * self.governor.limit_scale;
        let ps1 = s.on(id::PS1);
        let (rw, rh) = if ps1 { ps1_size(self.win.0, self.win.1) } else { render_size(self.win.0, self.win.1, scale) };
        let scale = rw as f32 / self.win.0.max(1) as f32;
        let gi_block = if s.gi_split() { s.gi_block() } else { 0 };
        if self.gfx.ensure_targets(rw, rh, gi_block) { self.reset_history = true; self.warmup = 2; }
        let gov = governor_active(self.warmup);

        self.cam.update(dt, s.get(id::ORBIT) as u32, Mode::from_param(s.get(id::CAM_MODE)), self.sticks,
            speed_factor(s.get(id::MOVE_SPEED)), speed_factor(s.get(id::LOOK_SPEED)));
        let pose = self.cam.pose();
        let pose = if ps1 { (ps1_snap(pose.0), ps1_snap(pose.1)) } else { pose };
        let fov = s.get(id::FOV).to_radians();
        let moved = pose != self.prev_pose;
        let pt = s.get(id::MODE) as u32 == 1;
        if !pt && s.on(id::ANIM) { self.time += dt; }
        let still = pt && !moved && !self.reset_history;
        let temporal = s.on(id::TEMPORAL);
        let jit = if temporal && !ps1 { halton::jitter(self.frame) } else { [0.0, 0.0] };
        let mut flags = s.flags();
        if ps1 { flags = (flags & !flags::CHECKER) | flags::PS1; }
        if self.gfx.is_srgb() { flags |= flags::SRGB; }
        if still { flags |= flags::STILL; }
        if moved { flags |= flags::MOVED; }
        let (ca, cb) = s.light_emission();
        let p = GpuParams {
            res: [rw as f32, rh as f32], out_size: [self.win.0 as f32, self.win.1 as f32],
            cam_pos: pose.0, time: if ps1 { ps1_time(self.time) } else { self.time }, cam_target: pose.1, fov,
            prev_pos: self.prev_pose.0, frame: self.frame, prev_target: self.prev_pose.1, seed: self.seed,
            jitter: jit, prev_jitter: self.prev_jitter,
            mode: pt as u32, flags, bounces: s.get(id::BOUNCES) as u32, spp: if gov && self.last_ms > Governor::SLOW_MS { 1 } else { s.get(id::SPP) as u32 },
            exposure: s.get(id::EXPOSURE) * 0.5, tonemap: s.get(id::TONEMAP) as u32,
            sharpen: [0.0, 0.25, 0.5, 0.8][s.get(id::SHARPEN) as usize], hist_floor: s.hist_floor(),
            sky_kind: s.get(id::SKY) as u32, history_reset: self.reset_history as u32, pad0: 0, pad1: 0,
            col_a: ca, light_k: crate::params::LIGHT_K[s.get(id::LIGHT) as usize - 1], col_b: cb, pad3: 0.0,
            gi_block, gi_floor: 0.15, pad4: 0, pad5: 0,
        };
        let ms = self.gfx.render(&p, s.get(id::DENOISE) as u32, (self.frame & 1) as usize, overlay);
        if ms < 0.0 {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        if ms >= 0.0 {
            if gov {
                self.last_ms = ms;
                self.governor.update(ms, self.start.elapsed().as_secs_f32());
            } else {
                self.last_ms = 0.0;
                self.warmup -= 1;
            }
            if s.on(id::ADAPTIVE) { self.adaptive.update(ms, s.get(id::TARGET_FPS), self.start.elapsed().as_secs_f32()); }
            self.prev_pose = pose;
            self.prev_jitter = jit;
            self.frame = self.frame.wrapping_add(1);
            self.seed = self.seed.wrapping_add(1);
            self.reset_history = false;
            self.fps_n += 1;
            let el = self.fps_t.elapsed().as_secs_f32();
            if el >= 0.5 {
                let fps = self.fps_n as f32 / el;
                let mut st = [0.0f32; 16];
                st[..6].copy_from_slice(&[fps, ms, rw as f32, rh as f32, scale, pt as u32 as f32]);
                st[8..].copy_from_slice(&self.gfx.pass_ms());
                *self.stats.lock().unwrap() = st;
                if self.log_t.elapsed().as_secs_f32() >= 2.0 {
                    log::info!("{:.1} fps | gpu {:.1} ms | {}x{} ({:.2}x) | {} | flags {:#x} | bounces {}{}", fps, ms, rw, rh, scale, if pt { "path" } else { "hybrid" }, flags, s.get(id::BOUNCES), crate::profile::format_pass_ms(&self.gfx.pass_ms()));
                    self.log_t = Instant::now();
                }
                self.fps_n = 0;
                self.fps_t = Instant::now();
            }
        }
    }
}
