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
    // invalid sample: keep the existing history (no length increment) unless history was reset
    var keep = vec4<f32>(0.0);
    if (P.history_reset == 0u) { keep = textureLoad(gi_hist_in, gp0, 0); }
    textureStore(gi_hist_out, gp0, keep);
    return;
  }
  let off = block_offset(gid.x, gid.y, P.frame, bs);
  let fp = min(vec2<i32>(i32(gid.x * bs + off.x), i32(gid.y * bs + off.y)), flim);
  // effective in-block offset after clamping fp to the frame (border blocks)
  let off_eff = vec2<f32>(f32(fp.x - i32(gid.x * bs)), f32(fp.y - i32(gid.y * bs)));
  let gc = textureLoad(g_cur, fp, 0);
  let c = sanitize(cur.rgb);
  var hist = vec3<f32>(0.0);
  var n = 0.0;
  if (P.history_reset == 0u) {
    // Jitter is deliberately omitted (here and in pp below): GI is low-frequency, and the per-frame
    // Halton jitter would otherwise shift the history lookup by up to +-1 px and blur it every frame.
    let rd = cam_ray(vec2<f32>(fp) + vec2<f32>(0.5), P.res, P.cam_pos, P.cam_target, P.fov);
    let wp = P.cam_pos + rd * gc.x;
    let pp = cam_project(wp, P.res, P.prev_pos, P.prev_target, P.fov);
    if (pp.x >= 0.0 && pp.y >= 0.0 && pp.x < P.res.x && pp.y < P.res.y) {
      let ipp = clamp(vec2<i32>(floor(pp)), vec2<i32>(0), flim);
      let gp = textureLoad(g_prev, ipp, 0);
      let dprev = length(wp - P.prev_pos);
      let ok = abs(gp.x - dprev) < 0.04 * dprev + 0.02
            && dot(oct_decode(gc.yz), oct_decode(gp.yz)) > 0.85
            && gp.w == gc.w;
      if (ok) {
        // history texels live at block centres: shift pp to the block-centre equivalent
        let pc = pp + vec2<f32>(f32(bs) * 0.5) - off_eff - vec2<f32>(0.5);
        let f = pc / f32(bs) - vec2<f32>(0.5);
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
