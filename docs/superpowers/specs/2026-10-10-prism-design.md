# Prism scene: dispersion and rainbow caustics (design)

Date: 2026-10-10. Target: 1.8.0. Builds on the scene system (docs/scenes.md, rust/src/scenegen.rs).

## Goal

A built-in scene "Prism": sunlight enters a dark room through a slit, passes a glass prism and fans out into a spectrum on the floor and a wall. The prism itself shows coloured edges to the camera. It must run on the phone (Adreno 650) with the hybrid renderer, and on the Linux build.

Decisions made with the user: forward photons from the sun (not path-sampled, not painted analytically); receivers are the floor plus one wall; real dispersion formula with a `dispersion` multiplier (built-in scene uses about 2.5).

## Data

- Object `"prism"`: `pos`, `size` (side of the equilateral base), `length`, `spin` (rad/s about y, usually 0), `yaw` (fixed angle about y, radians). Apex up. Counts as a "complex" shape (limit shared with torus/menger/cylinder/blobs).
- Glass materials get `"ior"` (default 1.5) and `"dispersion"` (default 0 = none). Refractive index for wavelength `l` in micrometres: `n(l) = ior + 0.012 * dispersion * ((0.589 / l)^2 - 1)` (1.0 matches crown glass: n(0.4) - n(0.7) is about 0.018).
- Scene key `"caustic_receivers"`: up to 2 rectangles `{"origin": [x,y,z], "u": [..], "v": [..], "cells": [nu, nv]}` (origin plus two edge vectors; the map covers that parallelogram, normal = u x v). Built-in prism scene: floor and the right wall.
- `scene` setting: 0 Classic, 1 Sun room, 2 Materials, 3 Prism, 4 = file the user loaded (was 3). `params.json`, Rust and Kotlin tables change together; the saved "file" selection of old installs falls on Prism.

## Photon pass (new compute pass, runs before trace when the scene has a dispersive prism, a sun and receivers)

1. `photon.wgsl`: each thread traces K photons. A photon picks a point on the disc perpendicular to the sun direction that covers the prism's bound sphere, a direction inside the sun cone, and a wavelength in 380..780 nm. The ray is clipped against the prism's three side planes and two end caps (exact convex polyhedron), refracted with `n(l)`; Fresnel decides reflect/transmit; at most 4 internal bounces.
2. The exit ray goes to the receiver rectangles (nearest hit inside the rectangle). Other solid objects are tested with the existing analytic `occ_all` (glass boxes tint, as in direct light).
3. The photon adds its RGB (CIE fit of the wavelength, normalised so the mean over wavelength is white) times power to the receiver's cell with `atomicAdd` on a fixed-point `u32` buffer (3 counters per cell). Photon power = `pi * sun_color * beam_area / N`, so that the cell irradiance divided by pi is in the same units as `direct_light`.
4. `caustic_resolve.wgsl`: per cell, `map = mix(map, counts * scale / cell_area, a)` into an `rgba16float` storage texture per receiver (exponential average, a about 0.15, reset on scene or sun change), then zeroes the counters.
5. Shading: `trace` and `gi_trace` read the maps with a manual bilinear fetch for hit points that lie on a receiver (within 2 mm of the plane, inside the rectangle) and add `albedo * map` to the light of that point. New bindings (read-only textures) in those two passes only.

Budget: N = 65536 photons per frame (256 x 256 threads, 1 photon each), tunable constant; cell resolution 256 x 256 per receiver.

## Prism in the picture

- Camera: SDF of a triangular prism extruded along its axis (2D triangle SDF plus slab) for marching; normals from the existing finite differences.
- Dispersive glass hits pick one wavelength per camera sample (hero wavelength, carried in the path); refraction uses `n(l)`, throughput gets the wavelength's RGB. Temporal accumulation averages it to white, leaving coloured fringes on edges.
- Shadow rays: the prism blocks direct sun like a glass sphere (opaque in `occ_all`); its light reaches the scene only through the photon maps (no double counting). In path-tracing mode a path that reaches a dispersive glass surface after a diffuse vertex terminates for the same reason.

## Scene "Prism"

Dark room (floor, back wall, left wall with a vertical slit, right wall used as the second receiver, ceiling), sun low through the slit (elevation about 12 degrees, angle 0.5 degrees for a crisp spectrum), a prism (apex up, side about 0.9, dispersion about 2.5, ior 1.5) on a small pedestal in the beam, a few neutral objects for context. Sun direction and prism yaw are chosen so that the exit fan lands on the floor and the right wall (checked by a unit test that runs the same refraction maths on the CPU for the central wavelength).

## Components and files

- `rust/src/scene.rs`: parse `prism`, `ior`/`dispersion`, `caustic_receivers`; limits.
- `rust/src/dispersion.rs` (new, host-testable): `ior(l, base, disp)`, wavelength to RGB, minimum-deviation helper for tests.
- `rust/src/scenegen.rs` + `shaders/scene.wgsl`: prism SDF, `occ` blocking, receiver lookup functions, per-wavelength refraction in `trace`.
- `rust/src/shaders/photon.wgsl`, `caustic_resolve.wgsl` (new) and `gfx.rs`: buffers, textures, two pipelines, per-scene recreation like trace.
- `rust/scenes/prism.json` (new), `rust/src/scene.rs` BUILTIN, params/Kotlin/desktop tables for the new scene id and renumbered file slot.
- Docs: `docs/scenes.md`, README (both languages), CHANGELOG (both), `docs/bench.md`.

## Tests

- Rust: `ior` values (400/589/700 nm), wavelength RGB sums to about white over the spectrum, parsing and error messages for the new keys, built-in scene count and ids, deviation of the central ray through the prism matches the closed-form minimum-deviation value for equal ior.
- naga validation of all shaders for every built-in scene (existing test extended with the new passes); buffer/binding sizes asserted.
- CI: desktop smoke run on lavapipe, one screenshot of the Prism scene checked by eye (spectrum on floor and wall).
- Phone (only when the user allows): first frame without device loss, fps against the Sun room, screenshot.

## Risks and fallbacks

- Adreno compile or frame time too high: lower N, lower map resolution, or drop the camera-side dispersion (keep photons).
- `atomicAdd` contention in the rainbow band: acceptable at this N; if needed, per-thread local accumulation over several photons before the add.
- Fixed-point overflow: scale chosen so that a full-power cell cannot reach 2^32; resolved by a unit test of the constant.
- Photon map is stale when the camera does not move but the sun changes: reset on scene or sun change (Sky setting).

## Out of scope

Rainbows on arbitrary objects, dispersion in other glasses and spheres, UI controls for photon count, spectral rendering of the whole scene.
