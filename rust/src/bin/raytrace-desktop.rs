//! Raytrace for Linux desktops: winit window, the same wgpu renderer as the Android app, egui settings panel.
//!
//! Mouse: drag = look/orbit, wheel = zoom. Keys (layout independent): W A S D = left stick, arrows = right stick,
//! Tab = settings panel, F = fullscreen, H = HUD, R = reset camera, Esc = quit.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use raytrace::params::{id, Store, DEFS, N};
use raytrace::presets::{self, KEYS};
use raytrace::{gfx::Gfx, renderer::Renderer};

// ---- settings file (~/.config/raytrace/settings.ini: key=value per line) -------------------------------------------

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("raytrace").join("settings.ini"))
}

fn load_settings() -> Vec<(usize, f32)> {
    let Some(text) = config_path().and_then(|p| std::fs::read_to_string(p).ok()) else { return vec![] };
    text.lines().filter_map(|l| {
        let (k, v) = l.split_once('=')?;
        let i = KEYS.iter().position(|x| *x == k.trim())?;
        Some((i, v.trim().parse::<f32>().ok()?))
    }).collect()
}

fn save_settings(s: &Store) {
    let Some(p) = config_path() else { return };
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let text: String = (0..N).map(|i| format!("{}={}\n", KEYS[i], s.get(i))).collect();
    if let Err(e) = std::fs::write(&p, text) { log::warn!("settings not saved: {e}"); }
}

fn system_is_russian() -> bool {
    if let Ok(l) = std::env::var("RAYTRACE_LANG") { return l.starts_with("ru"); }
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty()).is_some_and(|v| v.starts_with("ru"))
}

// ---- settings panel -------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Tab { Quality, Smoothing, Light, Scene, App }

struct Panel<'a> { r: &'a mut Renderer, ru: bool, changed: bool }

impl Panel<'_> {
    fn t<'s>(&self, en: &'s str, ru: &'s str) -> &'s str { if self.ru { ru } else { en } }

    fn set(&mut self, i: usize, v: f32) { self.r.set_param(i as i32, v); self.changed = true; }

    fn chips(&mut self, ui: &mut egui::Ui, title: &str, i: usize, opts: &[(&str, f32)]) {
        ui.label(egui::RichText::new(title).small().weak());
        ui.horizontal_wrapped(|ui| {
            for (name, v) in opts {
                if ui.selectable_label(self.r.store.get(i) == *v, *name).clicked() { self.set(i, *v); }
            }
        });
        ui.add_space(4.0);
    }

    fn toggle(&mut self, ui: &mut egui::Ui, title: &str, i: usize) {
        let mut on = self.r.store.on(i);
        if ui.checkbox(&mut on, title).changed() { self.set(i, on as u32 as f32); }
    }

    fn stepper(&mut self, ui: &mut egui::Ui, title: &str, i: usize, step: f32, fmt: impl Fn(f32) -> String) {
        let (min, max) = (DEFS[i].min, DEFS[i].max);
        let cur = self.r.store.get(i);
        ui.horizontal(|ui| {
            ui.label(title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(cur < max, egui::Button::new("+")).clicked() { self.set(i, (cur + step).min(max)); }
                ui.label(egui::RichText::new(fmt(cur)).monospace());
                if ui.add_enabled(cur > min, egui::Button::new("−")).clicked() { self.set(i, (cur - step).max(min)); }
            });
        });
    }

    fn onoff(&self) -> [(&'static str, f32); 2] { [(if self.ru { "Выкл" } else { "Off" }, 0.0), (if self.ru { "Вкл" } else { "On" }, 1.0)] }

    fn quality(&mut self, ui: &mut egui::Ui) {
        let cur = presets::detect(|i| self.r.store.get(i));
        ui.label(egui::RichText::new(self.t("Preset", "Пресет")).small().weak());
        let names = [self.t("Performance", "Скорость"), self.t("Balanced", "Баланс"), self.t("Quality", "Качество"), self.t("Custom", "Свой")];
        ui.horizontal_wrapped(|ui| {
            for (k, n) in names.iter().enumerate() {
                if ui.selectable_label(cur == k, *n).clicked() && k < 3 {
                    for (&i, &v) in presets::ORDER.iter().zip(presets::TABLE[k].iter()) { self.r.set_param(i as i32, v); }
                    self.changed = true;
                }
            }
        });
        ui.add_space(4.0);
        let onoff = self.onoff();
        self.chips(ui, self.t("Mode", "Режим"), id::MODE, &[(self.t("Hybrid", "Гибрид"), 0.0), (self.t("Path tracing", "Трассировка путей"), 1.0)]);
        self.chips(ui, self.t("Render scale", "Масштаб рендера"), id::SCALE_IDX, &[("0.25×", 0.0), ("0.33×", 1.0), ("0.5×", 2.0), ("0.75×", 3.0), ("1.0×", 4.0)]);
        self.chips(ui, self.t("Adaptive resolution", "Адаптивное разрешение"), id::ADAPTIVE, &onoff);
        self.chips(ui, self.t("Target FPS", "Целевой FPS"), id::TARGET_FPS, &[("30", 30.0), ("45", 45.0), ("60", 60.0), ("90", 90.0)]);
        self.stepper(ui, self.t("Bounces", "Отражения луча"), id::BOUNCES, 1.0, |v| format!("{}", v as i32));
        self.stepper(ui, self.t("Samples per pixel", "Сэмплов на пиксель"), id::SPP, 1.0, |v| format!("{}", v as i32));
        self.toggle(ui, self.t("PlayStation 1 look", "Стиль PlayStation 1"), id::PS1);
        ui.label(egui::RichText::new(self.t("240 px low-res, hard pixels, 15-bit dithered colour, snapped camera, 12 fps animation; replaces render scale",
            "Разрешение 240 px, жёсткие пиксели, 15-битный цвет с дизерингом, камера по сетке, анимация 12 к/с; заменяет масштаб рендера")).small().weak());
    }

    fn smoothing(&mut self, ui: &mut egui::Ui) {
        self.toggle(ui, self.t("Temporal smoothing", "Временное сглаживание"), id::TEMPORAL);
        self.stepper(ui, self.t("Smoothing strength", "Сила сглаживания"), id::STRENGTH, 1.0, |v| format!("{}", v as i32));
        self.chips(ui, self.t("Denoiser passes", "Проходы денойзера"), id::DENOISE, &[("0", 0.0), ("1", 1.0), ("2", 2.0), ("3", 3.0)]);
        self.chips(ui, self.t("Sharpen", "Резкость"), id::SHARPEN,
            &[(self.t("Off", "Выкл"), 0.0), (self.t("Low", "Низкая"), 1.0), (self.t("Medium", "Средняя"), 2.0), (self.t("High", "Высокая"), 3.0)]);
        self.toggle(ui, self.t("Checkerboard rendering", "Шахматный рендер"), id::CHECKER);
    }

    fn light(&mut self, ui: &mut egui::Ui) {
        self.toggle(ui, self.t("Soft shadows", "Мягкие тени"), id::SHADOWS);
        self.toggle(ui, self.t("Global illumination", "Глобальное освещение"), id::GI);
        self.chips(ui, self.t("GI resolution", "Разрешение GI"), id::GI_RES,
            &[(self.t("Full", "Полное"), 0.0), (self.t("Half", "Половина"), 1.0), (self.t("Quarter", "Четверть"), 2.0)]);
        ui.label(egui::RichText::new(self.t("Applies in hybrid mode with GI on", "Действует в гибридном режиме при включённом GI")).small().weak());
        self.toggle(ui, self.t("Caustics", "Каустики"), id::CAUSTICS);
        self.toggle(ui, self.t("Reflections & refraction", "Отражения и преломление"), id::REFLECTIONS);
        self.stepper(ui, self.t("Light intensity", "Яркость ламп"), id::LIGHT, 1.0, |v| ["0.5×", "0.75×", "1×", "1.5×", "2×"][v as usize - 1].to_string());
        let names = [self.t("Warm", "Тёплый"), self.t("White", "Белый"), self.t("Cyan", "Голубой"), self.t("Pink", "Розовый"), self.t("Green", "Зелёный"), self.t("Orange", "Оранжевый")];
        let opts: Vec<(&str, f32)> = names.iter().enumerate().map(|(k, n)| (*n, k as f32)).collect();
        self.chips(ui, self.t("Light A colour", "Цвет лампы A"), id::COL_A, &opts);
        self.chips(ui, self.t("Light B colour", "Цвет лампы B"), id::COL_B, &opts);
    }

    fn scene(&mut self, ui: &mut egui::Ui) {
        let speeds = ["0.4×", "0.7×", "1×", "1.5×", "2.2×"];
        self.chips(ui, self.t("Camera mode", "Режим камеры"), id::CAM_MODE, &[(self.t("Orbit", "Орбита"), 0.0), (self.t("Fly", "Полёт"), 1.0), (self.t("Helicopter", "Вертолёт"), 2.0)]);
        self.stepper(ui, self.t("Move speed", "Скорость движения"), id::MOVE_SPEED, 1.0, |v| speeds[v as usize - 1].to_string());
        self.stepper(ui, self.t("Look speed", "Скорость взгляда"), id::LOOK_SPEED, 1.0, |v| speeds[v as usize - 1].to_string());
        self.toggle(ui, self.t("Animation", "Анимация"), id::ANIM);
        self.chips(ui, self.t("Auto-orbit (Orbit mode)", "Автовращение (режим «Орбита»)"), id::ORBIT,
            &[(self.t("Off", "Выкл"), 0.0), (self.t("Slow", "Медленно"), 1.0), (self.t("Fast", "Быстро"), 2.0)]);
        self.stepper(ui, self.t("Field of view", "Угол обзора"), id::FOV, 5.0, |v| format!("{}°", v as i32));
        self.stepper(ui, self.t("Exposure", "Экспозиция"), id::EXPOSURE, 1.0, |v| format!("{:+.1} EV", v * 0.5));
        self.chips(ui, self.t("Tonemap", "Тональная кривая"), id::TONEMAP, &[("ACES", 0.0), ("Reinhard", 1.0), (self.t("None", "Нет"), 2.0)]);
        self.chips(ui, self.t("Sky", "Небо"), id::SKY, &[(self.t("Day", "День"), 0.0), (self.t("Dusk", "Закат"), 1.0), (self.t("Night", "Ночь"), 2.0)]);
    }

    fn app(&mut self, ui: &mut egui::Ui) {
        self.chips(ui, "HUD", id::HUD, &[(self.t("Off", "Выкл"), 0.0), ("FPS", 1.0), (self.t("Full", "Полный"), 2.0)]);
        let mut limit = self.r.store.on(id::FRAME_LIMIT);
        if ui.checkbox(&mut limit, self.t("Limit to display refresh (VSync)", "Лимит по частоте экрана (VSync)")).changed() {
            self.set(id::FRAME_LIMIT, limit as u32 as f32);
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new(self.t("Language", "Язык")).small().weak());
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.ru, "English").clicked() { self.ru = false; }
            if ui.selectable_label(self.ru, "Русский").clicked() { self.ru = true; }
        });
        ui.add_space(12.0);
        if ui.add(egui::Button::new(self.t("Reset to defaults", "Сбросить настройки")).fill(egui::Color32::from_rgb(120, 40, 40))).clicked() {
            for (i, d) in DEFS.iter().enumerate() { self.r.set_param(i as i32, d.default); }
            self.changed = true;
        }
        ui.add_space(12.0);
        ui.label(egui::RichText::new(self.t(
            "Mouse: drag to look, wheel to zoom\nW A S D: left stick · arrows: right stick\nTab: panel · F: fullscreen · H: HUD · R: reset camera · Esc: quit",
            "Мышь: перетаскивание — взгляд, колесо — зум\nW A S D — левый стик · стрелки — правый\nTab — панель · F — во весь экран · H — HUD · R — сброс камеры · Esc — выход")).small().weak());
    }
}

fn hud_text(st: &[f32; 16], level: u32, ru: bool) -> Option<String> {
    if level == 0 || st[0] <= 0.0 { return None; }
    let fps = format!("{:.0} fps", st[0]);
    if level == 1 { return Some(fps); }
    let mode = if st[5] > 0.5 { if ru { "путь" } else { "path" } } else if ru { "гибрид" } else { "hybrid" };
    let mut s = format!("{fps} · {:.1} ms · {}×{} · {mode}", st[1], st[2] as u32, st[3] as u32);
    if st[8..16].iter().any(|v| *v > 0.0) {
        let n = ["tr", "gi", "gt", "ga", "tm", "at", "cm", "pr"];
        s.push('\n');
        s.push_str(&(0..8).map(|i| format!("{} {:.1}", n[i], st[8 + i])).collect::<Vec<_>>().join(" · "));
    }
    Some(s)
}

// ---- application ------------------------------------------------------------------------------------------------------

struct State {
    window: Arc<Window>,
    r: Renderer,
    stats: Arc<Mutex<[f32; 16]>>,
    ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_rend: egui_wgpu::Renderer,
    keys: HashSet<KeyCode>,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    panel_open: bool,
    tab: Tab,
    ru: bool,
}

struct App { state: Option<State> }

impl State {
    fn new(el: &ActiveEventLoop) -> State {
        let attrs = Window::default_attributes().with_title("Raytrace").with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..Default::default() });
        let surface = instance.create_surface(window.clone()).expect("create_surface");
        let gfx = Gfx::from_surface(instance, surface, Box::new(window.clone()), w, h);
        let format = gfx.format();
        let egui_rend = egui_wgpu::Renderer::new(gfx.device(), format, None, 1, false);
        let stats = Arc::new(Mutex::new([0.0f32; 16]));
        let mut r = Renderer::new(gfx, (w, h), stats.clone());
        for (i, v) in load_settings() { r.set_param(i as i32, v); }
        let ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, &window, Some(window.scale_factor() as f32), None, None);
        State { window, r, stats, ctx, egui_state, egui_rend, keys: HashSet::new(), dragging: false, last_cursor: None, panel_open: true, tab: Tab::Quality, ru: system_is_russian() }
    }

    fn held(&self, k: KeyCode) -> f32 { self.keys.contains(&k) as u32 as f32 }

    fn key_pressed(&mut self, el: &ActiveEventLoop, k: KeyCode) {
        match k {
            KeyCode::Escape => { save_settings(&self.r.store); el.exit(); }
            KeyCode::Tab => self.panel_open = !self.panel_open,
            KeyCode::KeyR => self.r.cam.reset(),
            KeyCode::KeyH => {
                let v = (self.r.store.get(id::HUD) + 1.0) % 3.0;
                self.r.set_param(id::HUD as i32, v);
                save_settings(&self.r.store);
            }
            KeyCode::KeyF => {
                let fs = if self.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) };
                self.window.set_fullscreen(fs);
            }
            _ => {}
        }
    }

    fn event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let resp = self.egui_state.on_window_event(&self.window, &ev);
        if resp.repaint { self.window.request_redraw(); }
        match ev {
            WindowEvent::CloseRequested => { save_settings(&self.r.store); el.exit(); }
            WindowEvent::Resized(s) => self.r.resize(s.width, s.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if !event.repeat && !resp.consumed { self.key_pressed(el, k); }
                        self.keys.insert(k);
                    } else {
                        self.keys.remove(&k);
                    }
                }
            }
            WindowEvent::Focused(false) => { self.keys.clear(); self.dragging = false; }
            WindowEvent::MouseInput { state, button: MouseButton::Left | MouseButton::Right, .. } => {
                self.dragging = state == ElementState::Pressed && !resp.consumed && !self.ctx.is_pointer_over_area();
                if !self.dragging { self.last_cursor = None; }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.dragging {
                    if let Some((lx, ly)) = self.last_cursor { self.r.cam.orbit((position.x - lx) as f32, (position.y - ly) as f32); }
                    self.last_cursor = Some((position.x, position.y));
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !resp.consumed => {
                let y = match delta { MouseScrollDelta::LineDelta(_, y) => y, MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0 };
                self.r.cam.zoom(0.9f32.powf(y));
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn redraw(&mut self) {
        self.r.sticks = [
            self.held(KeyCode::KeyD) - self.held(KeyCode::KeyA), self.held(KeyCode::KeyW) - self.held(KeyCode::KeyS),
            self.held(KeyCode::ArrowRight) - self.held(KeyCode::ArrowLeft), self.held(KeyCode::ArrowUp) - self.held(KeyCode::ArrowDown),
        ];
        let st = *self.stats.lock().unwrap();
        let hud = hud_text(&st, self.r.store.get(id::HUD) as u32, self.ru);
        let (open, mut tab, mut ru) = (self.panel_open, self.tab, self.ru);
        let mut changed = false;
        let raw = self.egui_state.take_egui_input(&self.window);
        let out = self.ctx.run(raw, |ctx| {
            if let Some(h) = &hud {
                egui::Area::new("hud".into()).anchor(egui::Align2::LEFT_TOP, [10.0, 10.0]).interactable(false).show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| { ui.label(egui::RichText::new(h).monospace()); });
                });
            }
            if open {
                egui::SidePanel::right("settings").exact_width(340.0).resizable(false).show(ctx, |ui| {
                    let mut p = Panel { r: &mut self.r, ru, changed: false };
                    ui.horizontal_wrapped(|ui| {
                        let tabs = [(Tab::Quality, "Quality", "Качество"), (Tab::Smoothing, "Smoothing", "Сглаживание"), (Tab::Light, "Light", "Свет"),
                            (Tab::Scene, "Scene", "Сцена"), (Tab::App, "App", "Прил.")];
                        for (t, en, rus) in tabs { if ui.selectable_label(tab == t, p.t(en, rus)).clicked() { tab = t; } }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| match tab {
                        Tab::Quality => p.quality(ui),
                        Tab::Smoothing => p.smoothing(ui),
                        Tab::Light => p.light(ui),
                        Tab::Scene => p.scene(ui),
                        Tab::App => p.app(ui),
                    });
                    changed = p.changed;
                    ru = p.ru;
                });
            }
        });
        self.tab = tab;
        self.ru = ru;
        if changed { save_settings(&self.r.store); }
        self.egui_state.handle_platform_output(&self.window, out.platform_output);

        let ppp = out.pixels_per_point;
        let jobs = self.ctx.tessellate(out.shapes, ppp);
        let size = self.window.inner_size();
        let sd = egui_wgpu::ScreenDescriptor { size_in_pixels: [size.width.max(1), size.height.max(1)], pixels_per_point: ppp };
        let rend = &mut self.egui_rend;
        let mut overlay = |dev: &wgpu::Device, queue: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView| {
            for (tid, delta) in &out.textures_delta.set { rend.update_texture(dev, queue, *tid, delta); }
            let extra = rend.update_buffers(dev, queue, enc, &jobs, &sd);
            if !extra.is_empty() { queue.submit(extra); }
            {
                let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
                }).forget_lifetime();
                rend.render(&mut rp, &jobs, &sd);
            }
            for tid in &out.textures_delta.free { rend.free_texture(tid); }
        };
        self.r.frame_with(Some(&mut overlay));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_none() { self.state = Some(State::new(el)); }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        if let Some(s) = self.state.as_mut() { s.event(el, ev); }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(s) = &self.state { s.window.request_redraw(); }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut App { state: None }).expect("run");
}
