// Scene description and shading helpers shared by the trace and GI passes (concatenated after common.wgsl; the including shader declares the P uniform).

const PI: f32 = 3.14159265;
const MAX_STEPS: i32 = 72;
// Step budget for primary shadow rays from Menger hits (exact map, so the sponge self-shadows
// correctly). Gained: ~most of the cost of the full 72-step exact march; lost: occluders
// farther than ~24 steps away count as unshadowed (an exhausted march = lit).
const MENGER_SHADOW_STEPS: i32 = 24;
// GI rays: one cosine-weighted bounce marched in map_gi (Menger sponge replaced by an axis-aligned
// bounding box) with GI_STEPS steps and a 10 unit range. Rays that leave the range or run out of
// steps are not sky: they contribute a neutral grey (0.3 x sky luminance), so no light leaks in
// and there are no black holes; the cost is slight darkening near far geometry. Other
// approximations: the GI hit's light uses analytic sphere occluders (mirror, glass, the two blobs;
// no torus/Menger, so those do not shadow GI hits) and its normal comes from map_gi; secondary
// (reflection/refraction) rays are capped at 48 steps; primary shadow rays use map_gi with 16
// steps, except hits on the Menger sponge (id 5), which use the exact map so the proxy box
// cannot shadow the sponge's own surface.
const GI_STEPS: i32 = 12;

const MIRROR_C: vec3<f32> = vec3<f32>(-1.8, 1.0, 0.2);
const GLASS_C: vec3<f32> = vec3<f32>(1.2, 0.8, 1.0);
const GLASS_R: f32 = 0.8;
const LA_C: vec3<f32> = vec3<f32>(-2.5, 5.0, 1.5);
const LA_R: f32 = 0.8;
const LB_C: vec3<f32> = vec3<f32>(4.0, 3.0, -3.5);
const LB_R: f32 = 0.45;

// ---------------------------------------------------------------- RNG
var<private> rng: u32;

fn rnd() -> f32 {
  rng = pcg(rng);
  return f32(rng >> 8u) * (1.0 / 16777216.0);
}

fn rand_unit() -> vec3<f32> {
  let z = 1.0 - 2.0 * rnd();
  let r = sqrt(max(0.0, 1.0 - z * z));
  let phi = 2.0 * PI * rnd();
  return vec3<f32>(r * cos(phi), r * sin(phi), z);
}

fn make_basis(n: vec3<f32>) -> mat3x3<f32> {
  let s = select(-1.0, 1.0, n.z >= 0.0);
  let a = -1.0 / (s + n.z);
  let b = n.x * n.y * a;
  let t = vec3<f32>(1.0 + s * n.x * n.x * a, s * b, -s * n.x);
  let bt = vec3<f32>(b, s + n.y * n.y * a, -n.y);
  return mat3x3<f32>(t, bt, n);
}

fn cos_hemi(n: vec3<f32>) -> vec3<f32> {
  let u1 = rnd();
  let u2 = rnd();
  let r = sqrt(u1);
  let phi = 2.0 * PI * u2;
  let v = vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(0.0, 1.0 - u1)));
  return make_basis(n) * v;
}

fn sample_cone(axis: vec3<f32>, cos_max: f32) -> vec3<f32> {
  let u1 = rnd();
  let u2 = rnd();
  let ct = 1.0 - u1 * (1.0 - cos_max);
  let st = sqrt(max(0.0, 1.0 - ct * ct));
  let phi = 2.0 * PI * u2;
  return make_basis(axis) * vec3<f32>(st * cos(phi), st * sin(phi), ct);
}

// ---------------------------------------------------------------- SDF scene
fn rot_y(p: vec3<f32>, a: f32) -> vec3<f32> {
  let c = cos(a);
  let s = sin(a);
  return vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
}

fn sd_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
  let q = abs(p) - b;
  return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sd_torus(p: vec3<f32>, t: vec2<f32>) -> f32 {
  let q = vec2<f32>(length(p.xz) - t.x, p.y);
  return length(q) - t.y;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = max(k - abs(a - b), 0.0) / k;
  return min(a, b) - h * h * k * 0.25;
}

fn sd_menger(p0: vec3<f32>) -> f32 {
  var d = sd_box(p0, vec3<f32>(1.0));
  if (d > 0.25) {
    return d;
  }
  var s = 1.0;
  for (var i = 0; i < 3; i = i + 1) {
    let a = p0 * s;
    let m = a - 2.0 * floor(a * 0.5) - vec3<f32>(1.0);
    s = s * 3.0;
    let r = abs(vec3<f32>(1.0) - 3.0 * abs(m));
    let da = max(r.x, r.y);
    let db = max(r.y, r.z);
    let dc = max(r.z, r.x);
    let c = (min(da, min(db, dc)) - 1.0) / s;
    d = max(d, c);
  }
  return d;
}

fn opU(a: vec2<f32>, d: f32, id: f32) -> vec2<f32> {
  if (d < a.x) {
    return vec2<f32>(d, id);
  }
  return a;
}

// returns (distance, material id)
fn map(p: vec3<f32>) -> vec2<f32> {
  var r = vec2<f32>(p.y, 0.0);
  r = opU(r, p.z + 7.0, 6.0);
  r = opU(r, p.x + 6.0, 7.0);
  r = opU(r, 6.0 - p.x, 8.0);
  r = opU(r, length(p - MIRROR_C) - 1.0, 1.0);
  r = opU(r, length(p - GLASS_C) - GLASS_R, 2.0);
  let b1 = length(p - vec3<f32>(-0.2, 0.5, -1.8)) - 0.5;
  let b2 = length(p - vec3<f32>(0.5, 0.42, -1.6)) - 0.42;
  r = opU(r, smin(b1, b2, 0.35), 3.0);
  let tq = p - vec3<f32>(3.1, 0.3, -0.6);
  let tb = length(tq) - 1.25;
  r = opU(r, select(sd_torus(tq, vec2<f32>(0.85, 0.3)), tb, tb > 0.15), 4.0);
  let mq0 = p - vec3<f32>(-0.4, 1.1, -4.2);
  let mb = length(mq0) - 2.05;
  if (mb > 0.2) {
    r = opU(r, mb, 5.0);
  } else {
    let q = rot_y(mq0, P.time * 0.25);
    r = opU(r, sd_menger(q * (1.0 / 1.1)) * 1.1, 5.0);
  }
  r = opU(r, length(p - LA_C) - LA_R, 9.0);
  r = opU(r, length(p - LB_C) - LB_R, 10.0);
  return r;
}

// cheaper scene for GI / secondary rays: Menger sponge replaced by its bounding box
fn map_gi(p: vec3<f32>) -> vec2<f32> {
  var r = vec2<f32>(p.y, 0.0);
  r = opU(r, p.z + 7.0, 6.0);
  r = opU(r, p.x + 6.0, 7.0);
  r = opU(r, 6.0 - p.x, 8.0);
  r = opU(r, length(p - MIRROR_C) - 1.0, 1.0);
  r = opU(r, length(p - GLASS_C) - GLASS_R, 2.0);
  let b1 = length(p - vec3<f32>(-0.2, 0.5, -1.8)) - 0.5;
  let b2 = length(p - vec3<f32>(0.5, 0.42, -1.6)) - 0.42;
  r = opU(r, smin(b1, b2, 0.35), 3.0);
  let tq = p - vec3<f32>(3.1, 0.3, -0.6);
  let tb = length(tq) - 1.25;
  r = opU(r, select(sd_torus(tq, vec2<f32>(0.85, 0.3)), tb, tb > 0.15), 4.0);
  r = opU(r, sd_box((p - vec3<f32>(-0.4, 1.1, -4.2)) * (1.0 / 1.1), vec3<f32>(1.0)) * 1.1, 5.0);
  r = opU(r, length(p - LA_C) - LA_R, 9.0);
  r = opU(r, length(p - LB_C) - LB_R, 10.0);
  return r;
}

fn calc_normal(p: vec3<f32>) -> vec3<f32> {
  let e = vec2<f32>(1.0, -1.0) * 0.0007;
  return normalize(
    e.xyy * map(p + e.xyy).x +
    e.yyx * map(p + e.yyx).x +
    e.yxy * map(p + e.yxy).x +
    e.xxx * map(p + e.xxx).x
  );
}

// sgn = -1 marches from inside a solid. returns (t, id, hit)
fn march(ro: vec3<f32>, rd: vec3<f32>, sgn: f32, tmax: f32) -> vec3<f32> {
  return march_n(ro, rd, sgn, tmax, MAX_STEPS);
}

// same, with a step budget (cheap secondary rays for GI)
fn march_n(ro: vec3<f32>, rd: vec3<f32>, sgn: f32, tmax: f32, steps: i32) -> vec3<f32> {
  var t = 0.0;
  for (var i = 0; i < steps; i = i + 1) {
    let h = map(ro + rd * t);
    let d = h.x * sgn;
    if (d < 0.0005 * (1.0 + t)) {
      return vec3<f32>(t, h.y, 1.0);
    }
    t = t + d;
    if (t > tmax) {
      break;
    }
  }
  return vec3<f32>(tmax, -1.0, 0.0);
}

// GI variant: step budget + cheaper scene
fn march_gi(ro: vec3<f32>, rd: vec3<f32>, sgn: f32, tmax: f32, steps: i32) -> vec3<f32> {
  var t = 0.0;
  for (var i = 0; i < steps; i = i + 1) {
    let h = map_gi(ro + rd * t);
    let d = h.x * sgn;
    if (d < 0.0005 * (1.0 + t)) {
      return vec3<f32>(t, h.y, 1.0);
    }
    t = t + d;
    if (t > tmax) {
      return vec3<f32>(tmax, -1.0, 0.0);
    }
  }
  return vec3<f32>(t, -1.0, 0.25); // step budget exhausted (not a clean miss)
}

fn calc_normal_gi(p: vec3<f32>) -> vec3<f32> {
  let e = vec2<f32>(1.0, -1.0) * 0.002;
  return normalize(
    e.xyy * map_gi(p + e.xyy).x +
    e.yyx * map_gi(p + e.yyx).x +
    e.yxy * map_gi(p + e.yxy).x +
    e.xxx * map_gi(p + e.xxx).x
  );
}

// ---------------------------------------------------------------- materials
struct Mat {
  albedo: vec3<f32>,
  emit: vec3<f32>,
  kind: u32,   // 0 diffuse, 1 metal, 2 glass
  rough: f32,
};

fn material(p: vec3<f32>, id: f32) -> Mat {
  var m: Mat;
  m.albedo = vec3<f32>(0.8);
  m.emit = vec3<f32>(0.0);
  m.kind = 0u;
  m.rough = 0.0;
  let i = i32(id + 0.5);
  switch (i) {
    case 0: {
      let c = (i32(floor(p.x)) + i32(floor(p.z))) & 1;
      m.albedo = select(vec3<f32>(0.78), vec3<f32>(0.12), c == 1);
    }
    case 1: {
      m.albedo = vec3<f32>(0.95, 0.95, 0.97);
      m.kind = 1u;
    }
    case 2: {
      m.albedo = vec3<f32>(1.0);
      m.kind = 2u;
    }
    case 3: {
      m.albedo = vec3<f32>(0.85, 0.12, 0.08);
    }
    case 4: {
      m.albedo = vec3<f32>(1.0, 0.78, 0.34);
      m.kind = 1u;
      m.rough = 0.22;
    }
    case 5: {
      m.albedo = vec3<f32>(0.2, 0.42, 0.9);
    }
    case 6: {
      m.albedo = vec3<f32>(0.82, 0.8, 0.72);
    }
    case 7: {
      m.albedo = vec3<f32>(0.12, 0.62, 0.2);
    }
    case 8: {
      m.albedo = vec3<f32>(0.85, 0.28, 0.1);
    }
    case 9: {
      m.emit = P.col_a;
    }
    case 10: {
      m.emit = P.col_b;
    }
    default: {
    }
  }
  return m;
}

fn sky(rd: vec3<f32>) -> vec3<f32> {
  let t = clamp(0.5 * rd.y + 0.5, 0.0, 1.0);
  if (P.sky_kind == 1u) { return mix(vec3<f32>(0.9, 0.45, 0.3), vec3<f32>(0.25, 0.3, 0.6), t) * 0.35; }
  if (P.sky_kind == 2u) { return mix(vec3<f32>(0.05, 0.07, 0.14), vec3<f32>(0.02, 0.03, 0.08), t) * 0.8; }
  return mix(vec3<f32>(0.55, 0.62, 0.75), vec3<f32>(0.75, 0.88, 1.15), t) * 0.45;
}

fn schlick(c: f32, f0: f32) -> f32 {
  return f0 + (1.0 - f0) * pow(1.0 - c, 5.0);
}

fn fresnel3(c: f32, f0: vec3<f32>) -> vec3<f32> {
  return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - c, 5.0);
}

// ---------------------------------------------------------------- lights
fn light_c(i: u32) -> vec3<f32> {
  return select(LB_C, LA_C, i == 0u);
}

fn light_r(i: u32) -> f32 {
  return select(LB_R, LA_R, i == 0u);
}

fn light_e(i: u32) -> vec3<f32> {
  return select(P.col_b, P.col_a, i == 0u);
}

// (t_near, t_far) or (-1, -1)
fn isect_sphere(ro: vec3<f32>, rd: vec3<f32>, c: vec3<f32>, r: f32) -> vec2<f32> {
  let oc = ro - c;
  let b = dot(oc, rd);
  let cc = dot(oc, oc) - r * r;
  let disc = b * b - cc;
  if (disc < 0.0) {
    return vec2<f32>(-1.0, -1.0);
  }
  let s = sqrt(disc);
  return vec2<f32>(-b - s, -b + s);
}

// Entering hit of a sphere inside (0, tmax)?
fn sph_occ(ro: vec3<f32>, dir: vec3<f32>, c: vec3<f32>, r: f32, tmax: f32) -> bool {
  let s = isect_sphere(ro, dir, c, r);
  return s.y > 0.0 && s.x < tmax && s.x > 0.0;
}

fn map_menger(p: vec3<f32>) -> f32 {
  let q = rot_y(p - vec3<f32>(-0.4, 1.1, -4.2), P.time * 0.25);
  return sd_menger(q * (1.0 / 1.1)) * 1.1;
}

// exact short march of the sponge inside its box interval
fn menger_occ(ro: vec3<f32>, dir: vec3<f32>, tmax: f32) -> bool {
  let a = P.time * 0.25;
  let o = rot_y(ro - vec3<f32>(-0.4, 1.1, -4.2), a);
  let d = rot_y(dir, a);
  let inv = vec3<f32>(1.0) / select(d, vec3<f32>(1e-6), abs(d) < vec3<f32>(1e-6));
  let ta = (vec3<f32>(-1.1) - o) * inv;
  let tb = (vec3<f32>(1.1) - o) * inv;
  let t0 = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), min(ta.z, tb.z));
  let t1 = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), max(ta.z, tb.z));
  let te = min(t1, tmax);
  if (t1 <= 0.0 || t0 >= tmax || t0 > t1) {
    return false;
  }
  var t = max(t0, 0.0);
  for (var i = 0; i < MENGER_SHADOW_STEPS; i = i + 1) {
    let dd = map_menger(ro + dir * t);
    if (dd < 0.0005 * (1.0 + t)) {
      return true;
    }
    t = t + dd;
    if (t > te) {
      return false;
    }
  }
  return false;
}

fn torus_occ(ro: vec3<f32>, dir: vec3<f32>, tmax: f32) -> bool {
  let c = vec3<f32>(3.1, 0.3, -0.6);
  let s = isect_sphere(ro, dir, c, 1.25);
  if (s.y <= 0.0 || s.x >= tmax) {
    return false;
  }
  let te = min(s.y, tmax);
  var t = max(s.x, 0.02);
  for (var i = 0; i < 16; i = i + 1) {
    let dd = sd_torus(ro + dir * t - c, vec2<f32>(0.85, 0.3));
    if (dd < 0.0005 * (1.0 + t)) {
      return true;
    }
    t = t + dd;
    if (t > te) {
      return false;
    }
  }
  return false;
}

// Analytic shadow test for the hybrid path: all real occluders are simple. Floor/walls never occlude.
fn shadow_occluded(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, id: f32) -> bool {
  if (sph_occ(ro, dir, MIRROR_C, 1.0, tmax) || sph_occ(ro, dir, GLASS_C, GLASS_R, tmax)
      || sph_occ(ro, dir, LA_C, LA_R, tmax) || sph_occ(ro, dir, LB_C, LB_R, tmax)) {
    return true;
  }
  if (abs(id - 3.0) > 0.5) {
    if (sph_occ(ro, dir, vec3<f32>(-0.2, 0.5, -1.8), 0.52, tmax)
        || sph_occ(ro, dir, vec3<f32>(0.5, 0.42, -1.6), 0.44, tmax)) {
      return true;
    }
  }
  if (abs(id - 5.0) > 0.5 && menger_occ(ro, dir, tmax)) {
    return true;
  }
  return torus_occ(ro, dir, tmax);
}

// Next-event estimation: returns E/pi (multiply by albedo). Glass is opaque for shadows,
// light passing through it is handled by caustic() (hybrid) or BSDF paths (path tracing).
fn direct_light(p: vec3<f32>, n: vec3<f32>, steps: i32, exact: bool, id: f32) -> vec3<f32> {
  let li = select(0u, 1u, rnd() < 0.5);
  let c = light_c(li);
  let r = light_r(li);
  let to = c - p;
  let dist = length(to);
  let axis = to / dist;
  let sin2 = min((r * r) / (dist * dist), 0.999);
  let cos_max = sqrt(1.0 - sin2);
  let dir = sample_cone(axis, cos_max);
  let cos_n = dot(n, dir);
  if (cos_n <= 0.0) {
    return vec3<f32>(0.0);
  }
  let omega = 2.0 * PI * (1.0 - cos_max);
  let ro = p + n * 0.003;
  let hs = isect_sphere(ro, dir, c, r);
  if (hs.x < 0.0) {
    return vec3<f32>(0.0);
  }
  if (has(F_SHADOWS)) {
    if (exact) {
      let sh = march_n(ro, dir, 1.0, hs.x - 0.01, steps);
      if (sh.z > 0.5) {
        return vec3<f32>(0.0);
      }
    } else if (shadow_occluded(ro, dir, hs.x - 0.01, id)) {
      return vec3<f32>(0.0);
    }
  }
  return light_e(li) * (cos_n * omega * 2.0 / PI);
}

// Analytic single-sample caustic through the glass sphere: returns E/pi (multiply by albedo).
fn caustic(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
  let to = GLASS_C - p;
  let dist = length(to);
  if (dist < GLASS_R + 0.05) {
    return vec3<f32>(0.0);
  }
  let axis = to / dist;
  let cos_max = sqrt(max(0.0, 1.0 - (GLASS_R * GLASS_R) / (dist * dist)));
  let omega = 2.0 * PI * (1.0 - cos_max);
  let d0 = sample_cone(axis, cos_max);
  let cos_n = dot(n, d0);
  if (cos_n <= 0.0) {
    return vec3<f32>(0.0);
  }
  let h0 = isect_sphere(p, d0, GLASS_C, GLASS_R);
  if (h0.x < 0.0) {
    return vec3<f32>(0.0);
  }
  let q0 = p + d0 * h0.x;
  let n0 = normalize(q0 - GLASS_C);
  let f_in = schlick(clamp(-dot(d0, n0), 0.0, 1.0), 0.04);
  let d1 = refract(d0, n0, 1.0 / 1.5);
  let o1 = q0 - n0 * 0.002;
  let h1 = isect_sphere(o1, d1, GLASS_C, GLASS_R);
  if (h1.y < 0.0) {
    return vec3<f32>(0.0);
  }
  let q1 = o1 + d1 * h1.y;
  let n1 = normalize(q1 - GLASS_C);
  let d2 = refract(d1, -n1, 1.5);
  if (dot(d2, d2) < 0.5) {
    return vec3<f32>(0.0);
  }
  let f_out = schlick(clamp(dot(d2, n1), 0.0, 1.0), 0.04);
  let o2 = q1 + n1 * 0.002;
  var sum = vec3<f32>(0.0);
  for (var li = 0u; li < 2u; li = li + 1u) {
    let hl = isect_sphere(o2, d2, light_c(li), light_r(li));
    if (hl.x > 0.0) {
      sum += light_e(li);
    }
  }
  return sum * ((1.0 - f_in) * (1.0 - f_out) * cos_n * omega / PI);
}

// Cheap light for a GI hit point: one random light, shadowed analytically by the big spheres
// (mirror, glass, blob cluster) instead of a march. Returns E/pi.
fn direct_light_gi(p: vec3<f32>, n: vec3<f32>, id: f32) -> vec3<f32> {
  let li = select(0u, 1u, rnd() < 0.5);
  let c = light_c(li);
  let r = light_r(li);
  let to = c - p;
  let dist = length(to);
  let axis = to / dist;
  let sin2 = min((r * r) / (dist * dist), 0.999);
  let cos_max = sqrt(1.0 - sin2);
  let dir = sample_cone(axis, cos_max);
  let cos_n = dot(n, dir);
  if (cos_n <= 0.0) {
    return vec3<f32>(0.0);
  }
  let ro = p + n * 0.003;
  let hs = isect_sphere(ro, dir, c, r);
  if (hs.x < 0.0) {
    return vec3<f32>(0.0);
  }
  if (has(F_SHADOWS)) {
    let a = isect_sphere(ro, dir, MIRROR_C, 1.0);
    let b = isect_sphere(ro, dir, GLASS_C, GLASS_R);
    // the two blob spheres; a hit on a blob (id 3) must not be occluded by the blob it sits on
    let not_blob = abs(id - 3.0) > 0.5;
    let k1 = isect_sphere(ro, dir, vec3<f32>(-0.2, 0.5, -1.8), 0.5);
    let k2 = isect_sphere(ro, dir, vec3<f32>(0.5, 0.42, -1.6), 0.42);
    let t = hs.x;
    if ((a.y > 0.0 && a.x < t) || (b.y > 0.0 && b.x < t)
        || (not_blob && ((k1.y > 0.0 && k1.x < t) || (k2.y > 0.0 && k2.x < t)))) {
      return vec3<f32>(0.0);
    }
  }
  return light_e(li) * (cos_n * (2.0 * PI * (1.0 - cos_max)) * 2.0 / PI);
}

// One-bounce diffuse GI (hybrid mode): returns incoming radiance (multiply by albedo).
fn indirect(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
  let d = cos_hemi(n);
  let ro = p + n * 0.004;
  let h = march_gi(ro, d, 1.0, 10.0, GI_STEPS);
  if (h.z < 0.5) {
    if (h.z > 0.1) { return vec3<f32>(dot(sky(d), vec3<f32>(0.1)));  }
    return sky(d);
  }
  let q = ro + d * h.x;
  let m = material(q, h.y);
  if (m.emit.x > 0.0) {
    return vec3<f32>(0.0);
  }
  if (m.kind == 2u) {
    return vec3<f32>(0.0);
  }
  var alb = m.albedo;
  if (m.kind == 1u) {
    alb = alb * 0.5;
  }
  let nq = calc_normal_gi(q);
  return alb * direct_light_gi(q, nq, h.y);
}

fn clamp_lum(c: vec3<f32>, m: f32) -> vec3<f32> {
  let l = max(c.x, max(c.y, c.z));
  if (l > m) {
    return c * (m / l);
  }
  return c;
}
