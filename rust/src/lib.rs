pub mod adaptive;
pub mod camera;
pub mod halton;
pub mod params;
pub mod reproj;
pub mod gfx;
pub mod jni_api;
pub mod renderer;

/// Android logcat logger (tag "raytrace") + panic hook that logs instead of dying silently.
pub fn init_logger() {
    #[cfg(target_os = "android")]
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("raytrace"));
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| std::panic::set_hook(Box::new(|i| log::error!("panic: {i}"))));
}
