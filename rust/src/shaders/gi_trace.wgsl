// Half/quarter-resolution GI: one indirect() sample per block, taken at a rotating pixel of the block.
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var g_tex: texture_2d<f32>;
@group(0) @binding(2) var alb_tex: texture_2d<f32>;
@group(0) @binding(3) var gi_out: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let bs = P.gi_block;
  let gw = (u32(P.res.x) + bs - 1u) / bs;
  let gh = (u32(P.res.y) + bs - 1u) / bs;
  if (gid.x >= gw || gid.y >= gh) { return; }
  let off = block_offset(gid.x, gid.y, P.frame, bs);
  let flim = vec2<u32>(u32(P.res.x) - 1u, u32(P.res.y) - 1u);
  let fp = min(vec2<u32>(gid.x * bs + off.x, gid.y * bs + off.y), flim);
  let ip = vec2<i32>(fp);
  var o = vec4<f32>(0.0);
  let a = textureLoad(alb_tex, ip, 0);
  if (a.w > 0.5) {
    let g = textureLoad(g_tex, ip, 0);
    rng = pcg((gid.y * gw + gid.x + P.seed * 747796405u) ^ 0x9e3779b9u);
    rng = pcg(rng ^ 2747636419u);
    let rd = cam_ray(vec2<f32>(fp) + vec2<f32>(0.5) + P.jitter, P.res, P.cam_pos, P.cam_target, P.fov);
    let p = P.cam_pos + rd * g.x;
    let n0 = oct_decode(g.yz);
    let n = select(-n0, n0, dot(rd, n0) < 0.0);
    var e = indirect(p, n);
    if (is_nan(e.x) || is_nan(e.y) || is_nan(e.z) || any(e != e)) { e = vec3<f32>(0.0); }
    e = clamp_lum(e, 10.0);
    o = vec4<f32>(e, 1.0);
  }
  textureStore(gi_out, vec2<i32>(gid.xy), o);
}
