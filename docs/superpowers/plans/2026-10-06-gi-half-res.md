# Raytrace 1.2 — half-resolution GI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move global illumination out of the per-pixel trace pass into a half- (or quarter-) resolution pass with its own temporal accumulation, denoise and depth/normal-aware upsample, plus per-pass GPU timing, so the Balanced default gets at least 30% faster at the same visual quality.

**Architecture:** `trace` stops calling `indirect()` for primary diffuse hits and writes their albedo to a new `alb` texture. A new `gi_trace` pass (one `indirect()` ray per block of 2x2 or 4x4 pixels, rotating the sampled pixel every frame) feeds `gi_temporal` (reprojected accumulation) and `gi_atrous` (2 iterations). A `composite` pass adds `alb * bilateral_upsample(gi)` to the denoised colour before `present`. Everything else (specular chains, path tracing, GI off, `gi_res = 0`) behaves exactly as in v1.1.0.

**Tech Stack:** Rust 1.99 + wgpu 24 + WGSL (naga validation in unit tests), Kotlin Views + android-ui-kit, Gradle (`./gradlew`), Termux host == android aarch64.

**Spec:** `docs/superpowers/specs/2026-10-06-gi-half-res-design.md`

## Global Constraints
- Termux build: `cd rust && cargo test --lib` (host tests) and `./gradlew --no-daemon testDebugUnitTest assembleDebug|assembleRelease -q` (the Gradle build runs `tools/build-native.sh`). Run scripts with `bash script`; temp files in `~/tmp`; on "Text file busy" rerun.
- WGSL rules (hard-won): never pass the `Params` uniform struct by value to a function (use `has(F_X)` or read `P.flags`); never index an array by a loop variable (Adreno shader compiler crashed) — use computed values/`select`; NaN guard via `is_nan` / `sanitize`; every shader is `common.wgsl` + `scene.wgsl` (only for shaders that need it) + its own file; `Params` struct in `common.wgsl` and `GpuParams` in `gfx.rs` must stay field-for-field identical with a size assert.
- Param ids are stable: new id 30 `gi_res` (native, 0 full inline GI, 1 half, 2 quarter; default 1); N becomes 31; `params.json`, `rust/src/params.rs`, `Settings.kt` (+ tests) change together.
- Defaults: the Balanced preset row must equal the `params.json` defaults.
- Kit files `app/src/main/java/io/github/xtratter/uikit/*` are never edited. UI strings bilingual via `t(en, ru)`. No sliders.
- Every commit message ends with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; do not change git config; no secrets/IPs in commits. Branch: `feat/gi-halfres`.
- **Phone rules:** host-only work by default. Tasks 1-7 never touch the phone (no `su`, adb, install, launch, screencap, logcat). Only Task 8 uses the device, only when the controller says the user allows it, with short runs: after each test `su -c 'am force-stop dev.starinin.raytrace'` and `su -c 'am start -n com.termux/com.termux.app.TermuxActivity'`; `su` only for pm install (copy the APK to `/data/local/tmp` with `su -c cp` first), `pm clear dev.starinin.raytrace`, `am start/force-stop` (`--es dbg "id=v,..."` works in debug builds only), `logcat -c/-d -s raytrace`, `screencap` to `/sdcard/claude/`.
- Honest benchmarks only: A/B in the same session, GPU clocks vary ~25%; never claim numbers that were not measured.

## Review Focus
1. Odd render sizes (e.g. 353x791, 1x1) and blocks at the right/bottom border: half-res dims use ceil, the sampled pixel is clamped to the full-res bounds (Task 1 tests `gi_size`, `block_offset`; Task 4 shaders clamp).
2. NaN/Inf from `indirect()` must never poison `gi_hist` (sanitize on input and history; `reproj::blend` semantics) (Task 4).
3. Switching `gi_res` at runtime (0 <-> 1 <-> 2), toggling GI/mode/checkerboard, and resizing/governor scale changes must recreate GI textures and reset history without stale bind groups or crashes (Task 5: `ensure_targets(rw, rh, gi_block)` rebuilds on any change; `gi_res` is in `affects_history`).
4. No double GI: pixels with `alb.a = 1` get GI only from composite; mirror/glass/sky/lamp pixels and path tracing keep the inline path (Task 3 shader edits + Task 5 flag logic).
5. Checkerboard-skipped pixels still get `alb` written and GI from composite (Task 3).
6. Light leaking across depth/normal edges in the upsample and a-trous (Task 4 weights; Task 8 visual check at object silhouettes).

## File Structure
```
rust/src/gi.rs                       pcg, block_offset, gi_size (pure, host-tested)
rust/src/shaders/scene.wgsl          NEW: RNG(rnd), SDF, map/map_gi, marching, materials, sky, lights, direct_light*, caustic, indirect, clamp_lum
rust/src/shaders/trace.wgsl          integrator + main only (+ alb output, GI deferral)
rust/src/shaders/gi_trace.wgsl       NEW: half-res indirect() pass
rust/src/shaders/gi_temporal.wgsl    NEW: half-res reprojected accumulation (mirrors reproj::blend, still=false)
rust/src/shaders/gi_atrous.wgsl      NEW: half-res edge-aware filter
rust/src/shaders/composite.wgsl      NEW: full-res colour + albedo * bilateral GI upsample
rust/src/shaders/common.wgsl         + Params gi_block/gi_floor, F_GI_SPLIT, pcg, block_offset
rust/src/shaders.rs                  + scene join, gi_trace()/gi_temporal()/gi_atrous()/composite(), validation test
rust/src/{params,renderer,gfx,jni_api}.rs   param 30, flags, textures/passes, timing, stats 16
app/.../{Settings,Presets,SettingsSheet,Hud,Native}.kt   id, preset column, chip row, HUD pass times, stats size
params.json, README(.ru).md, CHANGELOG(.ru).md, docs/bench.md, app/build.gradle.kts (1.2.0/3), rust/Cargo.toml
```

---

### Task 1: Param `gi_res`, flags, pure helpers, Params extension

**Files:**
- Create: `rust/src/gi.rs`; Modify: `rust/src/lib.rs`, `rust/src/params.rs`, `rust/src/gfx.rs` (GpuParams only), `rust/src/renderer.rs` (struct fill only), `rust/src/shaders/common.wgsl`, `params.json`, `app/src/main/java/dev/starinin/raytrace/Settings.kt`, `app/src/test/java/dev/starinin/raytrace/SettingsTest.kt`
- Test: inline tests in `gi.rs`, `params.rs`; `SettingsTest.kt`

**Interfaces:**
- Produces: `gi::pcg(u32)->u32`, `gi::block_offset(gx: u32, gy: u32, frame: u32, bs: u32) -> (u32, u32)`, `gi::gi_size(w: u32, h: u32, bs: u32) -> (u32, u32)`; `params::id::GI_RES = 30`, `params::flags::GI_SPLIT = 512`, `Store::gi_block(&self) -> u32` (0 off, 2 half, 4 quarter), `Store::gi_split(&self) -> bool`; `GpuParams` gains `gi_block: u32, gi_floor: f32, pad4: u32, pad5: u32` appended at the end (size 192); WGSL `Params` same fields appended; `Ids.GI_RES = 30` in Kotlin.

- [ ] **Step 1: Failing tests for the pure helpers** — create `rust/src/gi.rs` with only the tests first (functions missing):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gi_size_ceils_and_never_zero() {
        assert_eq!(gi_size(353, 791, 2), (177, 396));
        assert_eq!(gi_size(8, 8, 4), (2, 2));
        assert_eq!(gi_size(1, 1, 2), (1, 1));
        assert_eq!(gi_size(264, 600, 4), (66, 150));
        assert_eq!(gi_size(0, 0, 2), (1, 1));
    }

    #[test]
    fn block_offset_in_range_and_covers_block_over_n_frames() {
        for bs in [2u32, 4] {
            let n = bs * bs;
            for (gx, gy) in [(0u32, 0u32), (1, 0), (0, 1), (37, 91), (65535, 1)] {
                let mut seen = vec![false; n as usize];
                for f in 0..n {
                    let (ox, oy) = block_offset(gx, gy, 1000 + f, bs);
                    assert!(ox < bs && oy < bs);
                    seen[(oy * bs + ox) as usize] = true;
                }
                assert!(seen.iter().all(|&s| s), "bs {bs} block ({gx},{gy}) missed an offset");
            }
        }
    }

    #[test]
    fn block_offset_differs_between_neighbouring_blocks() {
        // hash decorrelation: not all neighbours use the same offset on one frame
        let a: Vec<_> = (0..16u32).map(|gx| block_offset(gx, 3, 7, 2)).collect();
        assert!(a.iter().any(|&o| o != a[0]));
    }

    #[test]
    fn pcg_matches_known_values() {
        // reference values computed independently (python) from the same formula and constants as common.wgsl
        assert_eq!(pcg(0), 0x7bb2fe2);
        assert_eq!(pcg(1), 0xa8beea3c);
    }
}
```
Add `pub mod gi;` to `rust/src/lib.rs`.
- [ ] **Step 2: Run to verify failure** — `cd rust && cargo test --lib gi 2>&1 | tail -15` — Expected: compile errors (`gi_size`, `block_offset`, `pcg` not found).
- [ ] **Step 3: Implement** (above the tests in `gi.rs`):
```rust
//! Helpers shared (by formula) with the GI shaders; kept pure so the host can test them.

/// PCG hash, identical to `pcg` in common.wgsl.
pub fn pcg(v: u32) -> u32 {
    let s = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277803737);
    (w >> 22) ^ w
}

/// Which pixel of the `bs` x `bs` block (gx, gy) is traced this frame. Over `bs*bs` consecutive
/// frames every pixel of the block is visited once; a per-block hash decorrelates neighbours.
/// Identical to `block_offset` in common.wgsl.
pub fn block_offset(gx: u32, gy: u32, frame: u32, bs: u32) -> (u32, u32) {
    let n = bs * bs;
    let h = pcg(gx.wrapping_mul(73856093) ^ gy.wrapping_mul(19349663));
    let k = frame.wrapping_add(h) % n;
    (k % bs, k / bs)
}

/// Half/quarter-resolution size for a render size (ceil, never zero).
pub fn gi_size(w: u32, h: u32, bs: u32) -> (u32, u32) {
    (w.max(1).div_ceil(bs), h.max(1).div_ceil(bs))
}
```
- [ ] **Step 4: Run** — `cd rust && cargo test --lib gi` — Expected: PASS.
- [ ] **Step 5: Failing param tests** in `rust/src/params.rs` `mod tests`:
```rust
    #[test]
    fn gi_res_param_and_flags() {
        let mut s = Store::new();
        assert_eq!(s.get(id::GI_RES), 1.0);
        assert_eq!(s.gi_block(), 2);
        assert!(s.gi_split());
        assert_ne!(s.flags() & flags::GI_SPLIT, 0);
        s.set(id::GI_RES as i32, 2.0);
        assert_eq!(s.gi_block(), 4);
        s.set(id::GI_RES as i32, 0.0);
        assert_eq!(s.gi_block(), 0);
        assert!(!s.gi_split());
        assert_eq!(s.flags() & flags::GI_SPLIT, 0);
        s.set(id::GI_RES as i32, 1.0);
        s.set(id::GI as i32, 0.0); // GI off -> no deferred GI
        assert!(!s.gi_split());
        s.set(id::GI as i32, 1.0);
        s.set(id::MODE as i32, 1.0); // path tracing keeps inline GI
        assert!(!s.gi_split());
        assert!(affects_history(id::GI_RES));
        assert_eq!(s.set(id::GI_RES as i32, 99.0), Change::Changed);
        assert_eq!(s.get(id::GI_RES), 2.0);
    }
```
and add to `params.json` the entry `{"id":30,"key":"gi_res","default":1,"min":0,"max":2,"native":true}` placed after id 29 (before id 100). Run `cd rust && cargo test --lib params` — Expected: compile error / FAIL (`GI_RES` missing; `defaults_match_json` count mismatch).
- [ ] **Step 6: Implement** in `rust/src/params.rs`: `pub const N: usize = 31;` append `d(30, 1.0, 0.0, 2.0), // gi_res` to `DEFS`; `pub const GI_RES: usize = 30;` in `mod id`; `pub const GI_SPLIT: u32 = 512;` in `mod flags` (comment: deferred GI active); add `id::GI_RES` to the `matches!` list of `affects_history`; in `impl Store`:
```rust
    /// Pixels per GI block side: 0 = deferred GI off, 2 = half resolution, 4 = quarter resolution.
    pub fn gi_block(&self) -> u32 {
        match self.v[id::GI_RES] as u32 { 0 => 0, 1 => 2, _ => 4 }
    }

    /// True when GI is computed in the separate half/quarter-res pass (hybrid mode only).
    pub fn gi_split(&self) -> bool {
        self.on(id::GI) && self.gi_block() > 0 && self.v[id::MODE] < 0.5
    }
```
and in `flags()` add `if self.gi_split() { f |= flags::GI_SPLIT; }`. Update the existing count assertion if it hardcodes 30 (grep `30` in tests).
- [ ] **Step 7: Params struct extension.** `rust/src/gfx.rs`: append to `GpuParams` (after `col_b`/`pad3`): `pub gi_block: u32, pub gi_floor: f32, pub pad4: u32, pub pad5: u32,` and change the assert to `== 192`; fix the doc comment (192 bytes). `rust/src/shaders/common.wgsl`: append the same four fields (`gi_block: u32, gi_floor: f32, pad4: u32, pad5: u32,`) at the end of `struct Params` and `const F_GI_SPLIT: u32 = 512u;` after `F_MOVED`. `rust/src/renderer.rs`: set `gi_block: s.gi_block(), gi_floor: 0.15, pad4: 0, pad5: 0` in the `GpuParams` literal (value is inert until Task 5; if GI is split the shaders do not exist yet, so ALSO force `flags &= !flags::GI_SPLIT` in `frame()` with a comment `// enabled in Task 5` and make `gi_block: 0` — Task 5 removes both).
- [ ] **Step 8: Kotlin.** In `Settings.kt` add `const val GI_RES = 30` to `Ids` and `d(30, "gi_res", 1f, 0f, 2f),` to `DEFS` after the `look_speed` entry; in `SettingsTest.kt` change the pushAll expectation from 30 to 31.
- [ ] **Step 9: Run everything** — `cd rust && cargo test --lib 2>&1 | tail -5` (all pass incl. `shaders_validate`, `defaults_match_json`) and `./gradlew --no-daemon testDebugUnitTest -q 2>&1 | tail -5` (long timeout) — Expected: pass.
- [ ] **Step 10: Commit** — `git add -A && git commit -m "feat: gi_res param (id 30), GI_SPLIT flag, pure GI helpers, Params extension

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 2: Scene refactor (no behaviour change)

**Files:**
- Create: `rust/src/shaders/scene.wgsl`; Modify: `rust/src/shaders/trace.wgsl`, `rust/src/shaders/common.wgsl`, `rust/src/shaders.rs`
- Test: `rust/src/shaders.rs` (existing `shaders_validate`)

**Interfaces:**
- Produces: `shaders::trace()` = common + scene + trace entry (unchanged behaviour); `common.wgsl` now owns `pcg` and `block_offset`; `scene.wgsl` owns everything from the old `const PI` through `fn indirect` plus `clamp_lum` (RNG `rng`/`rnd`/`rand_unit`/`make_basis`/`cos_hemi`/`sample_cone`, `rot_y`, SDF, `map`, `map_gi`, `calc_normal*`, `march*`, `Mat`/`material`, `sky`, `schlick`, `fresnel3`, lights, `isect_sphere`, `direct_light`, `caustic`, `direct_light_gi`, `indirect`, `clamp_lum`). Entry files declare their own `@group(0) @binding(0) var<uniform> P: Params;` and their storage bindings.
- Consumes: Task 1 `Params`/flag consts.

- [ ] **Step 1: Baseline.** Record `cd rust && cargo test --lib 2>&1 | tail -3` (passing count) and save the current shader for comparison: `cp rust/src/shaders/trace.wgsl ~/tmp/trace_before.wgsl`.
- [ ] **Step 2: Move `pcg` and add `block_offset` to `common.wgsl`** (append after `sanitize`):
```wgsl
fn pcg(v: u32) -> u32 {
  let s = v * 747796405u + 2891336453u;
  let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}

// Which pixel of the bs x bs block (gx, gy) a GI pass samples this frame (mirrors gi::block_offset).
fn block_offset(gx: u32, gy: u32, frame: u32, bs: u32) -> vec2<u32> {
  let n = bs * bs;
  let h = pcg(gx * 73856093u ^ gy * 19349663u);
  let k = (frame + h) % n;
  return vec2<u32>(k % bs, k / bs);
}
```
- [ ] **Step 3: Create `scene.wgsl`** by cutting from `trace.wgsl`: everything from the line `const PI: f32 = 3.14159265;` through the end of `fn indirect(...)` (the line before `// ----- integrator` / `fn trace`), PLUS `fn clamp_lum`. Remove the old `fn pcg` from the moved text (it now lives in `common.wgsl`). Start `scene.wgsl` with the comment `// Scene description and shading helpers shared by the trace and GI passes (concatenated after common.wgsl; the including shader declares the P uniform).` Do not change any function body. What stays in `trace.wgsl`: the header comment, the `@group(0)` bindings (`P`, `out_rad`, `out_g`), the `g_depth/g_n/g_id/g_skip` privates, `fn trace`, `fn shade_pixel`, `fn main`.
- [ ] **Step 4: `shaders.rs`:**
```rust
const SCENE: &str = include_str!("shaders/scene.wgsl");

fn join_scene(body: &str) -> String { format!("{}\n{}\n{}", COMMON, SCENE, body) }

pub fn trace() -> String { join_scene(include_str!("shaders/trace.wgsl")) }
```
(keep `join` for the others). `shaders_validate` already calls `trace()`.
- [ ] **Step 5: Run** — `cd rust && cargo test --lib 2>&1 | tail -6` — Expected: all pass (the naga test proves the split compiles). Also verify the move was verbatim: `cd rust/src/shaders && cat scene.wgsl trace.wgsl | grep -c "fn "` equals the function count of `~/tmp/trace_before.wgsl` plus the two new ones? (before: `grep -c "fn " ~/tmp/trace_before.wgsl`; after: `cat common.wgsl scene.wgsl trace.wgsl | grep -c "fn "` minus the `common.wgsl` helpers that existed before: compare the sorted `fn` names of `scene.wgsl+trace.wgsl` with the before file's — only `pcg` is allowed to be missing). Record the comparison command and result in your report.
- [ ] **Step 6: Build** — `./gradlew --no-daemon assembleDebug -q 2>&1 | tail -3` — Expected: no errors.
- [ ] **Step 7: Commit** — `git add -A && git commit -m "refactor: split trace.wgsl into scene.wgsl + entry; pcg/block_offset in common

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 3: Trace writes `alb` and defers GI

**Files:**
- Modify: `rust/src/shaders/trace.wgsl`, `rust/src/gfx.rs`
- Test: `shaders_validate` + a Rust test for the bind layout is not possible on host; verification is naga + build.

**Interfaces:**
- Consumes: Task 2 shader layout, Task 1 `F_GI_SPLIT`.
- Produces: trace binding 3 `out_alb: texture_storage_2d<rgba16float, write>`: rgb = albedo of the primary hit, a = 1 when GI was deferred for that pixel else 0 (written for EVERY pixel incl. checkerboard-skipped ones; always `(0,0,0,0)` when `F_GI_SPLIT` is clear). `Targets.alb: Tex` (full-res Rgba16Float), bound at trace binding 3.

- [ ] **Step 1: `trace.wgsl` edits.**
(a) After the `out_g` binding add `@group(0) @binding(3) var out_alb: texture_storage_2d<rgba16float, write>;` and after `var<private> g_skip` add `var<private> g_alb: vec4<f32>; // rgb albedo of the primary diffuse hit, a = 1 when its GI is deferred to the GI pass`.
(b) In `trace()`, immediately AFTER the `if (!has(F_REFLECT) && m.kind != 0u) { ... }` override block and BEFORE `if (m.emit.x > 0.0) {`, insert:
```wgsl
    if (b == 0 && !pt && m.kind == 0u && m.emit.x <= 0.0 && has(F_GI) && has(F_GI_SPLIT)) {
      g_alb = vec4<f32>(m.albedo, 1.0);
    }
```
(c) In the hybrid diffuse branch replace `if (has(F_GI)) { lo += indirect(p, nf); }` with `if (has(F_GI) && g_alb.w < 0.5) { lo += indirect(p, nf); }` (specular-chain diffuse points, b > 0, never have `g_alb` set, so they keep the inline GI).
(d) In `main`, before `col = shade_pixel(pix);` add `g_alb = vec4<f32>(0.0);` and after the existing `textureStore(out_g, ...)` add `textureStore(out_alb, ip, g_alb);`.
- [ ] **Step 2: `gfx.rs`.** `Targets` gets `alb: Tex`; `ensure_targets` creates `alb: make_tex(d, rw, rh, f16)`; `trace_bgl` becomes `[uniform(0, cs), tex_out(1, f16), tex_out(2, f32x4), tex_out(3, f16)]`; in `render()` the `trace_bg` gets `(3, wgpu::BindingResource::TextureView(&t.alb.view))`.
- [ ] **Step 3: Run** — `cd rust && cargo test --lib 2>&1 | tail -4` (naga passes) and `./gradlew --no-daemon assembleDebug -q 2>&1 | tail -3`.
- [ ] **Step 4: Behaviour check by reasoning (write it in the report):** with `F_GI_SPLIT` clear (Task 1 forces it clear until Task 5) `g_alb.w` stays 0 so `indirect()` is called exactly as before; list the three cases (specular chain, checkerboard-skipped, path tracing) and why each is unchanged.
- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat: trace writes alb and can defer primary-hit GI (flag still off)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 4: GI shaders (gi_trace, gi_temporal, gi_atrous, composite)

**Files:**
- Create: `rust/src/shaders/{gi_trace,gi_temporal,gi_atrous,composite}.wgsl`; Modify: `rust/src/shaders.rs`
- Test: `shaders_validate` extended

**Interfaces (bindings, all group 0):**
- `gi_trace`: 0 `P` uniform; 1 `g_tex` texture_2d (full-res gbuf current); 2 `alb_tex` texture_2d (full-res); 3 `gi_out` storage rgba16float (gw x gh). Output texel: `(E, 1)` valid or `(0,0,0,0)` invalid.
- `gi_temporal`: 0 `P`; 1 `gi_raw`; 2 `g_cur`; 3 `g_prev`; 4 `gi_hist_in`; 5 `gi_hist_out` (storage rgba16float). Output `(rgb, history_length)`, length 0 = invalid.
- `gi_atrous`: 0 `A` uniform `vec4<u32>` (x step, y gw, z gh); 1 `src`; 2 `gb` (full-res gbuf); 3 `dst` storage rgba16float; 4 `P` uniform. Output `(rgb, 1)` valid or `(0,0,0,0)`.
- `composite`: 0 `P`; 1 `col_tex`; 2 `gb`; 3 `alb_tex`; 4 `gi_tex`; 5 `comp` storage rgba16float (full-res).
- `shaders::{gi_trace, gi_temporal, gi_atrous, composite}() -> String` (`gi_trace` = common + scene + body; the others = common + body).

- [ ] **Step 1: Failing test** — in `shaders.rs` extend `shaders_validate` with `validate("gi_trace", gi_trace()); validate("gi_temporal", gi_temporal()); validate("gi_atrous", gi_atrous()); validate("composite", composite());` and add the four `pub fn`s referencing files that do not exist yet; run `cd rust && cargo test --lib shaders 2>&1 | tail -5` — Expected: compile error (missing files).
- [ ] **Step 2: `gi_trace.wgsl`:**
```wgsl
// Half/quarter-resolution GI: one indirect() sample per block, taken at a rotating pixel of the block.
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var g_tex: texture_2d<f32>;
@group(0) @binding(2) var alb_tex: texture_2d<f32>;
@group(0) @binding(3) var gi_out: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let bs = P.gi_block;
  let gw = (u32(P.res.x) + bs - 1u) / bs;
  let gh = (u32(P.res.y) + bs - 1u) / bs;
  if (gid.x >= gw || gid.y >= gh) { return; }
  let off = block_offset(gid.x, gid.y, P.frame, bs);
  let flim = vec2<u32>(u32(P.res.x) - 1u, u32(P.res.y) - 1u);
  let fp = min(vec2<u32>(gid.x * bs + off.x, gid.y * bs + off.y), flim);
  let ip = vec2<i32>(fp);
  var o = vec4<f32>(0.0);
  let a = textureLoad(alb_tex, ip, 0);
  if (a.w > 0.5) {
    let g = textureLoad(g_tex, ip, 0);
    rng = pcg(gid.y * gw + gid.x + P.seed * 747796405u);
    rng = pcg(rng ^ 2747636419u);
    let rd = cam_ray(vec2<f32>(fp) + vec2<f32>(0.5) + P.jitter, P.res, P.cam_pos, P.cam_target, P.fov);
    let p = P.cam_pos + rd * g.x;
    let n0 = oct_decode(g.yz);
    let n = select(-n0, n0, dot(rd, n0) < 0.0);
    var e = indirect(p, n);
    if (is_nan(e.x) || is_nan(e.y) || is_nan(e.z) || any(e != e)) { e = vec3<f32>(0.0); }
    e = clamp_lum(e, 10.0);
    o = vec4<f32>(e, 1.0);
  }
  textureStore(gi_out, vec2<i32>(gid.xy), o);
}
```
- [ ] **Step 3: `gi_temporal.wgsl`** (mirrors `reproj::blend(.., still_pt = false)`: `nn = min(n, 2048) + 1`, `a = max(1/nn, floor)`, cap `ceil(1/floor)`):
```wgsl
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var gi_raw: texture_2d<f32>;
@group(0) @binding(2) var g_cur: texture_2d<f32>;
@group(0) @binding(3) var g_prev: texture_2d<f32>;
@group(0) @binding(4) var gi_hist_in: texture_2d<f32>;
@group(0) @binding(5) var gi_hist_out: texture_storage_2d<rgba16float, write>;

fn tap(p: vec2<i32>, w: f32, lim: vec2<i32>) -> vec4<f32> {
  let h = textureLoad(gi_hist_in, clamp(p, vec2<i32>(0), lim), 0);
  let v = select(0.0, w, h.a > 0.0);
  return vec4<f32>(h.rgb * v, v);
}

// validity-weighted history length
fn tap_len(p: vec2<i32>, w: f32, lim: vec2<i32>) -> f32 {
  let h = textureLoad(gi_hist_in, clamp(p, vec2<i32>(0), lim), 0);
  return select(0.0, h.a * w, h.a > 0.0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let bs = P.gi_block;
  let gw = (u32(P.res.x) + bs - 1u) / bs;
  let gh = (u32(P.res.y) + bs - 1u) / bs;
  if (gid.x >= gw || gid.y >= gh) { return; }
  let glim = vec2<i32>(i32(gw) - 1, i32(gh) - 1);
  let flim = vec2<i32>(P.res) - vec2<i32>(1);
  let gp0 = vec2<i32>(gid.xy);
  let cur = textureLoad(gi_raw, gp0, 0);
  if (cur.a < 0.5) {
    textureStore(gi_hist_out, gp0, vec4<f32>(0.0));
    return;
  }
  let off = block_offset(gid.x, gid.y, P.frame, bs);
  let fp = min(vec2<i32>(i32(gid.x * bs + off.x), i32(gid.y * bs + off.y)), flim);
  let gc = textureLoad(g_cur, fp, 0);
  let c = sanitize(cur.rgb);
  var hist = vec3<f32>(0.0);
  var n = 0.0;
  if (P.history_reset == 0u) {
    let rd = cam_ray(vec2<f32>(fp) + vec2<f32>(0.5) + P.jitter, P.res, P.cam_pos, P.cam_target, P.fov);
    let wp = P.cam_pos + rd * gc.x;
    let pp = cam_project(wp, P.res, P.prev_pos, P.prev_target, P.fov) - P.prev_jitter;
    if (pp.x >= 0.0 && pp.y >= 0.0 && pp.x < P.res.x && pp.y < P.res.y) {
      let ipp = clamp(vec2<i32>(floor(pp)), vec2<i32>(0), flim);
      let gp = textureLoad(g_prev, ipp, 0);
      let dprev = length(wp - P.prev_pos);
      let ok = abs(gp.x - dprev) < 0.04 * dprev + 0.02
            && dot(oct_decode(gc.yz), oct_decode(gp.yz)) > 0.85
            && gp.w == gc.w;
      if (ok) {
        let f = pp / f32(bs) - vec2<f32>(0.5);
        let i0 = vec2<i32>(floor(f));
        let t = f - floor(f);
        let w00 = (1.0 - t.x) * (1.0 - t.y);
        let w10 = t.x * (1.0 - t.y);
        let w01 = (1.0 - t.x) * t.y;
        let w11 = t.x * t.y;
        let s = tap(i0, w00, glim) + tap(i0 + vec2<i32>(1, 0), w10, glim)
              + tap(i0 + vec2<i32>(0, 1), w01, glim) + tap(i0 + vec2<i32>(1, 1), w11, glim);
        if (s.w > 1e-3) {
          hist = sanitize(s.xyz / s.w);
          let l = tap_len(i0, w00, glim) + tap_len(i0 + vec2<i32>(1, 0), w10, glim)
                + tap_len(i0 + vec2<i32>(0, 1), w01, glim) + tap_len(i0 + vec2<i32>(1, 1), w11, glim);
          n = l / s.w;
          if (!(n > 0.0) || n != n) { n = 0.0; }
        }
      }
    }
  }
  var out_c = c;
  var out_n = 1.0;
  if (n > 0.0) {
    let fl = P.gi_floor;
    let nn = min(n, 2048.0) + 1.0;
    let a = max(1.0 / nn, fl);
    out_c = mix(hist, c, a);
    out_n = min(nn, ceil(1.0 / fl));
  }
  textureStore(gi_hist_out, gp0, vec4<f32>(out_c, out_n));
}
```
- [ ] **Step 4: `gi_atrous.wgsl`:**
```wgsl
@group(0) @binding(0) var<uniform> A: vec4<u32>;   // x = step, y = gw, z = gh
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var gb: texture_2d<f32>;     // full-res gbuffer
@group(0) @binding(3) var dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<uniform> P: Params;

// B3-spline taps computed (not an indexed array: Adreno compiler crash).
fn kw(o: i32) -> f32 {
  let a = abs(o);
  return select(select(0.0625, 0.25, a == 1), 0.375, a == 0);
}

// full-res pixel at the centre of half-res texel p
fn center(p: vec2<i32>) -> vec2<i32> {
  let bs = i32(P.gi_block);
  return min(p * bs + vec2<i32>(bs / 2), vec2<i32>(P.res) - vec2<i32>(1));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let lim = vec2<i32>(i32(A.y), i32(A.z)) - vec2<i32>(1);
  let ip = vec2<i32>(gid.xy);
  if (ip.x > lim.x || ip.y > lim.y) { return; }
  let s0 = textureLoad(src, ip, 0);
  if (s0.a <= 0.0) {
    textureStore(dst, ip, vec4<f32>(0.0));
    return;
  }
  let g0 = textureLoad(gb, center(ip), 0);
  let n0 = oct_decode(g0.yz);
  var sum = vec3<f32>(0.0);
  var wsum = 0.0;
  let step = i32(A.x);
  for (var j = -2; j <= 2; j = j + 1) {
    for (var i = -2; i <= 2; i = i + 1) {
      let q = clamp(ip + vec2<i32>(i, j) * step, vec2<i32>(0), lim);
      let s = textureLoad(src, q, 0);
      let g = textureLoad(gb, center(q), 0);
      let wn = pow(max(dot(n0, oct_decode(g.yz)), 0.0), 24.0);
      let wz = exp(-abs(g.x - g0.x) / (0.05 * g0.x + 0.01));
      let wid = select(0.0, 1.0, g.w == g0.w);
      let wv = select(0.0, 1.0, s.a > 0.0);
      let w = kw(i) * kw(j) * wn * wz * wid * wv;
      sum += sanitize(s.rgb) * w;
      wsum += w;
    }
  }
  textureStore(dst, ip, vec4<f32>(sum / max(wsum, 1e-5), 1.0));
}
```
- [ ] **Step 5: `composite.wgsl`:**
```wgsl
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var col_tex: texture_2d<f32>;
@group(0) @binding(2) var gb: texture_2d<f32>;
@group(0) @binding(3) var alb_tex: texture_2d<f32>;
@group(0) @binding(4) var gi_tex: texture_2d<f32>;
@group(0) @binding(5) var comp: texture_storage_2d<rgba16float, write>;

fn center(p: vec2<i32>) -> vec2<i32> {
  let bs = i32(P.gi_block);
  return min(p * bs + vec2<i32>(bs / 2), vec2<i32>(P.res) - vec2<i32>(1));
}

// one bilateral tap: xyz = gi * weight, w = weight
fn gi_tap(q: vec2<i32>, wb: f32, glim: vec2<i32>, n0: vec3<f32>, z0: f32, id0: f32) -> vec4<f32> {
  let qc = clamp(q, vec2<i32>(0), glim);
  let s = textureLoad(gi_tex, qc, 0);
  let g = textureLoad(gb, center(qc), 0);
  let wn = pow(max(dot(n0, oct_decode(g.yz)), 0.0), 16.0);
  let wz = exp(-abs(g.x - z0) / (0.05 * z0 + 0.01));
  let wid = select(0.0, 1.0, g.w == id0);
  let wv = select(0.0, 1.0, s.a > 0.5);
  let w = wb * wn * wz * wid * wv;
  return vec4<f32>(sanitize(s.rgb) * w, w);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let flim = vec2<i32>(P.res) - vec2<i32>(1);
  let ip = vec2<i32>(gid.xy);
  if (ip.x > flim.x || ip.y > flim.y) { return; }
  let c = textureLoad(col_tex, ip, 0).rgb;
  let a = textureLoad(alb_tex, ip, 0);
  var o = c;
  if (a.w > 0.5) {
    let bs = i32(P.gi_block);
    let glim = (vec2<i32>(P.res) + vec2<i32>(bs - 1)) / vec2<i32>(bs) - vec2<i32>(1);
    let g0 = textureLoad(gb, ip, 0);
    let n0 = oct_decode(g0.yz);
    let hp = (vec2<f32>(ip) + vec2<f32>(0.5)) / f32(bs) - vec2<f32>(0.5);
    let i0 = vec2<i32>(floor(hp));
    let t = hp - floor(hp);
    let s = gi_tap(i0, (1.0 - t.x) * (1.0 - t.y), glim, n0, g0.x, g0.w)
          + gi_tap(i0 + vec2<i32>(1, 0), t.x * (1.0 - t.y), glim, n0, g0.x, g0.w)
          + gi_tap(i0 + vec2<i32>(0, 1), (1.0 - t.x) * t.y, glim, n0, g0.x, g0.w)
          + gi_tap(i0 + vec2<i32>(1, 1), t.x * t.y, glim, n0, g0.x, g0.w);
    var gi = vec3<f32>(0.0);
    if (s.w > 1e-4) {
      gi = s.xyz / s.w;
    } else {
      let nq = clamp(vec2<i32>(floor(hp + vec2<f32>(0.5))), vec2<i32>(0), glim);
      let sn = textureLoad(gi_tex, nq, 0);
      if (sn.a > 0.5) { gi = sanitize(sn.rgb); }
    }
    o = c + a.rgb * gi;
  }
  textureStore(comp, ip, vec4<f32>(o, 1.0));
}
```
- [ ] **Step 6: `shaders.rs` functions:** `pub fn gi_trace() -> String { join_scene(include_str!("shaders/gi_trace.wgsl")) }`, `pub fn gi_temporal() -> String { join(include_str!("shaders/gi_temporal.wgsl")) }`, `pub fn gi_atrous() -> String { join(include_str!("shaders/gi_atrous.wgsl")) }`, `pub fn composite() -> String { join(include_str!("shaders/composite.wgsl")) }`.
- [ ] **Step 7: Run** — `cd rust && cargo test --lib 2>&1 | tail -6` — Expected: all pass including the extended `shaders_validate`. If naga rejects something (e.g. `rng` private variable declared in scene.wgsl is writable from `gi_trace` — it is a module-scope `var<private>`), fix the shader, not the test.
- [ ] **Step 8: Review-focus tests in the report:** (1) odd sizes: `gw/gh` use ceil and `fp` is clamped by `flim`; (2) NaN: `is_nan` + `sanitize` on input, history and taps; (3) all-invalid neighbourhoods: composite falls back to the nearest texel or 0.
- [ ] **Step 9: Commit** — `git add -A && git commit -m "feat: GI shaders (gi_trace, gi_temporal, gi_atrous, composite)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 5: Wire the GI pipeline (textures, pipelines, passes, renderer) and enable it

**Files:**
- Modify: `rust/src/gfx.rs`, `rust/src/renderer.rs`
- Test: host tests + `./gradlew assembleDebug`

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: `Gfx::ensure_targets(&mut self, rw: u32, rh: u32, gi_block: u32) -> bool` (recreates when size OR `gi_block` changed; `gi_block = 0` allocates no GI textures); `Gfx::render(&mut self, p: &GpuParams, denoise_iters: u32, parity: usize) -> f32` unchanged signature; GI passes run only when `p.flags & GI_SPLIT != 0 && p.gi_block > 0`.

- [ ] **Step 1: Targets.** Add (in `gfx.rs`):
```rust
struct GiTargets { bs: u32, w: u32, h: u32, raw: Tex, hist: [Tex; 2], tmp: [Tex; 2] }
```
to `Targets`: `gi: Option<GiTargets>, comp: Tex`. In `ensure_targets` take `gi_block`; early-return false only if `t.w == rw && t.h == rh && t.gi.as_ref().map(|g| g.bs).unwrap_or(0) == gi_block`; create `comp: make_tex(d, rw, rh, f16)` and, if `gi_block > 0`, `let (gw, gh) = crate::gi::gi_size(rw, rh, gi_block);` and `GiTargets { bs: gi_block, w: gw, h: gh, raw/hist x2/tmp x2: make_tex(d, gw, gh, f16) }`; write the two GI step buffers `[1 << k, gw, gh, 0]` (add `gi_step_bufs: [wgpu::Buffer; 2]` to `Gfx`, created like `step_bufs`). Keep the existing full-res step-buffer writes.
- [ ] **Step 2: Layouts + pipelines** (in `Gfx::new`, next to the others; `f16 = wgpu::TextureFormat::Rgba16Float`): 
```rust
let gi_trace_bgl = bgl(&device, "gi_trace", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_out(3, f16)]);
let gi_temporal_bgl = bgl(&device, "gi_temporal", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_in(3, cs, false), tex_in(4, cs, false), tex_out(5, f16)]);
let gi_atrous_bgl = bgl(&device, "gi_atrous", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_out(3, f16), uniform(4, cs)]);
let composite_bgl = bgl(&device, "composite", &[uniform(0, cs), tex_in(1, cs, false), tex_in(2, cs, false), tex_in(3, cs, false), tex_in(4, cs, false), tex_out(5, f16)]);
```
plus four compute pipelines created exactly like `temporal_pl` from `crate::shaders::{gi_trace,gi_temporal,gi_atrous,composite}()` with entry `main`, stored as fields (`gi_trace_pl`, ..., `composite_pl` and their bgls).
- [ ] **Step 3: Passes in `render()`.** After the `trace` pass: if `gi_on` (`p.flags & GI_SPLIT != 0 && p.gi_block > 0 && t.gi.is_some()`), build and run, each as its own compute pass dispatching `gw.div_ceil(8) x gh.div_ceil(8)`:
  1. `gi_trace`: bind `(0 pb, 1 gbuf[parity], 2 alb, 3 gi.raw)`.
  2. `gi_temporal` (after `gi_trace`): `(0 pb, 1 gi.raw, 2 gbuf[parity], 3 gbuf[1-parity], 4 gi.hist[1-parity], 5 gi.hist[parity])`.
  3. `gi_atrous` k=0: `(0 gi_step_bufs[0], 1 gi.hist[parity], 2 gbuf[parity], 3 gi.tmp[0], 4 pb)`; k=1: `(0 gi_step_bufs[1], 1 gi.tmp[0], 2 gbuf[parity], 3 gi.tmp[1], 4 pb)`; final GI = `gi.tmp[1]`.
  After the existing full-res a-trous chain (the `out` view), if `gi_on`: `composite` pass with `(0 pb, 1 out, 2 gbuf[parity], 3 alb, 4 gi.tmp[1], 5 comp)` dispatching full-res; then the present bind group samples `comp` instead of `out`. Order inside the encoder: trace, gi_trace, temporal, gi_temporal, gi_atrous x2, atrous x iters, composite, present (all in one encoder; wgpu inserts the barriers). Bind groups are created before the passes like the existing ones.
- [ ] **Step 4: Renderer.** In `frame()`: `let gi_block = if s.gi_split() { s.gi_block() } else { 0 };` and call `self.gfx.ensure_targets(rw, rh, gi_block)`; remove the Task-1 temporary overrides (the forced `flags &= !GI_SPLIT` and `gi_block: 0`) and set `gi_block` in `GpuParams` from the local `gi_block`, `gi_floor: 0.15`. `set_param`: add `id::GI_RES` to the existing history-reset condition via `affects_history` (already done in Task 1, verify). The `flags` variable already comes from `s.flags()` which sets `GI_SPLIT` through `gi_split()`.
- [ ] **Step 5: Host + build checks.** `cd rust && cargo test --lib 2>&1 | tail -4`; `./gradlew --no-daemon assembleDebug -q 2>&1 | tail -3` (long timeout). Also `cd rust && cargo check --release` is covered by the Gradle build.
- [ ] **Step 6: Reasoning checklist in the report (no device):** for each of: `gi_res` 0->1, 1->2, GI toggle, mode toggle, resize, governor scale change — state which `ensure_targets` condition rebuilds textures, that `reset_history` is set, and that no bind group is cached across frames (bind groups are created per frame).
- [ ] **Step 7: Commit** — `git add -A && git commit -m "feat: GI pipeline wiring (half/quarter-res GI textures, passes, composite) enabled

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 6: Per-pass GPU timestamps, stats 16, HUD

**Files:**
- Modify: `rust/src/gfx.rs`, `rust/src/renderer.rs`, `rust/src/jni_api.rs`, `app/src/main/java/dev/starinin/raytrace/Hud.kt`, `app/src/main/java/dev/starinin/raytrace/MainActivity.kt` (only if it indexes stats), `app/src/main/java/dev/starinin/raytrace/Native.kt` (doc only)
- Test: pure helper test for `pass_ms` conversion in `gfx.rs`? (not GPU-bound: put `fn ticks_to_ms(begin: u64, end: u64, period_ns: f32) -> f32` in `gi.rs`-style pure code in `rust/src/profile.rs`, host-tested)

**Interfaces:**
- Produces: `profile::ticks_to_ms(begin: u64, end: u64, period_ns: f32) -> f32` (0 if end < begin or any value is 0); `Gfx::pass_ms(&self) -> [f32; 8]` (slots: 0 trace, 1 gi_trace, 2 gi_temporal, 3 gi_atrous, 4 temporal, 5 atrous, 6 composite, 7 present; 0.0 if unsupported/unmeasured); stats array is 16 floats: indices 0..7 as before, 8..15 = the eight pass times; Kotlin `Native.stats()` returns 16 floats; `Hud.update` shows pass times in level 2 when `st.size >= 16 && st[8..16].any > 0`.

- [ ] **Step 1: Failing test** — `rust/src/profile.rs`:
```rust
//! Timestamp helpers (pure, host-tested).

/// GPU ticks -> milliseconds. `period_ns` is `Queue::get_timestamp_period()` (ns per tick).
pub fn ticks_to_ms(begin: u64, end: u64, period_ns: f32) -> f32 {
    if begin == 0 || end == 0 || end < begin || !period_ns.is_finite() { return 0.0; }
    ((end - begin) as f64 * period_ns as f64 / 1.0e6) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converts_and_guards() {
        assert!((ticks_to_ms(1_000, 2_001_000, 1.0) - 2.0).abs() < 1e-4);
        assert!((ticks_to_ms(10, 20, 52.083333) - 0.000521).abs() < 1e-5);
        assert_eq!(ticks_to_ms(0, 5, 1.0), 0.0);
        assert_eq!(ticks_to_ms(9, 5, 1.0), 0.0);
        assert_eq!(ticks_to_ms(1, 5, f32::NAN), 0.0);
    }
}
```
Add `pub mod profile;` to `lib.rs` (ungated). Run `cd rust && cargo test --lib profile` — expected FAIL first (write the test before the function body: stub `todo!()`-free approach: create the file with the test and a deliberately wrong body `0.0`, see RED, then fix).
- [ ] **Step 2: Gfx timing.** In `Gfx::new`: `let want = wgpu::Features::TIMESTAMP_QUERY; let has_ts = adapter.features().contains(want);` set `required_features: if has_ts { want } else { Features::empty() }`; if `has_ts`: `query_set = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("ts"), ty: wgpu::QueryType::Timestamp, count: 16 })`, `resolve_buf` (size 16*8, usage `QUERY_RESOLVE | COPY_SRC`) and `read_buf` (size 16*8, usage `COPY_DST | MAP_READ`), and `period = queue.get_timestamp_period()`. Store as `Option<Prof>` in `Gfx` plus `pass_ms: [f32; 8]`. In `render()` give every compute/render pass `timestamp_writes: prof.as_ref().map(|p| wgpu::ComputePassTimestampWrites { query_set: &p.qs, beginning_of_pass_write_index: Some(2*slot), end_of_pass_write_index: Some(2*slot+1) })` (render pass: `RenderPassTimestampWrites` likewise); the two `gi_atrous` passes share slot 3 only if you sum them — simpler: give the FIRST gi_atrous pass `beginning` index 6 and the SECOND `end` index 7 of slot 3? WebGPU allows one begin/end per pass; so time slot 3 as `begin of gi_atrous[0]` .. `end of gi_atrous[1]` by using `beginning_of_pass_write_index: Some(6), end_of_pass_write_index: None` on the first and `beginning: None, end: Some(7)` on the second; the same trick for the full-res `atrous` chain (slot 5, indices 10/11) — if `iters == 0` leave the slot unwritten. Zero the `resolve` range before use is not needed: unwritten queries resolve to 0 and `ticks_to_ms` returns 0 for them. After encoding the passes: `enc.resolve_query_set(&p.qs, 0..16, &p.resolve, 0); enc.copy_buffer_to_buffer(&p.resolve, 0, &p.read, 0, 128);`. After the existing `self.device.poll(wgpu::Maintain::Wait)`: `p.read.slice(..).map_async(wgpu::MapMode::Read, |_| {}); self.device.poll(wgpu::Maintain::Wait);` then read `bytemuck::cast_slice::<u8, u64>(&view)`, compute `pass_ms[slot] = ticks_to_ms(t[2*slot], t[2*slot+1], period)`, drop the view and `p.read.unmap()`. (We already wait for the GPU each frame, so there is no new pipeline stall; document this deviation from the spec's "one frame late".) `pub fn pass_ms(&self) -> [f32; 8]`.
- [ ] **Step 3: Stats 16.** `renderer.rs`: `pub stats: Arc<Mutex<[f32; 16]>>`, fill indices 8..15 from `self.gfx.pass_ms()`; `jni_api.rs`: all `[f32; 8]` become `[f32; 16]`, `new_float_array(16)`, zero-fill on thread end as before. Search for every `8` tied to stats (`grep -n "; 8\]\|(8)" rust/src/*.rs`).
- [ ] **Step 4: HUD.** `Hud.update(st, level)`: when `level == 2 && st.size >= 16 && (8..16).any { st[it] > 0f }` append a second line with `trace/gi/gt/ga/tmp/atr/cmp/pres` times (one decimal, ms) e.g. `"tr 4.1 · gi 6.0 · gt 0.3 · ga 0.4 · tm 0.5 · at 0.6 · cm 0.2 · pr 0.3"`; keep the single-line form when no timings. `MainActivity` tick already passes `Native.stats()`; if it checks `size < 6` make sure 16 passes.
- [ ] **Step 5: Run** — `cd rust && cargo test --lib 2>&1 | tail -4`; `./gradlew --no-daemon testDebugUnitTest assembleDebug -q 2>&1 | tail -3`. Reasoning note in the report: if the adapter lacks `TIMESTAMP_QUERY`, `prof` is `None`, all values stay 0 and the HUD shows no second line.
- [ ] **Step 6: Commit** — `git add -A && git commit -m "feat: per-pass GPU timestamps, stats 16, HUD pass times

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 7: UI chip, presets, docs, version 1.2.0, signed APK

**Files:**
- Modify: `app/src/main/java/dev/starinin/raytrace/{SettingsSheet,Presets}.kt`, `app/src/test/java/dev/starinin/raytrace/PresetsTest.kt`, `app/build.gradle.kts`, `rust/Cargo.toml` (+ `Cargo.lock`), `README.md`, `README.ru.md`, `CHANGELOG.md`, `CHANGELOG.ru.md`, `~/projects/releases/raytrace-1.2.0.md`
- Test: `PresetsTest`

**Interfaces:**
- Consumes: `Ids.GI_RES`.
- Produces: Light-tab chip row "GI resolution / Разрешение GI" (Full 0 / Half 1 / Quarter 2) via the existing `chips(...)` helper; `Presets.CONTROLLED` includes `Ids.GI_RES`; preset rows: Performance 2, Balanced 1, Quality 0.

- [ ] **Step 1: Failing tests** in `PresetsTest.kt`:
```kotlin
    @Test fun presetsSetGiResolution() {
        val st = s()
        Presets.apply(st, 0); assertEquals(2f, st.get(Ids.GI_RES))
        Presets.apply(st, 1); assertEquals(1f, st.get(Ids.GI_RES))
        Presets.apply(st, 2); assertEquals(0f, st.get(Ids.GI_RES))
    }

    @Test fun editingGiResolutionMarksCustom() {
        val st = s(); Presets.apply(st, 1)
        Presets.touch(st, Ids.GI_RES)
        assertEquals(3f, st.get(Ids.PRESET))
    }
```
Run `./gradlew --no-daemon testDebugUnitTest --tests '*PresetsTest*' -q` — expected FAIL (`balancedMatchesDefaults`/new tests).
- [ ] **Step 2: Implement.** `Presets.kt`: add `Ids.GI_RES` to `CONTROLLED` and `ORDER` (append at the END of both lists) and one more column to each row of `TABLE` (Performance `2f`, Balanced `1f`, Quality `0f`). `SettingsSheet.light()`: after the GI switch add `chips(t("GI resolution", "Разрешение GI"), Ids.GI_RES, listOf(t("Full", "Полное") to 0f, t("Half", "Половина") to 1f, t("Quarter", "Четверть") to 2f))`. Run the tests — Expected: PASS.
- [ ] **Step 3: Version + docs.** `app/build.gradle.kts` versionName `1.2.0`, versionCode 3; `rust/Cargo.toml` version `1.2.0` (Cargo.lock updates on build). README.md/README.ru.md: features (GI computed at half/quarter resolution with temporal accumulation and bilateral upsampling, per-pass GPU timings in the HUD "Full" mode), settings table row for GI resolution, architecture pipeline diagram updated (trace -> gi_trace -> temporal -> gi_temporal -> gi_atrous -> atrous -> composite -> present), known limitations (GI of the rotating sponge lags ~6 frames; GI shadows approximate). Do NOT put any performance number in docs yet (Task 8 measures); write "measured results: see docs/bench.md (1.2 section pending device check)". CHANGELOG.md/.ru.md: `1.2.0` entry (Added: GI resolution setting, half/quarter-res GI pass, per-pass timings; Changed: default GI is half resolution). Release notes `~/projects/releases/raytrace-1.2.0.md` (EN + RU, no numbers yet).
- [ ] **Step 4: Build + sign (host only)** — `cd rust && cargo test --lib`, `./gradlew --no-daemon testDebugUnitTest assembleRelease -q` (long timeout), then `zipalign -f -p 4 app/build/outputs/apk/release/app-release-unsigned.apk ~/tmp/rtsign/al120.apk && apksigner sign --ks ~/.android/debug.keystore --ks-pass pass:android --key-pass pass:android --out ~/tmp/rtsign/raytrace-1.2.0-arm64.apk ~/tmp/rtsign/al120.apk && apksigner verify --print-certs ~/tmp/rtsign/raytrace-1.2.0-arm64.apk | grep SHA-256` (starts `9d30a1f9`). Also build the DEBUG APK for Task 8 (`assembleDebug`).
- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat: GI resolution setting and presets; docs; version 1.2.0

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`

---

### Task 8: Device A/B verification, benchmarks, release (controller-gated: needs the user's phone)

**Files:** `docs/bench.md`, `README.md`, `README.ru.md`, `~/projects/releases/raytrace-1.2.0.md`

Run ONLY when the controller states that the user allows a short device session. If the phone is not available, stop after Step 0 and report; the controller will schedule it.

- [ ] **Step 0: Preconditions** — Task 7 committed; debug APK built at `app/build/outputs/apk/debug/app-debug.apk`.
- [ ] **Step 1: Install the debug build** — `su -c "cp $PWD/app/build/outputs/apk/debug/app-debug.apk /data/local/tmp/rt-dbg.apk && pm install -r /data/local/tmp/rt-dbg.apk"`.
- [ ] **Step 2: A/B in one session, back to back** (each: `su -c "pm clear dev.starinin.raytrace; logcat -c; am start -n dev.starinin.raytrace/.MainActivity --es dbg '<dbg>'"`, wait 10 s, `logcat -d -s raytrace | grep fps | tail -3`, one `screencap -p /sdcard/claude/gi-<name>.png`, then `am force-stop` + return to Termux). Common settings: `19=0,18=0,2=0,1=1` (fixed camera, animation off, fixed 0.33x). Runs: `A: 30=0` (inline GI, same as 1.1.0), `B: 30=1`, `C: 30=2`, `D: 30=1,12=0` (GI off, control), plus the same `A,B` once more in reverse order (`B` then `A`) to expose drift. Record gpu ms, fps and (if the HUD/log shows them) per-pass times. Repeat `30=1` with the clean-defaults setting (no dbg, release preset Balanced defaults) once for the adaptive-floor figure.
- [ ] **Step 3: Image fidelity** — decode two screenshots (A vs B, and A vs C) with a stdlib-only PNG reader (zlib/struct; write the script to `~/tmp/imgstat.py`, do not commit it) and compare mean RGB inside fixed regions (floor near the green wall, floor near the orange wall, back wall centre, the Menger cube face) and the global mean; acceptance: each region's mean within 5% of A, and B/C show the green/orange bleeding. View the screenshots with the Read tool at object silhouettes (sphere/torus edges) for light leaks; report honestly what you see.
- [ ] **Step 4: Sanity** — path-tracing (`0=1`) and GI-off hybrid (`12=0`) run without errors; mirror/glass spheres look right (no double GI brightening); toggling `30` in sequence across launches never crashes; `logcat -d | grep -iE 'FATAL|panic|ANR in'` is empty.
- [ ] **Step 5: Docs** — append a `1.2 GI` section to `docs/bench.md` with the A/B table (fps, gpu ms, scale, conditions, order, date) and the fidelity numbers; update README/README.ru benchmark lines with ONLY measured numbers (state "Balanced default" figure from the clean-defaults run and the speedup B vs A measured back-to-back); update `~/projects/releases/raytrace-1.2.0.md`. If the speedup is below 30% or fidelity fails the 5% bound, say so plainly in the docs and the report; do not publish claims.
- [ ] **Step 6: Restore the phone** — install the release build `su -c "cp ~/tmp/rtsign/raytrace-1.2.0-arm64.apk /data/local/tmp/rt-final.apk && pm install -r /data/local/tmp/rt-final.apk && pm clear dev.starinin.raytrace"` (rebuild + re-sign first if docs/version changed the APK: they do not, only the debug/release build output matters), Termux in the foreground.
- [ ] **Step 7: Commit** — `git add -A && git commit -m "docs: 1.2 GI benchmarks

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`. Do not push or tag: the controller publishes after the final review.

---

## Self-review (done while writing)
- **Spec coverage:** pipeline steps 1-7 (Tasks 3-5), shader split (Task 2), params/flags/UI/presets (Tasks 1, 7), textures and `ensure_targets` (Task 5), timing (Task 6, deviation: results read after the existing `poll(Wait)` instead of one frame late), error handling (odd sizes Task 1 tests + Task 4 clamps; first frame/reset via `history_reset`; invalid texels; checkerboard Task 3), testing (Tasks 1, 2, 4, 6 host; Task 8 device), risks documented in Task 7 docs, release 1.2.0 (Tasks 7-8 + controller).
- **Type consistency:** `gi_block` is u32 pixels-per-side (0/2/4) in `Store::gi_block`, `GpuParams.gi_block`, WGSL `P.gi_block`, `ensure_targets(.., gi_block)`; `gi_floor` f32 = 0.15; texel formats f16; `block_offset` identical in `gi.rs` and `common.wgsl`; `alb.a` semantics identical in Tasks 3, 4; GpuParams size 192 (assert in Task 1).
- **Placeholders:** none.
