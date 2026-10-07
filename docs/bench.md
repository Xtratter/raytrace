# Bench

- prototype hybrid 0.33x: 7.1 fps | render 356x767 | hybrid (steady state, last log lines 6.4-7.2 fps)
- task 5 hybrid 0.33x (dbg 6=0,8=0,2=0): 88.9-95.2 fps | gpu 9.6-10.0 ms | render 352x792 | hybrid (SUPERSEDED, invalid, see CORRECTION below; caveat: prototype was 356x767 with accumulation shader, different pass structure)
- flag toggles (anim off, 6=0,8=0,2=0): default 93.2 fps / 9.9 ms; 14=0 93.7 / 10.2; 12=0 93.0 / 9.7; 13=0 93.1 / 9.8; 23=2 85.5 / 10.4; 11=0 85.4 / 11.1 (single last-line samples, noisy)
- CORRECTION (task 5 fix): the 88-95 fps figure above was invalid. `has(P, F_*)` (uniform struct passed by value) evaluated wrong on the phone GPU, so reflections/GI/caustics/shadows were effectively off. Fixed (`has(f)` reads the global P). Real numbers, 352x792 hybrid, spp 1, bounces 6, 6=0 8=0 2=0:
  - all features, GI march 72 steps: 3.3 fps | gpu 303 ms (below the 7.1 baseline)
  - 12=0 (no GI): 29.4 fps | 32.5 ms; 11=0: 6.1 fps | 163 ms; 13=0: 3.3 fps; 14=0: 3.6 fps | 277 ms; all four off: 93.5 fps | 9.6 ms
  - after cheaper GI rays (24-step budget, Menger replaced by its bounding box in `map_gi`): 9.4 fps | gpu 104 ms (above the 7.1 baseline)
- task 6 temporal pass (352x792, 0=0 2=0 8=0 18=0 10=0, spp 1, bounces 6): 6=1 still 9.3-9.5 fps / gpu 106-111 ms; 6=0 still 9.2-9.3 fps / 106-108 ms (temporal pass cost within noise). Orbit 19=2: 8.6-8.9 fps both.
  Noise (screenshot luminance, high-pass residual std; wall / floor): still 6=0 18.6 / 15.9 -> 6=1 3.8 / 4.2; orbit 6=0 18.4 / 13.3 -> 6=1 4.0 / 3.9.
  Path tracing still (0=1): 0.9 fps / gpu ~1160 ms; wall/floor hp-std 14.1 / 12.4 at 4 s -> 2.2 / 2.5 at 20 s; no NaN log lines.
- task 7 a-trous + full present (352x792 hybrid, 6=1 still, 0=0 2=0 18=0 19=0, spp 1, bounces 6): 8=0 9.2-9.3 fps / gpu 106-111 ms; 8=2 8.9 fps / 111-112 ms; 8=3 8.8 fps / 112-115 ms (3 passes cost ~5 ms, ~4%). Sharpen 9=3: within noise.
  Screenshot metrics (hp-std wall / green wall; p99 gradient floor): 8=0 6.0 / 5.7, 22.0; 8=2 1.9 / 1.3, 28.6; 8=3 0.9 / 0.7, 29.1 (checker edges kept). First weights (wl = exp(-1.5|dl|)) gave 0.6 grain but blurred the checker: p99 floor 8.5.
- task 8 adaptive + frame limit (device, POCO F3, display 120 Hz, Mailbox+Fifo available):
  - heavy (0=0 1=4 2=1 3=60, all features on, bounces 6): scale at the 0.25 floor (264x600) by the first 2 s log, then flat 15.0 fps / gpu 63-68 ms for 16 s (target unreachable, no oscillation). Non-adaptive 1=4 2=0: 1.1 fps / gpu 915 ms at 1080x2400.
  - light (12=0 13=0 14=0 11=0): target 30 -> settles 0.49x (520x1168), 36 fps / gpu 26.4 ms (15% headroom design); target 60 -> 0.32x (344x760), 70 fps / gpu 13.2 ms. Stable, one transition each.
  - frame limit (light, 0.25x, adaptive off): 25=0 100 fps, 25=1 99 fps (GPU-bound ~9 ms, display 120 Hz, so no visible difference); 0.33x: 67 fps either way.
  - extremes: 1=0,2=1: 99-100 fps; 1=4,2=0: 1.1 fps; no crash. Landscape rotation (2400x1080): surface reconfigured, rendering continues (75 fps, 0.29x), no panic.
- task 10 perf tuning (352x792 = 0.33x fixed, hybrid, all features, 6=1 8=1, still): before 9.0 fps / 108 ms -> GI hit lit with analytic sphere shadows + map_gi normal 16.2 fps / 60 ms -> GI_STEPS 12 / range 10 (+grey instead of sky for exhausted rays) -> primary shadow rays on map_gi (box instead of Menger), 16 steps. Feature costs at 0.25x after: shadows were ~12 ms (dominant once GI was cheap), reflections ~1 ms, GI ~12 ms.
- checkerboard fix: skipped pixels used a duplicated march+normal path; the shader grew and ran ~3x slower with checker on (42 ms vs 15 ms at 0.25x). Now they share shade_pixel/trace and stop after the primary hit: 11 ms. Checker still looks grainier (half the samples), so Performance keeps it off.
- auto-orbit (19=1/2) had not been device-checked before task 10; checked once (19=2, two screenshots 3 s apart differ, camera swings, no errors).
- Final benchmark (pm clear, fresh process, 12 s, last log line; scale is the adaptive result unless noted; 19=0 still / 19=2 orbit):

| preset | mode | still gpu ms / fps / scale | orbit fps |
|---|---|---|---|
| Performance | hybrid | 12.0 / 79 / 0.25 | 80 |
| Performance | path | 23.9 / 41 / 0.25 | 39 |
| Balanced | hybrid | 25.0 / 40 / 0.25 | 40 |
| Balanced | path | 182 / 5.5 / 0.25 | 5.4 |
| Quality (0.5x fixed, spp 2, bounces 9) | hybrid | 164 / 6.0 / 0.50 | 6.1 |
| Quality | path | 1674 / 0.6 / 0.50 | 0.6 |

  Before/after (task 10; still, hybrid unless noted; "before" = task 6/8 measurements, which used different scales, so only same-row comparisons are meaningful): Balanced adaptive: ~15 fps -> 40 fps (table above, 0.25x floor); Balanced fixed 0.33x: 9.0 fps / 108 ms -> 17 fps / 58 ms; Performance (checker preset): 12.7 fps -> 79 fps (checker off in the final preset); Balanced path: 0.9 fps (0.33x, prototype era) -> 5.5 fps (0.25x). Balanced stays at the 0.25 adaptive floor; the 30 fps acceptance is NOT met at fixed 0.33x, met at 0.25x. Balanced 60 s auto-orbit: 34-40 fps throughout, no drift, no FATAL/panic/ANR. Quality guard: green tint on floor/spheres and orange on the right remain visible; trade-offs: GI hits use sphere-approximated shadows (mirror, glass, two blobs), blue speckle on the Menger remains.
- fix round 1 (review, host-side only): primary shadow rays for Menger hits (id 5) use the exact map (the map_gi proxy box used to shadow the sponge's own surface, black recessed faces); GI-hit shadows use the two real blob spheres (a blob hit skips them). The perf/visual re-measurement on device for these changes is PENDING (device busy). An earlier experiment recorded 71 ms vs 41 ms at fixed 0.33x for an exact-map Menger shadow (divergence); re-check that cost.
- Known issue, NOT investigated: dotted lines along the room wall edges.
- Note: the Final benchmark table above and the "40 fps" Balanced figures were measured during development under different (cooler / debug-override) conditions and could not be reproduced on clean defaults; see the final check below.

## Final check (06.10.2026)

POCO F3 (Adreno 650), 18:21-18:25, release build, clean defaults: Balanced preset, hybrid, adaptive target 60 at the 0.25x floor, render size 264x600.
- Measured 22-23 fps / GPU 42-44 ms, repeated on several runs (the 30 fps target was NOT met).
- Debug build, fixed camera, animation off, same session: 17.7 fps / 55 ms.
- GPU clock / thermal state changes results by roughly 25% between sessions.
- The earlier Balanced figure of about 40 fps / 25 ms (0.25x) could NOT be reproduced on clean defaults; it was measured during development under different (cooler / debug-override) conditions. Performance, Quality and path-tracing numbers above are from those development runs and were not re-measured; treat them as approximate.
- Menger-sponge shadow step budget (MENGER_SHADOW_STEPS = 24) vs the exact-march variant: no measurable difference (22.4 vs 22.5 fps).
- Comparison with the prototype (7.1 fps, 356x767, hybrid, before any changes): about 3x on clean defaults (22.5 vs 7.1) at a lower internal resolution (0.25x vs 0.33x) with better image quality (temporal reprojection, denoiser). The earlier 13x / 90+ fps claims are withdrawn.
- A safety governor scales heavy settings down automatically when a frame takes more than ~1.2 s.


## 1.2 half-resolution GI (results)

**1.2 half-resolution GI, measured on a device** (POCO F3 / Adreno 650, on-device session 07.10.2026, debug build of commit 6ca912e = version 1.2.0). Hybrid mode, fixed 0.33x render size 352x792, bounces 6, GI/shadows/caustics/reflections on, camera fixed, animation off, adaptive off, HUD Full. Sequence run back to back: gi_res 0, 1, 2, 1, 0. GPU clocks/thermal state vary by roughly 25% between sessions, so only these back-to-back numbers are compared.

| GI resolution | fps | GPU ms | Note |
|---|---:|---:|---|
| 0 Full (GI inline in the trace pass, 1.1.0 behaviour) | 10.4 | 92.7-97.3 | two runs |
| 1 Half | 14.9-15.4 | 62.5-65.5 | two runs; +46% fps (10.4 -> 15.2), about -33% GPU time vs Full |
| 2 Quarter | 15.7-16.1 | 58.9-60.5 | +53% fps vs Full |
| GI off (control, 12=0) | 19.3 | 51.5 | |

- Per-pass times from the HUD, Full: trace 90.0, temporal 0.7, atrous 2.0, present 2.4 ms.
- Per-pass times, Half: trace 51.9, gi_trace 0.4, gi_temporal 0.3, gi_atrous 3.4, temporal 0.7, atrous 2.0, composite 0.5, present 2.4 ms.
- Quarter is only slightly better than Half because the trace pass (primary, direct light, caustics, reflections), not GI, is now the dominant cost: about 52 ms of about 62 ms at Half. This is the next bottleneck.
- Per-pass GPU timestamps are supported on the Adreno 650 (HUD Full shows them).
- Denoiser passes = 3 with Half: 14.3-14.5 fps, no crash (the timestamp fix works with timestamps supported on this GPU).
- Clean defaults (release build, Balanced preset, adaptive resolution at its 0.25x floor, 264x600): 24-25 fps, GPU 36-38 ms. The earlier 1.1.0 clean-defaults figure (22-23 fps) was measured in a different session, so this is not a controlled comparison and no speedup is claimed from these two numbers.

Image fidelity, Full vs Half (mean RGB of fixed screenshot regions, 1080x2400 screenshots):

| Region | Full | Half |
|---|---|---|
| Floor near the green wall | 155.2 / 149.4 / 141.4 | 154.6 / 148.8 / 140.7 |
| Floor near the orange wall | 128.9 / 126.7 / 126.2 | 128.0 / 125.9 / 125.8 |
| Back wall centre | 182.5 / 178.4 / 170.2 | 182.5 / 178.5 / 170.8 |
| Whole image | 140.6 / 150.3 / 140.4 | 140.2 / 149.9 / 140.2 |
| Menger cube face | 88.7 / 126.4 / 172.8 | 90.1 / 128.7 / 176.8 |

All regions are within about 1% (cube face about 2%); repeating Full twice differs by less than 1%. With GI off the same regions differ by 5-17% (e.g. whole image 120.9 / 127.4 / 116.6), so the comparison is meaningful. A visual check of silhouettes (spheres, torus, Menger cube) showed no visible halos or leaks at Half with a still camera. Moving-camera behaviour and Quarter softness were not specifically examined.


## Results 1.3

**1.3 analytic shadow rays, measured on a device** (POCO F3 / Adreno 650, on-device session 07.10.2026, debug build of commit d66dfc1 = version 1.3.0). Hybrid mode, Half-resolution GI, fixed 0.33x render size 352x792, GI/shadows/caustics/reflections on, camera fixed, animation off, adaptive off, HUD Full. GPU clocks/thermal state vary by roughly 25% between sessions, so only same-session comparisons are meaningful. The 1.2.0 baseline was measured earlier the same day with the same settings.

| Build / setting | fps | GPU ms | Trace pass ms |
|---|---:|---:|---:|
| 1.2.0 baseline, bounces 6 | 15.4 (15.2 repeat) | 62.7 (63.4) | 52.5 (52.6) |
| 1.3.0, bounces 6 (isolates the shadow change) | 20.8 / 20.4 (two runs) | 47.2 / 44.1 | 36.7 / 33.5 (HUD screenshot 34.2) |
| 1.3.0, Balanced default bounces 4 | 20.7 | 45.4 | 34.7 |
| 1.3.0, shadows off (control) | 24.3 | 37.3 | 27.1 |

- 1.3.0 vs 1.2.0 at equal bounces (6): about +35% fps, about -28% GPU time, trace pass about -30%.
- Bounces 6 -> 4 gives only about 2 ms in this scene; the shadow change is the real win.
- Shadows still cost about 10 ms of the trace pass in 1.3.0 (control above); they were about 24 ms in 1.2.0.
- Other 1.2.0 trace-pass costs measured earlier (context, 1.2.0): no caustics 43.6 ms; no reflections 39.5 ms; shadows + caustics + reflections all off 8.0 ms.
- The release build with 1.3.0 clean defaults was NOT measured.

Image fidelity vs 1.2.0 (mean RGB of fixed screenshot regions, bounces 6):

| Region | 1.2.0 | 1.3.0 |
|---|---|---|
| Floor near the green wall | 154.6 / 148.8 / 140.7 | 154.0 / 147.6 / 137.7 |
| Floor near the orange wall | 128.0 / 125.9 / 125.8 | 126.5 / 124.6 / 124.9 |
| Back wall centre | 182.5 / 178.5 / 170.8 | 182.2 / 178.2 / 170.6 |
| Whole image | 140.2 / 149.9 / 140.2 | 139.2 / 148.9 / 139.2 |
| Menger cube face | 90.1 / 128.7 / 176.8 | 88.0 / 126.0 / 174.6 |

All regions are within about 1-2%, slightly darker: exact sponge/blob shadows replace the old solid-box and sphere approximations, and light now passes through the sponge's holes. Visual check: shadows of spheres, torus and Menger cube are present and plausible; no new artifacts with a still camera. Moving-camera behaviour was not specifically examined.
