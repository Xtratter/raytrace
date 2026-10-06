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
