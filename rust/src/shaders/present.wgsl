@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
  let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
  return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn aces(x: vec3<f32>) -> vec3<f32> {
  return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

// 9-tap Catmull-Rom (bilinear-optimised): sharp upscale from the low-res render
fn sample_cr(uv: vec2<f32>) -> vec3<f32> {
  let sz = P.res;
  let pos = uv * sz;
  let c = floor(pos - 0.5) + 0.5;
  let f = pos - c;
  let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
  let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
  let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
  let w3 = f * f * (-0.5 + 0.5 * f);
  let w12 = w1 + w2;
  let o12 = w2 / w12;
  let p0 = (c - 1.0) / sz;
  let p3 = (c + 2.0) / sz;
  let p12 = (c + o12) / sz;
  var r = textureSampleLevel(src, smp, vec2<f32>(p0.x, p0.y), 0.0).rgb * w0.x * w0.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p12.x, p0.y), 0.0).rgb * w12.x * w0.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p3.x, p0.y), 0.0).rgb * w3.x * w0.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p0.x, p12.y), 0.0).rgb * w0.x * w12.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p12.x, p12.y), 0.0).rgb * w12.x * w12.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p3.x, p12.y), 0.0).rgb * w3.x * w12.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p0.x, p3.y), 0.0).rgb * w0.x * w3.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p12.x, p3.y), 0.0).rgb * w12.x * w3.y;
  r += textureSampleLevel(src, smp, vec2<f32>(p3.x, p3.y), 0.0).rgb * w3.x * w3.y;
  return max(r, vec3<f32>(0.0));
}

// P.exposure is in EV (renderer converts half-stops); 0 = ACES, 1 = Reinhard, 2 = clamp.
fn tone(c: vec3<f32>) -> vec3<f32> {
  let e = c * exp2(P.exposure);
  if (P.tonemap == 0u) { return aces(e); }
  if (P.tonemap == 1u) { return e / (e + 1.0); }
  return clamp(e, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn hash12(p: vec2<f32>) -> f32 {
  var q = fract(vec3<f32>(p.xyx) * 0.1031);
  q += dot(q, q.yzx + 33.33);
  return fract((q.x + q.y) * q.z);
}

// 4x4 ordered-dither threshold in [0,1) from the pixel parity bits (no array indexing)
fn bayer4(p: vec2<u32>) -> f32 {
  let x = p.x & 3u;
  let y = p.y & 3u;
  let a = x ^ y;
  let v = ((a & 1u) << 3u) | ((y & 1u) << 2u) | (((a >> 1u) & 1u) << 1u) | ((y >> 1u) & 1u);
  return (f32(v) + 0.5) * (1.0 / 16.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
  let uv = pos.xy / P.out_size;
  if ((P.flags & F_PS1) != 0u) {
    // hard pixels (no filtering), 5 bits per channel with ordered dither on the source pixel grid
    let ip = vec2<u32>(min(floor(uv * P.res), P.res - vec2<f32>(1.0)));
    var c = tone(textureLoad(src, vec2<i32>(ip), 0).rgb);
    if ((P.flags & F_SRGB) == 0u) { c = pow(c, vec3<f32>(1.0 / 2.2)); }
    c = floor(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * 31.0 + vec3<f32>(bayer4(ip))) * (1.0 / 31.0);
    return vec4<f32>(c, 1.0);
  }
  var o = tone(sample_cr(uv));
  if (P.sharpen > 0.0) {
    // unsharp mask in display space (after tonemap)
    let px = 1.0 / P.res;
    let avg = (tone(textureSampleLevel(src, smp, uv + vec2<f32>(px.x, 0.0), 0.0).rgb)
             + tone(textureSampleLevel(src, smp, uv - vec2<f32>(px.x, 0.0), 0.0).rgb)
             + tone(textureSampleLevel(src, smp, uv + vec2<f32>(0.0, px.y), 0.0).rgb)
             + tone(textureSampleLevel(src, smp, uv - vec2<f32>(0.0, px.y), 0.0).rgb)) * 0.25;
    o = clamp(o + (o - avg) * P.sharpen, vec3<f32>(0.0), vec3<f32>(1.0));
  }
  if ((P.flags & F_SRGB) == 0u) { o = pow(o, vec3<f32>(1.0 / 2.2)); }
  o += (hash12(pos.xy) - 0.5) / 255.0;
  return vec4<f32>(o, 1.0);
}
