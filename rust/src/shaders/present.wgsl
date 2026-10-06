@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
  let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
  return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
  let c = textureSampleLevel(src, smp, pos.xy / P.out_size, 0.0).rgb;
  let m = c / (c + 1.0);
  var o = m;
  if ((P.flags & F_SRGB) == 0u) { o = pow(m, vec3<f32>(1.0 / 2.2)); }
  return vec4<f32>(o, 1.0);
}
