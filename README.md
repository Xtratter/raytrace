# Raytrace

[Русский](README.ru.md) · **English**

A realtime ray tracer for Android. A signed-distance-field scene (mirror and glass spheres, a torus, a Menger sponge,
two glowing lamps, a checkered room) is rendered on the GPU with **Rust + wgpu (Vulkan)**, then cleaned up by
**temporal reprojection** and an **a-trous denoiser**, with **adaptive resolution** to hold a target frame rate.
The settings UI is Material 3 Expressive, built on [android-ui-kit](https://github.com/Xtratter/android-ui-kit).
The interface is bilingual (English and Russian, follows the system language); the screenshots below were taken with the Russian locale.

## Screenshots

| Scene (Balanced, still) | Quality tab | Light tab |
|:---:|:---:|:---:|
| <img src="docs/img/scene.png" width="240"> | <img src="docs/img/settings-quality.png" width="240"> | <img src="docs/img/settings-light.png" width="240"> |

## Features

- **Two modes**: *Hybrid* (one ray per pixel with shadows, global illumination, caustics, reflections and refraction) and
  *Path tracing* (progressive: converges to a clean image when the camera is still)
- **Temporal reprojection**: history is reprojected with the previous camera, validated by depth, normal and material id,
  clamped to the neighbourhood colour range and blended by history length; sub-pixel jitter (Halton) gives anti-aliasing
- **Half-resolution GI**: global illumination is traced in its own pass at half or quarter resolution, accumulated temporally, denoised with a-trous and bilateral-upsampled to the full image
- **Per-pass GPU timings** in the HUD "Full" mode (when the driver supports timestamp queries)
- **A-trous denoiser**: 0-3 edge-stopping passes (depth, normal, luminance), checker edges are preserved
- **Safety governor**: heavy settings (e.g. path tracing at 1.0x with many samples and bounces) are scaled down automatically when a frame takes more than ~1.2 s, then recover slowly
- **Adaptive resolution**: pick a target fps (30 / 45 / 60 / 90); the render scale moves between 0.25x and your chosen maximum
  with hysteresis (at most one change per half second)
- **Present**: Catmull-Rom upscale, optional sharpening, exposure, tonemapping (ACES / Reinhard / none), dithering
- **Presets**: Performance, Balanced, Quality (and Custom once you change anything)
- **Many switches**: soft shadows, global illumination, caustics, reflections and refraction, lamp brightness and colours,
  animation, auto-orbit, field of view, exposure, tonemap, sky (day / dusk / night)
- **Two camera modes**: orbit (default) and free-fly, driven by touch or by two on-screen gamepad sticks
- **Material 3 Expressive UI** without sliders: chips, switches, steppers; themes (system, light, dark, graphite, AMOLED),
  haptics, soft blurred edges, a status island with fps / frame time / resolution

## Controls

| Gesture | Action |
|---|---|
| One finger drag | Orbit the camera |
| Pinch | Zoom |
| Left stick | Move (fly mode) / zoom (orbit mode); up = forward / zoom in |
| Right stick | Look (fly mode) / orbit (orbit mode); up = look up |
| Double tap | Reset the camera |
| Gear button | Open the settings sheet |
| Tap the status island | Toggle HUD detail |

## Settings

| Tab | Setting | Values |
|---|---|---|
| Quality | Preset | Performance / Balanced / Quality / Custom |
| | Mode | Hybrid / Path tracing |
| | Render scale | 0.25x, 0.33x, 0.5x, 0.75x, 1.0x |
| | Adaptive resolution | off / on |
| | Target fps | 30, 45, 60, 90 |
| | Ray bounces | 1-9 |
| | Samples per pixel | 1-4 |
| Smoothing | Temporal accumulation | off / on |
| | Temporal strength | 1-5 |
| | Denoiser passes | 0-3 |
| | Sharpen | off / low / medium / high |
| | Checkerboard | off / on (traces half of the pixels per frame) |
| Light | Soft shadows, Global illumination, Caustics, Reflections and refraction | off / on |
| | GI resolution | Full / Half / Quarter (applies only in hybrid mode with GI on; presets: Performance has GI off so the resolution is irrelevant, Balanced = Half, Quality = Full) |
| | Lamp brightness | 1-5 |
| | Lamp A / B colour | six colours each |
| Scene | Animation | off / on |
| | Auto-orbit | off / slow / fast |
| | Camera mode | orbit / fly |
| | On-screen sticks | off / on |
| | Move speed | 1-5 |
| | Look speed | 1-5 |
| | Field of view | 40-90 |
| | Exposure | -4..+4 |
| | Tonemap | ACES / Reinhard / none |
| | Sky | day / dusk / night |
| App | Theme | system / light / dark / graphite / AMOLED |
| | HUD | off / fps / full |
| | Frame limit | off / on (Fifo vs Mailbox present mode) |
| | Reset to defaults | |

## Architecture

```
app/ (Gradle, Kotlin)            rust/ (cargo, cdylib libraytrace.so)
  MainActivity                     lib.rs      logger + module wiring
  RenderView : SurfaceView  --->   jni_api.rs  JNI entry points, render thread
  SettingsSheet (kit UI)           renderer.rs frame loop, adaptive resolution, camera
  Settings (SharedPreferences)     gfx.rs      wgpu device/surface, passes, buffers
  uikit/ (android-ui-kit)          shaders/    scene helpers, trace, gi_trace, temporal, gi_temporal, gi_atrous, atrous, composite, present (WGSL)
                                   params.rs   id -> value table, clamping, presets
```

Per frame (internal resolution = scale x screen): **trace** (compute: radiance + depth / normal / material id, jittered) ->
**gi_trace** (indirect light, half or quarter resolution only) -> **temporal** (reproject, validate, clamp, blend, ping-pong history) ->
**gi_temporal** (GI history) -> **gi_atrous** (GI denoise) -> **a-trous** (0-3 passes) -> **composite** (bilateral GI upsample + add) -> **present**
(gi_trace, gi_temporal, gi_atrous and composite run only in the split case: hybrid mode, GI on, resolution Half or Quarter. With Full, GI is computed inline in the trace pass as in 1.1; with GI off nothing is deferred; in path-tracing mode GI always stays inline.)

(upscale, sharpen, tonemap, dither). Kotlin owns the window, input, UI and persistence; Rust owns the GPU and its own render thread.
Setting ids are shared by both sides through `params.json` (a unit test on each side checks the lists match).
`shaders.rs` assembles the WGSL (`common.wgsl` + a pass) and a unit test validates every shader with naga on the host.

## Build

Requires an arm64 Android device (Vulkan 1.1), minSdk 28.

**Termux (native build on the phone)**
```bash
pkg install rust clang zip openjdk-17 ...   # plus the Android SDK as in termux-android-build
./gradlew assembleRelease                    # Gradle runs tools/build-native.sh (cargo) first
```
The unsigned APK is `app/build/outputs/apk/release/app-release-unsigned.apk`; sign it with `apksigner`.

**PC (Linux)**
```bash
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=...   # NDK r26+
export TARGET=aarch64-linux-android
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android28-clang"
export CC_aarch64_linux_android="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
./gradlew testDebugUnitTest assembleRelease
(cd rust && cargo test --lib)   # host unit tests, no device needed
```

## Linux desktop (x86-64)

The same renderer runs on desktop Linux (tested target: Manjaro / Arch, any Vulkan 1.1 GPU) as `raytrace-desktop`: a winit window with an
egui settings panel (same tabs, presets and languages as the Android app; the language follows `LANG`, override with `RAYTRACE_LANG=en|ru`).
Settings are saved to `~/.config/raytrace/settings.ini`.

- **Mouse**: drag to look/orbit, wheel to zoom. **Keys** (layout independent): W A S D = left stick, arrows = right stick, Tab = panel, F = fullscreen, H = HUD, R = reset camera, Esc = quit
- **Install**: download `raytrace-<version>-linux-x86_64.tar.gz` or the `Raytrace-<version>-x86_64.AppImage` from the Releases page, or build the package: `cd packaging/arch && makepkg -si`
- **Run-time dependencies**: a Vulkan driver (`vulkan-radeon`, `vulkan-intel` or `nvidia-utils`), `vulkan-icd-loader`, `libxkbcommon-x11` (X11) or `wayland`
- **From source**: `cd rust && cargo run --release --features desktop --bin raytrace-desktop`
- CI builds the binary on Ubuntu, runs the unit tests and smoke-runs it on software Vulkan (lavapipe) under Xvfb; performance on real desktop GPUs has not been measured yet

## Benchmarks

Measured on a **POCO F3 (Adreno 650)**, 120 Hz display, 1080x2400. Details and history are in [docs/bench.md](docs/bench.md).

**Final check (06.10.2026, release build, clean defaults):** Balanced preset, hybrid, adaptive target 60 at the 0.25x floor (264x600 render size) reaches
**about 22 fps (GPU 42-44 ms)**, repeated on several runs. The 30 fps target was **not** met. A debug build with a fixed camera and animation off
measured 17.7 fps / 55 ms in the same session. GPU clock and thermal state shift results by roughly 25% between sessions.

The table below comes from **earlier development runs** (cooler device, debug overrides, before the exact-march Menger shadow fix) and was **not re-measured**
in the final check; treat it as approximate. In particular, an earlier Balanced figure of about 40 fps / 25 ms could not be reproduced on clean defaults.

| Preset | Mode | GPU ms | fps | Render scale |
|---|---|---:|---:|---:|
| Performance | hybrid | ~12 | ~79 | 0.25 |
| Performance | path tracing | ~24 | ~41 | 0.25 |
| Balanced | hybrid | 42-44 (final check) | ~22 (final check) | 0.25 |
| Balanced | path tracing | ~182 | ~5.5 | 0.25 |
| Quality (spp 2, 9 bounces) | hybrid | ~164 | ~6.0 | 0.50 |
| Quality | path tracing | ~1674 | ~0.6 | 0.50 |

For comparison, the prototype this app grew out of ran at **7.1 fps** (356x767, 0.33x, hybrid). On clean defaults Balanced is therefore about 3x faster
(22.5 vs 7.1 fps), at a lower internal resolution (0.25x vs 0.33x) but with better image quality (temporal reprojection, denoiser). Path tracing is a
**progressive** mode, meant for a still camera. The safety governor scales heavy settings down automatically. The Menger-sponge shadow step budget
(`MENGER_SHADOW_STEPS = 24`) and the exact-march variant made no measurable difference in the final check (22.4 vs 22.5 fps).

**1.2 (half-resolution GI)**, A/B on the same device, back to back (hybrid, fixed 0.33x, GI on): Full 10.4 fps / GPU 92.7-97.3 ms; Half 14.9-15.4 fps / 62.5-65.5 ms (+46% fps, about -33% GPU time); Quarter 15.7-16.1 fps / 58.9-60.5 ms (+53% fps); GI off 19.3 fps / 51.5 ms. Half stays within about 1% of Full in mean colour of fixed image regions (Menger cube face about 2%). The trace pass is now the main cost (about 52 ms of about 62 ms at Half). Clean defaults on the release build (Balanced, 0.25x floor): 24-25 fps, GPU 36-38 ms, measured in a different session than the 1.1.0 figure, so no speedup is claimed. Details in [docs/bench.md](docs/bench.md).

**1.3 (analytic shadow rays)**, same device and session as the 1.2.0 baseline (hybrid, Half GI, fixed 0.33x, bounces 6): 1.2.0 15.4 fps / GPU 62.7 ms / trace pass 52.5 ms; 1.3.0 20.8 and 20.4 fps / GPU 47.2 and 44.1 ms / trace pass 36.7 and 33.5 ms (about +35% fps, about -28% GPU time, trace pass about -30%). With the new Balanced default of 4 bounces: 20.7 fps / 45.4 ms (bounces 6 -> 4 gives only about 2 ms here; the shadow change is the real win). Shadows off (control): 24.3 fps / 37.3 ms, so shadows still cost about 10 ms of the trace pass (about 24 ms in 1.2.0). Mean colour of fixed image regions stays within about 1-2% of 1.2.0 (slightly darker, exact shadows). The release build with 1.3.0 clean defaults was not measured. Details in [docs/bench.md](docs/bench.md).

## Known limitations

- Reflections lag slightly in fast motion (reprojection follows the mirror surface, not the reflected object)
- Menger sponge rotation can leave ghosting that the neighbourhood clamp only partly removes
- Global illumination and shadows use approximations of the Menger sponge (a bounding box / analytic spheres for secondary rays), with some blue speckle and tint on the floor and spheres
- GI of the rotating Menger sponge lags a few frames behind the motion
- GI hit shadows are approximate
- Shadows of the blob pair use two slightly enlarged spheres, so the smooth neck between them casts a slightly thin shadow
- Short bounded marches for the Menger sponge and torus shadows can leak light on grazing rays when the step budget runs out
- Quarter-resolution GI can look softer
- GI keeps its own temporal accumulation (about 7 frames) even when "Temporal smoothing" is switched off
- GI of fast-moving geometry and silhouettes may smear or halo slightly; no halos or leaks were seen at Half with a still camera, but moving-camera quality, Quarter softness and the sponge GI lag were not specifically verified on a device
- Dotted lines along the room wall edges are visible and were not investigated
- Very heavy settings are scaled down automatically by a safety governor (lower resolution, 1 sample per pixel) until frames get fast again, so the image can look coarser than the chosen scale
- Fly mode is clamped to the room box (x ±5.5, y 0.3..8, z -6.5..12) and has no collision with objects
- The on-screen sticks are covered by unit tests only; the stick and touch feel remain untested by the maintainer
- Needs Vulkan 1.1 and an Adreno-class GPU; arm64 only
- No Vulkan-unavailable screen: the app logs the error and shows a black surface

## License

Apache-2.0, see [LICENSE](LICENSE).
