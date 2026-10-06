// Temporal reprojection + neighbourhood clamp. Mirrors reproj::blend (CPU reference).
// Flags are tested via has(f) (global P); never pass the Params struct by value (Adreno miscompile).
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var raw_tex: texture_2d<f32>;
@group(0) @binding(2) var g_cur: texture_2d<f32>;
@group(0) @binding(3) var g_prev: texture_2d<f32>;
@group(0) @binding(4) var hist_in: texture_2d<f32>;
@group(0) @binding(5) var hist_out: texture_storage_2d<rgba16float, write>;

const LEN_CAP_STILL: f32 = 2048.0;

fn load_hist(p: vec2<i32>, lim: vec2<i32>) -> vec4<f32> {
  return textureLoad(hist_in, clamp(p, vec2<i32>(0), lim), 0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let res = P.res;
  let lim = vec2<i32>(res) - vec2<i32>(1);
  let ip = vec2<i32>(gid.xy);
  if (ip.x > lim.x || ip.y > lim.y) { return; }

  let cur = textureLoad(raw_tex, ip, 0);
  let gc = textureLoad(g_cur, ip, 0);
  let traced = cur.a > 0.5;

  // 3x3 moments over traced neighbours
  var m1 = vec3<f32>(0.0);
  var m2 = vec3<f32>(0.0);
  var cnt = 0.0;
  for (var dy = -1; dy <= 1; dy = dy + 1) {
    for (var dx = -1; dx <= 1; dx = dx + 1) {
      let s = textureLoad(raw_tex, clamp(ip + vec2<i32>(dx, dy), vec2<i32>(0), lim), 0);
      if (s.a > 0.5) {
        let c = sanitize(s.rgb);
        m1 += c;
        m2 += c * c;
        cnt += 1.0;
      }
    }
  }
  var mean = sanitize(cur.rgb);
  var sigma = vec3<f32>(0.0);
  if (cnt > 0.0) {
    mean = m1 / cnt;
    sigma = sqrt(max(m2 / cnt - mean * mean, vec3<f32>(0.0)));
  }
  let col = select(mean, sanitize(cur.rgb), traced);

  let still_pt = P.mode == 1u && has(F_STILL);
  var hist = vec3<f32>(0.0);
  var n = 0.0;
  if (still_pt && has(F_TEMPORAL) && P.history_reset == 0u) {
    // camera + scene identical: exact texel, no reprojection/bilinear (avoids repeated low-pass)
    let h4 = textureLoad(hist_in, ip, 0);
    hist = sanitize(h4.rgb);
    n = h4.a;
    if (!(n > 0.0) || n != n) { n = 0.0; }
  } else if (has(F_TEMPORAL) && P.history_reset == 0u) {
    // same pixel convention as trace.wgsl: pixel + 0.5 + jitter
    let rd = cam_ray(vec2<f32>(ip) + vec2<f32>(0.5) + P.jitter, res, P.cam_pos, P.cam_target, P.fov);
    let wp = P.cam_pos + rd * gc.x;
    let pp = cam_project(wp, res, P.prev_pos, P.prev_target, P.fov) - P.prev_jitter;
    if (pp.x >= 0.0 && pp.y >= 0.0 && pp.x < res.x && pp.y < res.y) {
      let ipp = clamp(vec2<i32>(floor(pp)), vec2<i32>(0), lim);
      let gp = textureLoad(g_prev, ipp, 0);
      let dprev = length(wp - P.prev_pos);
      let ok = abs(gp.x - dprev) < 0.04 * dprev + 0.02
            && dot(oct_decode(gc.yz), oct_decode(gp.yz)) > 0.85
            && gp.w == gc.w;
      if (ok) {
        let f = pp - vec2<f32>(0.5);
        let i0 = vec2<i32>(floor(f));
        let t = f - floor(f);
        let a = load_hist(i0, lim);
        let b = load_hist(i0 + vec2<i32>(1, 0), lim);
        let c = load_hist(i0 + vec2<i32>(0, 1), lim);
        let d = load_hist(i0 + vec2<i32>(1, 1), lim);
        let h4 = mix(mix(a, b, t.x), mix(c, d, t.x), t.y);
        hist = sanitize(h4.rgb);
        n = h4.a;
        if (!(n > 0.0) || n != n) { n = 0.0; }
      }
    }
  }

  let moved = has(F_MOVED);
  let floor_w = P.hist_floor;
  var out_col = col;
  var out_n = 1.0;
  if (n > 0.0) {
    if (!still_pt && cnt > 0.0) {
      hist = clamp(hist, mean - sigma * 1.25, mean + sigma * 1.25);
    }
    let nn = min(n, LEN_CAP_STILL) + 1.0;
    var a = select(max(1.0 / nn, floor_w), 1.0 / nn, still_pt);
    if (!still_pt && moved && (gc.w == 1.0 || gc.w == 2.0)) { a = max(a, 0.6); }
    let cap = select(ceil(1.0 / floor_w), LEN_CAP_STILL, still_pt);
    if (traced) {
      out_col = mix(hist, col, a);
      out_n = min(nn, cap);
    } else {
      out_col = hist;      // checkerboard: keep history for pixels not traced this frame
      out_n = min(n, cap);
    }
  }
  textureStore(hist_out, ip, vec4<f32>(out_col, out_n));
}
