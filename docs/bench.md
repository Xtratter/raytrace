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
