# Bench

- prototype hybrid 0.33x: 7.1 fps | render 356x767 | hybrid (steady state, last log lines 6.4-7.2 fps)
- task 5 hybrid 0.33x (dbg 6=0,8=0,2=0): 88.9-95.2 fps | gpu 9.6-10.0 ms | render 352x792 | hybrid (~13x the prototype's 7.1 fps; caveat: prototype was 356x767 with accumulation shader, different pass structure)
- flag toggles (anim off, 6=0,8=0,2=0): default 93.2 fps / 9.9 ms; 14=0 93.7 / 10.2; 12=0 93.0 / 9.7; 13=0 93.1 / 9.8; 23=2 85.5 / 10.4; 11=0 85.4 / 11.1 (single last-line samples, noisy)
