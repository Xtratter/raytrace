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
