# Raytrace 1.0 — design

Date: 2026-10-06. Target device: POCO F3 (Adreno 650, Vulkan, no hardware RT). Build: Termux, offline.

## Goal
Take the prototype (single compute shader, SDF scene, hybrid + path-tracing modes, ~no UI, very low fps)
and ship a fast, good-looking app with a rich settings UI built on the user's `android-ui-kit`,
published as a public GitHub repo with release v1.0.0.

Success criteria
- Visibly less noise and no smearing while the camera moves (temporal reprojection + denoise).
- Higher fps than the prototype at equal visual quality; adaptive resolution holds a target fps.
- Settings UI in M3 Expressive (kit), no sliders; chips, switches, steppers.
- Builds offline in Termux with `./gradlew assembleRelease` (+ cargo step); signed with `~/.android/debug.keystore`.

Non-goals: hardware RT, new scenes, scene editor, F-Droid submission (later, separate task).

## Assumptions (to be confirmed by review)
- Repo name `Xtratter/raytrace`, public, Apache-2.0, EN + RU docs.
- Package `dev.starinin.raytrace` is kept (as in the prototype), minSdk 28, targetSdk 34.

## Architecture
```
app/ (Gradle, Kotlin)            rust/ (cargo, cdylib libraytrace.so)
  MainActivity                     lib.rs      JNI entry, render thread, param store
  RenderView : SurfaceView  --->   gfx.rs      wgpu device/surface, passes, buffers
  SettingsSheet (kit UI)           shaders/    trace.wgsl, temporal.wgsl, atrous.wgsl, present.wgsl
  Settings (SharedPreferences)     params.rs   Params table (id -> value), presets
  uikit/ (copy via install.sh)
```
- Kotlin owns window, input, UI, persistence. Rust owns the GPU and runs its own render thread.
- JNI surface: `init(Surface)`, `resize(w,h)`, `destroy()`, `setParam(id:Int, v:Float)`,
  `touch(...)` / `orbit(dx,dy)` / `zoom(f)`, `stats(): FloatArray` (fps, ms, render w/h, scale, history).
- Rust builds with `ndk`-free raw `ANativeWindow` handle (`raw-window-handle`), `wgpu` Vulkan only.
  `winit`, `android-activity`, `NativeActivity` are removed.
- Build: `tools/build-native.sh` runs cargo (aarch64-linux-android native in Termux) and copies the `.so`
  into `app/src/main/jniLibs/arm64-v8a/`; Gradle `preBuild` calls it. Release profile: opt-level 3, fat LTO, codegen-units 1, panic=abort, strip.

## Render pipeline (per frame, internal resolution = scale x screen)
1. **trace** (compute): primary ray, outputs radiance (rgba16f) + gbuffer (depth, octahedral normal, material id)
   into storage textures. Camera jitter (Halton) for temporal AA.
2. **temporal** (compute): reproject with previous view-projection from gbuffer depth; validate by depth/normal/id
   thresholds; clamp history to the 3x3 neighbourhood color AABB (variance clip); blend factor from history length
   (alpha = max(1/n, floor)); history length resets on invalid reprojection. Path-tracing mode on a still camera
   uses pure 1/n accumulation (converges); on motion it falls back to reprojection.
3. **a-trous** (compute, 0-3 iterations): edge-stopping on depth, normal and luminance variance.
4. **present** (render): Catmull-Rom upscale to screen, optional CAS-style sharpen, exposure, tonemap
   (ACES / Reinhard / none), dithering.
5. Ping-pong history textures; all textures recreated on resolution change only.

Shader performance work
- Bounding spheres for the Menger sponge and torus (skip detailed SDF when the ray is far).
- Fewer march steps and looser epsilon for secondary and shadow rays; early-out on shadow hit.
- Optional checkerboard: trace half of the pixels per frame, temporal fills the rest.
- Adaptive resolution controller (Rust): target fps, EMA of frame time, step through scale with hysteresis
  (change at most once per 0.5 s, snap to multiples of 8 px).
- Present mode FIFO by default; optional Mailbox/unlimited when "frame limit" is off.

## Settings (id -> UI)
Quality tab
- Preset: Performance / Balanced / Quality / Custom (sets many values below)
- Mode: Hybrid / Path tracing
- Render scale: 0.25 0.33 0.5 0.75 1.0 (chips); Adaptive resolution on/off; Target fps 30/45/60/90
- Bounces (stepper 1-9), Samples per pixel (1-4)

Smoothing tab
- Temporal on/off; strength (stepper 1-5); Denoiser passes (0-3); Sharpen (off/low/med/high); Checkerboard on/off

Light tab
- Soft shadows, Global illumination, Caustics, Reflections/refraction (switches); Light intensity (stepper); Light A / B colour (chips)

Scene tab
- Animation on/off; Auto-orbit (off/slow/fast); FOV (stepper); Exposure (stepper); Tonemap; Sky (day/dusk/night)

App tab
- Theme (System/Light/Dark/Graphite/AMOLED); Haptics (kit); HUD (off/fps/full); Frame limit; Reset to defaults

Each setting has a stable integer id shared by Kotlin and Rust (one table in `params.rs`, mirrored in `Settings.kt`;
a unit test on each side checks the id list matches a shared `params.json`).

## UI
- Full-screen `RenderView`; top status island (fps - resolution - mode), tap = toggle HUD detail.
- Gear button opens a bottom sheet (M3Dialog-style, soft corners, EdgeBlur on scroll edges) with tab chips.
- Controls: `M3Widgets.chip` groups, `switchRow`, stepper (round -/+ buttons with value), no sliders.
- Gestures: one-finger orbit, pinch zoom, double-tap resets camera. Haptics on chips via kit, Help bubbles on long press.
- Theme via `M3.Mode`; kit copied with `install.sh` (kit repo is not modified for this project).

## Error handling
- Vulkan unavailable / device lost: show a full-screen message with the adapter log; try re-init once on surface recreation.
- Surface lost/outdated: reconfigure; on `suspended`/`destroy` drop all GPU resources, recreate on `surfaceCreated`.
- Param ids unknown to Rust are ignored and logged; out-of-range values clamped.
- Adaptive resolution never goes below 0.25 or above the user's chosen max.

## Testing
- Rust unit tests: param table/clamping/presets, adaptive-resolution controller, Halton sequence, reprojection math (CPU reference).
- Kotlin unit tests: Settings defaults/migration, preset application, ids match `params.json`.
- On-device (root): install, launch, `screencap` per preset, `logcat -s raytrace` fps; before/after numbers recorded in README.
- Build verification: `./gradlew testDebugUnitTest assembleRelease`, `apksigner verify`.

## Release
Public repo `Xtratter/raytrace`: README(.ru), CHANGELOG(.ru), LICENSE (Apache-2.0), `.github/workflows/build.yml` like other projects,
fastlane metadata, tag v1.0.0 via `~/projects/tools/release.sh`, APK in `/sdcard/claude/builds/raytrace`. Source prototype zip is credited in README only if the user wants.

## Out of scope / later
F-Droid MR, more scenes, screenshot/share button, ReSTIR-style resampling.
