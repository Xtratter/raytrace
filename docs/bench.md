# Bench

- prototype hybrid 0.33x: 7.1 fps | render 356x767 | hybrid (steady state, last log lines 6.4-7.2 fps)
- task 5 hybrid 0.33x (dbg 6=0,8=0,2=0): 88.9-95.2 fps | gpu 9.6-10.0 ms | render 352x792 | hybrid (~13x the prototype's 7.1 fps; caveat: prototype was 356x767 with accumulation shader, different pass structure)
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

  Before (task 8/6): Balanced hybrid ~15 fps at the 0.25 floor, 9 fps at 0.33x fixed; Performance preset (with checker) 12.7 fps at 0.25 after the GI work alone (checker slowdown), path 0.9 fps. Balanced stays at the 0.25 adaptive floor; at fixed 0.33x it is ~17 fps / 58 ms (acceptance of 30 fps at 0.33x is NOT met; met at 0.25x). Balanced 60 s auto-orbit: 34-40 fps throughout, no drift, no FATAL/panic/ANR. Quality guard: green tint on floor/spheres and orange on the right remain visible; trade-offs: Menger casts a box-shaped shadow on the primary hit, GI hits use sphere-approximated shadows, blue speckle on the Menger remains.
