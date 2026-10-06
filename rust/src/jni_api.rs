//! JNI entry points (dev.starinin.raytrace.Native) and the render thread.
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use jni::objects::{JClass, JObject};
use jni::sys::{jfloat, jfloatArray, jint};
use jni::JNIEnv;
use ndk::native_window::NativeWindow;

use crate::{gfx::Gfx, renderer::Renderer};

pub enum Cmd { Param(i32, f32), Orbit(f32, f32), Sticks(f32, f32, f32, f32), Zoom(f32), ResetCam, Resize(u32, u32), Stop }

struct Engine { tx: Sender<Cmd>, handle: Option<JoinHandle<()>>, stats: Arc<Mutex<[f32; 8]>> }

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn send(c: Cmd) {
    if let Some(e) = ENGINE.lock().unwrap().as_ref() { let _ = e.tx.send(c); }
}

#[no_mangle]
pub extern "system" fn Java_dev_starinin_raytrace_Native_start(env: JNIEnv, _c: JClass, surface: JObject, w: jint, h: jint) {
    crate::init_logger();
    stop_engine();
    let win = unsafe { NativeWindow::from_surface(env.get_raw() as *mut _, surface.as_raw() as *mut _) };
    let Some(win) = win else { log::error!("ANativeWindow_fromSurface failed"); return; };
    let (tx, rx) = channel::<Cmd>();
    let stats = Arc::new(Mutex::new([0.0f32; 8]));
    let st = stats.clone();
    let st2 = stats.clone();
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    let handle = std::thread::Builder::new().name("raytrace".into()).spawn(move || {
        // Unwinding panics (e.g. wgpu after a GPU watchdog reset, or Gfx::new failing) must end
        // only this thread, not the process; stats are zeroed when the thread ends so the HUD shows 0 fps, not stale values.
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        log::info!("render thread: creating Gfx");
        let gfx = Gfx::new(win, w, h);
        let mut r = Renderer::new(gfx, (w, h), st);
        'run: loop {
            loop {
                match rx.try_recv() {
                    Ok(Cmd::Stop) => break 'run,
                    Ok(Cmd::Param(i, v)) => r.set_param(i, v),
                    Ok(Cmd::Orbit(dx, dy)) => r.cam.orbit(dx, dy),
                    Ok(Cmd::Sticks(a, b, c, d)) => r.sticks = [a, b, c, d],
                    Ok(Cmd::Zoom(f)) => r.cam.zoom(f),
                    Ok(Cmd::ResetCam) => r.cam.reset(),
                    Ok(Cmd::Resize(w, h)) => r.resize(w, h),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break 'run,
                }
            }
            r.frame();
        }
        }));
        *st2.lock().unwrap() = [0.0; 8];
        if let Err(e) = res {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            log::error!("render thread panicked (GPU init failure or device loss), rendering stopped: {msg}");
        }
    }).expect("spawn");
    *ENGINE.lock().unwrap() = Some(Engine { tx, handle: Some(handle), stats });
}

// Called from the UI thread (surfaceDestroyed). The join must stay: the native window is
// released right after, so the render thread must be gone first. Frame time is bounded by the
// safety governor, and a panicking render thread exits immediately (catch_unwind).
fn stop_engine() {
    let e = ENGINE.lock().unwrap().take();
    if let Some(mut e) = e {
        let _ = e.tx.send(Cmd::Stop);
        if let Some(h) = e.handle.take() { let _ = h.join(); }
    }
}

#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_stop(_e: JNIEnv, _c: JClass) { stop_engine(); }
#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_resize(_e: JNIEnv, _c: JClass, w: jint, h: jint) { send(Cmd::Resize(w.max(1) as u32, h.max(1) as u32)); }
#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_setParam(_e: JNIEnv, _c: JClass, id: jint, v: jfloat) { send(Cmd::Param(id, v)); }
#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_orbit(_e: JNIEnv, _c: JClass, dx: jfloat, dy: jfloat) { send(Cmd::Orbit(dx, dy)); }
#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_sticks(_e: JNIEnv, _c: JClass, lx: jfloat, ly: jfloat, rx: jfloat, ry: jfloat) { send(Cmd::Sticks(lx, ly, rx, ry)); }
#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_zoom(_e: JNIEnv, _c: JClass, f: jfloat) { send(Cmd::Zoom(f)); }
#[no_mangle] pub extern "system" fn Java_dev_starinin_raytrace_Native_resetCamera(_e: JNIEnv, _c: JClass) { send(Cmd::ResetCam); }

#[no_mangle]
pub extern "system" fn Java_dev_starinin_raytrace_Native_stats(env: JNIEnv, _c: JClass) -> jfloatArray {
    let s = ENGINE.lock().unwrap().as_ref().map(|e| *e.stats.lock().unwrap()).unwrap_or([0.0; 8]);
    let arr = env.new_float_array(8).unwrap();
    env.set_float_array_region(&arr, 0, &s).unwrap();
    arr.into_raw()
}
