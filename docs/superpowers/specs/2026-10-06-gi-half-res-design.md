# Raytrace 1.2 — half-resolution GI pass (design)

Date: 2026-10-06. Builds on v1.1.0. Device: POCO F3 (Adreno 650).

## Goal
Make the default (Balanced, hybrid) experience substantially faster by moving global illumination (GI) out of the
per-pixel trace pass into its own half-resolution pass with its own temporal accumulation, denoise and a
depth/normal-aware upsample. Add per-pass GPU timing so results are measured honestly.

Success criteria (measured on device, A/B in the same session because GPU clocks vary ~25%):
- Balanced default at least 30% faster than `gi_res = 0` (old inline GI) at the same render scale and flags.
- Image fidelity: mean luminance and colour bleeding (green from the left wall, orange from the right) in chosen
  regions differ by at most 5% from `gi_res = 0`; no new visible light leaks at object edges.
- No regression with GI off, in path-tracing mode, and for mirror/glass pixels.
- All existing tests green; new pure helpers unit-tested; all WGSL passes the naga validation test.

Non-goals: world-space probe grids, changing the path-tracing mode, new scenes, TAAU upscaling (later).

## Current state (facts)
- `trace.wgsl` (single compute pass) computes per pixel: primary hit, direct light, caustics, GI (`indirect()`),
  specular chains; writes `raw` (rgba16f, alpha = traced flag) and `gbuf` (rgba32f: depth, octahedral normal,
  material id).
- `temporal.wgsl` accumulates `raw` into `hist[parity]` (reprojection by gbuf); `atrous.wgsl` filters 0-3 times;
  `present.wgsl` upsamples, tonemaps, sharpens.
- GI is the dominant cost (about 60-70% of frame time); measured Balanced default is ~22 fps at 0.25x.

## Design

### Pipeline (hybrid mode, GI on, gi_res > 0)
1. `trace` (full render resolution): as today, but for pixels whose primary hit is a diffuse surface (material kind 0,
   not emissive, not sky) it does NOT call `indirect()`. It additionally writes `alb` (new rgba16f full-res texture):
   rgb = albedo of the primary hit, a = 1 if GI was deferred for this pixel, else 0. Pixels with a = 0 (sky, lamps,
   mirror, glass, path-tracing mode) keep the old inline path; checkerboard-skipped pixels still get `alb` written (the primary hit is always known) because their colour comes from history and GI is added at composite time
   (specular chains still call `indirect()` inline for the diffuse point they reach).
2. `gi_trace` (half resolution, `gw = ceil(w/2)`, `gh = ceil(h/2)`): each invocation chooses ONE full-res pixel of its
   2x2 block, `block_pixel(gx, gy, frame)` = block origin + offset `frame & 3` rotated by a per-block hash so
   all four pixels are visited over 4 frames; reads `gbuf` and `alb.a` at that pixel; if `alb.a < 0.5` writes
   `(0,0,0,0)` (invalid). Otherwise rebuilds the world point from `cam_pos + cam_ray * depth` and the normal from
   the octahedral value, runs `indirect(p, n)` once and writes `(E, 1)` to `gi_raw` (rgba16f half-res).
   Uses the same `rng` seeding scheme as trace. Clamps luminance like `clamp_lum`.
3. `gi_temporal` (half res): reprojects with the previous camera (same math as `temporal.wgsl`, using the full-res
   `gbuf` of the representative pixel and the previous frame's gbuf), validity by depth/normal/id, bilinear history
   read from `gi_hist[1-parity]`, blend weight `max(1/n, gi_floor)` with `gi_floor = 0.15` (fixed in 1.2),
   history length capped at `ceil(1/gi_floor)` while moving, NaN-safe `sanitize`, invalid samples keep history.
   Output `gi_hist[parity]` (rgb = filtered GI, a = history length).
4. `gi_atrous` (half res, 2 iterations, steps 1 and 2): edge-stopping on depth, normal and material id using the
   half-res subsample of `gbuf` (pixel (2x, 2y)). Output `gi_flt`.
5. Existing full-res `temporal` and `atrous` run on `raw` as today (it no longer contains GI, so it is cleaner).
6. `composite` (full res): `out = filtered_color + alb.rgb * gi_up` where `gi_up` is a joint bilateral upsample of
   `gi_flt` over the 4 nearest half-res texels with weights `w = bilinear * wn * wz * wid`
   (`wn = pow(max(dot(n,n_i),0),16)`, `wz = exp(-|z-z_i|/(0.05*z+0.01))`, `wid = id match`), normalised;
   if all weights vanish fall back to the nearest valid texel, else 0. Applies only where `alb.a >= 0.5`.
   `present` then samples `out` (it already samples whatever final texture it is given).
7. If `gi_res = 0`, GI is not deferred (flag clear), none of steps 2-4 and 6 run, and the pipeline equals v1.1.0.

`gi_res = 2` (quarter) uses 4x4 blocks with offset `frame & 15` and `gw = ceil(w/4)` — same code with the
block side (2 or 4) passed in `Params.gi_block`.

### Shader organisation
`rust/src/shaders/trace.wgsl` is split into `scene.wgsl` (RNG, SDF, `map`, `map_gi`, marching, materials, sky,
lights, `direct_light`, `direct_light_gi`, `caustic`, `indirect`) and two entry files: `trace.wgsl` (integrator and
`main`) and `gi_trace.wgsl` (GI `main`). `shaders.rs` concatenates `common + scene + entry`. Step 1 of the plan is
this refactor alone, proven by unchanged behaviour (naga test, device fps and screenshot compared to v1.1.0).
New files: `gi_trace.wgsl`, `gi_temporal.wgsl`, `gi_atrous.wgsl`, `composite.wgsl`. The rules from v1.0 stay:
never pass the `Params` struct by value (use `has(F_X)` / `P.flags`); never index an array by a loop variable;
NaN guard uses the bit-pattern `is_nan`.

### Params / UI
- `Params` gains `gi_block: u32` (pixels per block side: 0 = deferred GI off, 2 = half resolution, 4 = quarter
  resolution) and `gi_floor: f32`; layout stays 16-byte aligned (fields added into the existing pad
  slots or at the end with explicit padding; `GpuParams` size assert updated and the WGSL struct kept field-for-field).
  New flag bit `F_GI_SPLIT = 512` set by the renderer when hybrid mode, GI on and `gi_res > 0`.
- New param id 30 `gi_res` (0 full inline, 1 half, 2 quarter; default 1; native). N becomes 31;
  `params.json`, `params.rs`, `Settings.kt` and tests updated together. `gi_res` change resets history
  (it is added to `affects_history`).
- UI: chip row "GI resolution" (Full / Half / Quarter) on the Light tab; presets: Performance = 2, Balanced = 1,
  Quality = 0 (Balanced default row must equal `params.json` defaults; tests updated).
- Textures are (re)created in `ensure_targets` together with the others: `alb` full-res rgba16f,
  `gi_raw`, `gi_hist[2]`, `gi_tmp[2]` half/quarter-res rgba16f, `comp` full-res rgba16f. When `gi_res = 0` the GI
  textures are not touched and cost nothing.

### Timing
`Features::TIMESTAMP_QUERY` is requested only if the adapter supports it. A query set with 2 timestamps per
timed pass writes via `ComputePassTimestampWrites`; results resolved to a buffer and read back one frame late
(no stall). `stats()` grows from 8 to 16 floats: pass ms for `trace`, `gi`, `temporal`, `atrous`, `composite`,
`present`; HUD "Full" shows them when available. If the feature is missing the values are 0 and nothing else changes.

### Error handling and edge cases
- Odd render sizes: half-res dims use ceil; the representative pixel is clamped to the full-res bounds.
- First frame / history reset / resize: GI history is not read (same `history_reset` flag).
- Pixels whose block's chosen pixel is invalid (sky, mirror, ...) write invalid; the bilateral upsample ignores
  invalid texels; a valid full-res pixel with no valid neighbour texel gets GI from the nearest valid one or 0.
- Governor/adaptive resolution continue to work unchanged (they use total GPU ms).
- Checkerboard: a skipped pixel still writes `alb` (a = 1 when its primary hit is diffuse); its colour comes from the
  temporal history as today and its GI comes from the same upsample.

### Testing
- Rust unit tests (host): `gi_size(w, h, level)`, `block_pixel` coverage (all offsets visited within 4/16 frames, always
  in range, odd sizes), param table consistency, `Governor`/adaptive untouched, naga validation of all new WGSL.
- Kotlin: updated param count tests, Presets table tests (Balanced = defaults), Light-tab chip is covered by code review.
- Device (short, user permitting): A/B `gi_res` 0/1/2 on the same camera with `19=0,18=0`, fps and GPU ms from the log
  and from per-pass timings; screenshots for colour-bleed regions; no-GI, path tracing and mirror/glass sanity.
- Bench: results appended to `docs/bench.md` and the README table updated with the exact conditions.

### Risks
- GI of animated geometry (Menger rotation) lags by ~6 frames (documented).
- Bilateral upsample slightly softens fine GI detail (GI is low-frequency).
- The representative-pixel jitter plus temporal accumulation can show faint 2x2 patterns on very noisy surfaces
  during fast motion; mitigated by the à-trous pass and the bilateral upsample.
- Adreno compiler hazards (see rules above); mitigated by the naga test plus the first device run of each new shader.

## Release
Version 1.2.0 (versionCode 3), EN + RU README/CHANGELOG, release notes, GitHub release with signed APK, as for 1.1.0.
