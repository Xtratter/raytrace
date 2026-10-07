# Changelog

[Русский](CHANGELOG.ru.md) · **English**

## 1.2.0 - half-resolution global illumination

- Added: GI resolution setting (Full / Half / Quarter) on the Light tab; presets set it (Performance = Quarter, Balanced = Half, Quality = Full)
- Added: half/quarter-resolution GI pass with its own temporal accumulation, a-trous denoise and bilateral upsampling
- Added: per-pass GPU timings in the HUD "Full" mode (when timestamp queries are supported)
- Changed: default GI is half resolution

## 1.1.0 - on-screen sticks and fly camera

- Added: two on-screen gamepad sticks (left = move / zoom, right = look / orbit)
- Added: free-fly camera mode alongside the orbit camera
- Added: four Scene tab settings - camera mode, on-screen sticks, move speed, look speed

## 1.0.0 - initial release

- SDF scene rendered with Rust + wgpu (Vulkan): hybrid and progressive path-tracing modes
- Temporal reprojection with depth / normal / id validation, neighbourhood clamp and Halton jitter
- A-trous denoiser (0-3 passes), Catmull-Rom upscale, sharpening, exposure, tonemapping, dithering
- Adaptive resolution with a target fps, optional checkerboard tracing, frame limit
- Presets (Performance, Balanced, Quality), shadows / global illumination / caustics / reflections switches, lamp colours, sky, auto-orbit
- Material 3 Expressive settings UI (android-ui-kit): five tabs, themes, haptics, HUD; English and Russian
- Safety governor: very slow frames automatically reduce render scale and samples per pixel
- Orbit, pinch zoom and double-tap reset gestures
