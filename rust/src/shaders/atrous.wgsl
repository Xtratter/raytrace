// Edge-aware a-trous wavelet filter (one iteration per dispatch; step = 1, 2, 4).
@group(0) @binding(0) var<uniform> A: vec4<u32>;   // x = step, y = width, z = height
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var gb: texture_2d<f32>;     // current gbuffer: x depth, yz oct normal, w object id
@group(0) @binding(3) var dst: texture_storage_2d<rgba16float, write>;
// Bound (but unused) so that common.wgsl's has() resolves its global `P`.
@group(0) @binding(4) var<uniform> P: Params;

fn lum(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }

// B3-spline taps (1, 4, 6, 4, 1) / 16 for offset -2..2. Computed instead of a dynamically indexed
// local array: the Adreno 650 driver segfaulted compiling the array version.
fn kw(o: i32) -> f32 {
  let a = abs(o);
  return select(select(0.0625, 0.25, a == 1), 0.375, a == 0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let lim = vec2<i32>(i32(A.y), i32(A.z)) - vec2<i32>(1);
  let ip = vec2<i32>(gid.xy);
  if (ip.x > lim.x || ip.y > lim.y) { return; }
  let c0 = sanitize(textureLoad(src, ip, 0).rgb);
  let g0 = textureLoad(gb, ip, 0);
  let n0 = oct_decode(g0.yz);
  let l0 = lum(c0);
  var sum = vec3<f32>(0.0);
  var wsum = 0.0;
  let step = i32(A.x);
  for (var j = -2; j <= 2; j = j + 1) {
    for (var i = -2; i <= 2; i = i + 1) {
      let q = clamp(ip + vec2<i32>(i, j) * step, vec2<i32>(0), lim);
      let c = sanitize(textureLoad(src, q, 0).rgb);
      let g = textureLoad(gb, q, 0);
      let wn = pow(max(dot(n0, oct_decode(g.yz)), 0.0), 24.0);
      let wz = exp(-abs(g.x - g0.x) / (0.05 * g0.x + 0.01));
      // Relative luminance stop (sigma ~0.14): no albedo in the gbuffer, so texture edges (checker)
      // are only protected by this term; residual temporal noise (~5-10%) still passes.
      let dl = abs(lum(c) - l0) / (max(lum(c), l0) + 0.02);
      let wl = exp(-dl * dl * 25.0);
      let wid = select(0.0, 1.0, g.w == g0.w);
      let w = kw(i) * kw(j) * wn * wz * wl * wid;
      sum += c * w;
      wsum += w;
    }
  }
  textureStore(dst, ip, vec4<f32>(sum / max(wsum, 1e-5), 1.0));
}
