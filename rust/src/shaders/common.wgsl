struct Params {
  res: vec2<f32>, out_size: vec2<f32>,
  cam_pos: vec3<f32>, time: f32,
  cam_target: vec3<f32>, fov: f32,
  prev_pos: vec3<f32>, frame: u32,
  prev_target: vec3<f32>, seed: u32,
  jitter: vec2<f32>, prev_jitter: vec2<f32>,
  mode: u32, flags: u32, bounces: u32, spp: u32,
  exposure: f32, tonemap: u32, sharpen: f32, hist_floor: f32,
  sky_kind: u32, history_reset: u32, pad0: u32, pad1: u32,
  col_a: vec3<f32>, pad2: f32,
  col_b: vec3<f32>, pad3: f32,
  gi_block: u32, gi_floor: f32, pad4: u32, pad5: u32,
};

const F_SRGB: u32 = 1u;
const F_SHADOWS: u32 = 2u;
const F_GI: u32 = 4u;
const F_CAUSTICS: u32 = 8u;
const F_REFLECT: u32 = 16u;
const F_CHECKER: u32 = 32u;
const F_TEMPORAL: u32 = 64u;
const F_STILL: u32 = 128u;
const F_MOVED: u32 = 256u;
const F_GI_SPLIT: u32 = 512u;

// Reads the global uniform `P` (passing the uniform struct by value mis-evaluated flags on the target GPU driver).
fn has(f: u32) -> bool { return (P.flags & f) != 0u; }

fn cam_scale(res: vec2<f32>, fov: f32) -> vec2<f32> {
  let th = tan(fov * 0.5);
  let aspect = res.x / res.y;
  if (aspect > 1.0) { return vec2<f32>(th * aspect, th); }
  return vec2<f32>(th, th / aspect);
}

// pix: pixel coordinates (centre = +0.5, jitter already added)
fn cam_ray(pix: vec2<f32>, res: vec2<f32>, pos: vec3<f32>, tgt: vec3<f32>, fov: f32) -> vec3<f32> {
  let uv = pix / res;
  let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
  let s = cam_scale(res, fov);
  let fwd = normalize(tgt - pos);
  let right = normalize(cross(fwd, vec3<f32>(0.0, 1.0, 0.0)));
  let up = cross(right, fwd);
  return normalize(fwd + right * (ndc.x * s.x) + up * (ndc.y * s.y));
}

// inverse of cam_ray: world point -> pixel coordinates; (-1,-1) if behind the camera
fn cam_project(p: vec3<f32>, res: vec2<f32>, pos: vec3<f32>, tgt: vec3<f32>, fov: f32) -> vec2<f32> {
  let fwd = normalize(tgt - pos);
  let right = normalize(cross(fwd, vec3<f32>(0.0, 1.0, 0.0)));
  let up = cross(right, fwd);
  let v = p - pos;
  let z = dot(v, fwd);
  if (z <= 0.0001) { return vec2<f32>(-1.0, -1.0); }
  let s = cam_scale(res, fov);
  let ndc = vec2<f32>(dot(v, right) / (z * s.x), dot(v, up) / (z * s.y));
  return vec2<f32>((ndc.x * 0.5 + 0.5) * res.x, (0.5 - ndc.y * 0.5) * res.y);
}

fn oct_encode(n: vec3<f32>) -> vec2<f32> {
  let a = n / (abs(n.x) + abs(n.y) + abs(n.z));
  var e = a.xy;
  if (a.z < 0.0) { e = (1.0 - abs(a.yx)) * select(vec2<f32>(-1.0), vec2<f32>(1.0), a.xy >= vec2<f32>(0.0)); }
  return e;
}

fn oct_decode(e: vec2<f32>) -> vec3<f32> {
  var n = vec3<f32>(e.x, e.y, 1.0 - abs(e.x) - abs(e.y));
  if (n.z < 0.0) {
    let t = (1.0 - abs(n.yx)) * select(vec2<f32>(-1.0), vec2<f32>(1.0), n.xy >= vec2<f32>(0.0));
    n = vec3<f32>(t.x, t.y, n.z);
  }
  return normalize(n);
}

fn is_nan(x: f32) -> bool { return (bitcast<u32>(x) & 0x7fffffffu) > 0x7f800000u; }

fn sanitize(c: vec3<f32>) -> vec3<f32> {
  if (is_nan(c.x) || is_nan(c.y) || is_nan(c.z) || any(c != c) || any(abs(c) > vec3<f32>(1e4))) { return vec3<f32>(0.0); }
  return c;
}

fn pcg(v: u32) -> u32 {
  let s = v * 747796405u + 2891336453u;
  let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}

// Which pixel of the bs x bs block (gx, gy) a GI pass samples this frame (mirrors gi::block_offset).
fn block_offset(gx: u32, gy: u32, frame: u32, bs: u32) -> vec2<u32> {
  let n = bs * bs;
  let h = pcg(gx * 73856093u ^ gy * 19349663u);
  let k = (frame + h) % n;
  return vec2<u32>(k % bs, k / bs);
}
