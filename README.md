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
- **A-trous denoiser**: 0-3 edge-stopping passes (depth, normal, luminance), checker edges are preserved
- **Adaptive resolution**: pick a target fps (30 / 45 / 60 / 90); the render scale moves between 0.25x and your chosen maximum
  with hysteresis (at most one change per half second)
- **Present**: Catmull-Rom upscale, optional sharpening, exposure, tonemapping (ACES / Reinhard / none), dithering
- **Presets**: Performance, Balanced, Quality (and Custom once you change anything)
- **Many switches**: soft shadows, global illumination, caustics, reflections and refraction, lamp brightness and colours,
  animation, auto-orbit, field of view, exposure, tonemap, sky (day / dusk / night)
- **Material 3 Expressive UI** without sliders: chips, switches, steppers; themes (system, light, dark, graphite, AMOLED),
  haptics, soft blurred edges, a status island with fps / frame time / resolution

## Controls

| Gesture | Action |
|---|---|
| One finger drag | Orbit the camera |
| Pinch | Zoom |
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
| | Lamp brightness | 1-5 |
| | Lamp A / B colour | six colours each |
| Scene | Animation | off / on |
| | Auto-orbit | off / slow / fast |
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
  uikit/ (android-ui-kit)          shaders/    trace, temporal, atrous, present (WGSL)
                                   params.rs   id -> value table, clamping, presets
```

Per frame (internal resolution = scale x screen): **trace** (compute: radiance + depth / normal / material id, jittered) ->
**temporal** (reproject, validate, clamp, blend, ping-pong history) -> **a-trous** (0-3 passes) -> **present**
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

## Benchmarks

Measured on a **POCO F3 (Adreno 650)**, 120 Hz display, 1080x2400. Fresh process, 12 s, last log line; the scale is what the adaptive controller settled on
(Quality uses a fixed 0.5x). Details and history are in [docs/bench.md](docs/bench.md).

| Preset | Mode | GPU ms | fps | Render scale |
|---|---|---:|---:|---:|
| Performance | hybrid | 12.0 | 79 | 0.25 |
| Performance | path tracing | 23.9 | 41 | 0.25 |
| Balanced | hybrid | 25.0 | 40 | 0.25 |
| Balanced | path tracing | 182 | 5.5 | 0.25 |
| Quality (spp 2, 9 bounces) | hybrid | 164 | 6.0 | 0.50 |
| Quality | path tracing | 1674 | 0.6 | 0.50 |

For comparison, the prototype this app grew out of ran at **7.1 fps** (0.33x, hybrid). At a fixed 0.33x the Balanced hybrid mode went from
9.0 fps to 17 fps; the prototype-era path tracing run was 0.9 fps and is now 5.5 fps. Path tracing is a **progressive** mode (about 5 fps at the
0.25x floor in Balanced), meant for a still camera. Balanced does not reach 30 fps at a fixed 0.33x; it does at 0.25x.

## Known limitations

- Reflections lag slightly in fast motion (reprojection follows the mirror surface, not the reflected object)
- Menger sponge rotation can leave ghosting that the neighbourhood clamp only partly removes
- Global illumination and shadows use approximations of the Menger sponge (a bounding box / analytic spheres for secondary rays), with some blue speckle and tint on the floor and spheres
- Dotted lines along the room wall edges are visible and were not investigated
- Needs Vulkan 1.1 and an Adreno-class GPU; arm64 only
- No Vulkan-unavailable screen: the app logs the error and shows a black surface

## License

Apache-2.0, see [LICENSE](LICENSE).
