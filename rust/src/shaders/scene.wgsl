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

// Shape helpers: the scene's generated map() / occ_all() call them with literal arguments, so the driver folds every constant.
fn lp(p: vec3<f32>, c: vec3<f32>, spin: f32) -> vec3<f32> {
  if (spin == 0.0) { return p - c; }
  return rot_y(p - c, P.time * spin);
}

fn d_sphere(p: vec3<f32>, c: vec3<f32>, r: f32) -> f32 { return length(p - c) - r; }

fn d_box(p: vec3<f32>, c: vec3<f32>, h: vec3<f32>, rd: f32, spin: f32) -> f32 {
  return sd_box(lp(p, c, spin), h - vec3<f32>(rd)) - rd;
}

fn d_torus(p: vec3<f32>, c: vec3<f32>, rr: vec2<f32>, spin: f32, bound: f32) -> f32 {
  let bd = length(p - c) - bound;
  if (bd > 0.2) { return bd; }
  return sd_torus(lp(p, c, spin), rr);
}

// fast: the sponge is replaced by its (unrotated) bounding box (GI and secondary rays)
fn d_menger(p: vec3<f32>, c: vec3<f32>, s: f32, spin: f32, bound: f32, fast: bool) -> f32 {
  let bd = length(p - c) - bound;
  if (bd > 0.2) { return bd; }
  if (fast) { return sd_box(p - c, vec3<f32>(s)); }
  return sd_menger(lp(p, c, spin) * (1.0 / s)) * s;
}

fn d_cyl(p: vec3<f32>, c: vec3<f32>, r: f32, h: f32, spin: f32, bound: f32) -> f32 {
  let bd = length(p - c) - bound;
  if (bd > 0.2) { return bd; }
  let q = lp(p, c, spin);
  let d = vec2<f32>(length(q.xz) - r, abs(q.y) - h);
  return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn d_pair(p: vec3<f32>, c: vec3<f32>, c2: vec3<f32>, r1: f32, r2: f32, k: f32, bound: f32) -> f32 {
  let bd = length(p - c) - bound;
  if (bd > 0.2) { return bd; }
  return smin(length(p - c) - r1, length(p - c2) - r2, k);
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
// kind: 0 diffuse, 1 metal, 2 glass (albedo = tint), 3 emissive, 4 glossy (diffuse under a specular coat)
struct Mat {
  albedo: vec3<f32>,
  emit: vec3<f32>,
  kind: u32,
  rough: f32,
};

// material(p, id) is generated per scene (id = object index).

fn has_sun() -> bool { return SUN_ON && P.sky_kind != 2u; }

// Sun radiance for the current sky setting: day full, dusk dim and orange, night off.
fn sun_rad() -> vec3<f32> {
  var k = vec3<f32>(P.lk);
  if (P.sky_kind == 1u) { k = k * vec3<f32>(0.35, 0.18, 0.08); }
  return SUN_COL * k;
}

fn sky(rd: vec3<f32>) -> vec3<f32> {
  let t = clamp(0.5 * rd.y + 0.5, 0.0, 1.0);
  var c = mix(vec3<f32>(0.55, 0.62, 0.75), vec3<f32>(0.75, 0.88, 1.15), t) * 0.45;
  if (P.sky_kind == 1u) { c = mix(vec3<f32>(0.9, 0.45, 0.3), vec3<f32>(0.25, 0.3, 0.6), t) * 0.35; }
  if (P.sky_kind == 2u) { c = mix(vec3<f32>(0.05, 0.07, 0.14), vec3<f32>(0.02, 0.03, 0.08), t) * 0.8; }
  if (has_sun() && dot(rd, SUN_DIR) > SUN_COS) { c = c + sun_rad(); }
  return c;
}

fn schlick(c: f32, f0: f32) -> f32 {
  return f0 + (1.0 - f0) * pow(1.0 - c, 5.0);
}

fn fresnel3(c: f32, f0: vec3<f32>) -> vec3<f32> {
  return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - c, 5.0);
}



// ---------------------------------------------------------------- lights
fn lamp_c(i: u32) -> vec3<f32> { return select(L1_C, L0_C, i == 0u); }
fn lamp_r(i: u32) -> f32 { return select(L1_R, L0_R, i == 0u); }

fn lamp_e(i: u32) -> vec3<f32> {
  let slot = select(L1_SLOT, L0_SLOT, i == 0u);
  if (slot == 2) { return P.col_b; }
  if (slot == 1) { return P.col_a; }
  return select(L1_COL, L0_COL, i == 0u) * P.lk;
}

fn light_count() -> u32 {
  var n = 0u;
  if (has_sun()) { n = n + 1u; }
  if (L0_R > 0.0) { n = n + 1u; }
  if (L1_R > 0.0) { n = n + 1u; }
  return n;
}

// Picks one of the enabled lights uniformly: 0 sun, 1 lamp 0, 2 lamp 1.
fn pick_light(n: u32) -> u32 {
  var k = min(u32(rnd() * f32(n)), n - 1u);
  if (has_sun()) {
    if (k == 0u) { return 0u; }
    k = k - 1u;
  }
  if (L0_R > 0.0) {
    if (k == 0u) { return 1u; }
    k = k - 1u;
  }
  return 2u;
}

// Slab test in the box's own frame (spin about y).
fn box_occ(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, c: vec3<f32>, h: vec3<f32>, spin: f32) -> bool {
  var o = ro - c;
  var d = dir;
  if (spin != 0.0) {
    let a = P.time * spin;
    o = rot_y(o, a);
    d = rot_y(dir, a);
  }
  let inv = vec3<f32>(1.0) / select(d, vec3<f32>(1e-6), abs(d) < vec3<f32>(1e-6));
  let ta = (-h - o) * inv;
  let tb = (h - o) * inv;
  let t0 = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), min(ta.z, tb.z));
  let t1 = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), max(ta.z, tb.z));
  return t1 > max(t0, 0.0) && t0 < tmax;
}

// Bounded exact marches of the non-convex shapes inside their bound spheres (torus / cylinder 16 steps, Menger sponge 24).
fn occ_menger(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, c: vec3<f32>, s: f32, spin: f32, bound: f32) -> bool {
  let b = isect_sphere(ro, dir, c, bound);
  if (b.y <= 0.0 || b.x >= tmax) { return false; }
  let te = min(b.y, tmax);
  var t = max(b.x, 0.02);
  for (var i = 0; i < MENGER_SHADOW_STEPS; i = i + 1) {
    let dd = d_menger(ro + dir * t, c, s, spin, bound, false);
    if (dd < 0.0005 * (1.0 + t)) { return true; }
    t = t + dd;
    if (t > te) { return false; }
  }
  return false;
}

fn occ_torus(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, c: vec3<f32>, rr: vec2<f32>, spin: f32, bound: f32) -> bool {
  let b = isect_sphere(ro, dir, c, bound);
  if (b.y <= 0.0 || b.x >= tmax) { return false; }
  let te = min(b.y, tmax);
  var t = max(b.x, 0.02);
  for (var i = 0; i < 16; i = i + 1) {
    let dd = d_torus(ro + dir * t, c, rr, spin, bound);
    if (dd < 0.0005 * (1.0 + t)) { return true; }
    t = t + dd;
    if (t > te) { return false; }
  }
  return false;
}

fn occ_cyl(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, c: vec3<f32>, r: f32, h: f32, spin: f32, bound: f32) -> bool {
  let b = isect_sphere(ro, dir, c, bound);
  if (b.y <= 0.0 || b.x >= tmax) { return false; }
  let te = min(b.y, tmax);
  var t = max(b.x, 0.02);
  for (var i = 0; i < 16; i = i + 1) {
    let dd = d_cyl(ro + dir * t, c, r, h, spin, bound);
    if (dd < 0.0005 * (1.0 + t)) { return true; }
    t = t + dd;
    if (t > te) { return false; }
  }
  return false;
}

// The blob pair is approximated by two slightly enlarged spheres.
fn occ_pair(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, c: vec3<f32>, c2: vec3<f32>, r1: f32, r2: f32) -> bool {
  return sph_occ(ro, dir, c, r1 + 0.02, tmax) || sph_occ(ro, dir, c2, r2 + 0.02, tmax);
}

struct LS { dir: vec3<f32>, tmax: f32, w: vec3<f32>, ok: bool };

// Picks one light and samples a direction to it; w = E/pi (multiply by albedo and by the light's visibility).
fn sample_light(p: vec3<f32>, n: vec3<f32>, ro: vec3<f32>) -> LS {
  var r: LS;
  r.ok = false;
  r.w = vec3<f32>(0.0);
  r.dir = vec3<f32>(0.0, 1.0, 0.0);
  r.tmax = 80.0;
  let cnt = light_count();
  if (cnt == 0u) { return r; }
  let li = pick_light(cnt);
  var le = sun_rad();
  var omega = 2.0 * PI * (1.0 - SUN_COS);
  if (li == 0u) {
    r.dir = sample_cone(SUN_DIR, SUN_COS);
  } else {
    let lj = li - 1u;
    let c = lamp_c(lj);
    let lr = lamp_r(lj);
    let to = c - p;
    let dist = length(to);
    let sin2 = min((lr * lr) / (dist * dist), 0.999);
    let cos_max = sqrt(1.0 - sin2);
    r.dir = sample_cone(to / dist, cos_max);
    let hs = isect_sphere(ro, r.dir, c, lr);
    if (hs.x < 0.0) { return r; }
    r.tmax = hs.x - 0.01;
    le = lamp_e(lj);
    omega = 2.0 * PI * (1.0 - cos_max);
  }
  let cos_n = dot(n, r.dir);
  if (cos_n <= 0.0) { return r; }
  r.w = le * (cos_n * omega * f32(cnt) / PI);
  r.ok = true;
  return r;
}

// Next-event estimation: returns E/pi (multiply by albedo). Glass panes tint the light; glass spheres block it
// (their light comes from caustic()).
fn direct_light(p: vec3<f32>, n: vec3<f32>, id: f32) -> vec3<f32> {
  let ro = p + n * 0.003;
  let ls = sample_light(p, n, ro);
  if (!ls.ok) { return vec3<f32>(0.0); }
  var tr = vec3<f32>(1.0);
  if (has(F_SHADOWS)) { tr = occ_all(ro, ls.dir, ls.tmax, i32(id + 0.5)); }
  return ls.w * tr;
}

// Cheap variant for GI hit points: only spheres, boxes and the blob pair can shadow.
fn direct_light_gi(p: vec3<f32>, n: vec3<f32>, id: f32) -> vec3<f32> {
  let ro = p + n * 0.003;
  let ls = sample_light(p, n, ro);
  if (!ls.ok) { return vec3<f32>(0.0); }
  var tr = vec3<f32>(1.0);
  if (has(F_SHADOWS)) { tr = occ_cheap(ro, ls.dir, ls.tmax, i32(id + 0.5)); }
  return ls.w * tr;
}

// Analytic single-sample caustic through the scene's glass sphere (lamps and sun): returns E/pi (multiply by albedo).
fn caustic(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
  let gc = CAU_C;
  let gr = CAU_R;
  if (gr <= 0.0) { return vec3<f32>(0.0); }
  let to = gc - p;
  let dist = length(to);
  if (dist < gr + 0.05) { return vec3<f32>(0.0); }
  let axis = to / dist;
  let cos_max = sqrt(max(0.0, 1.0 - (gr * gr) / (dist * dist)));
  let omega = 2.0 * PI * (1.0 - cos_max);
  let d0 = sample_cone(axis, cos_max);
  let cos_n = dot(n, d0);
  if (cos_n <= 0.0) { return vec3<f32>(0.0); }
  let h0 = isect_sphere(p, d0, gc, gr);
  if (h0.x < 0.0) { return vec3<f32>(0.0); }
  let q0 = p + d0 * h0.x;
  let n0 = normalize(q0 - gc);
  let f_in = schlick(clamp(-dot(d0, n0), 0.0, 1.0), 0.04);
  let d1 = refract(d0, n0, 1.0 / 1.5);
  let o1 = q0 - n0 * 0.002;
  let h1 = isect_sphere(o1, d1, gc, gr);
  if (h1.y < 0.0) { return vec3<f32>(0.0); }
  let q1 = o1 + d1 * h1.y;
  let n1 = normalize(q1 - gc);
  let d2 = refract(d1, -n1, 1.5);
  if (dot(d2, d2) < 0.5) { return vec3<f32>(0.0); }
  let f_out = schlick(clamp(dot(d2, n1), 0.0, 1.0), 0.04);
  let o2 = q1 + n1 * 0.002;
  var sum = vec3<f32>(0.0);
  for (var li = 0u; li < 2u; li = li + 1u) {
    if (lamp_r(li) > 0.0) {
      let hl = isect_sphere(o2, d2, lamp_c(li), lamp_r(li));
      if (hl.x > 0.0) { sum += lamp_e(li); }
    }
  }
  if (has_sun() && dot(d2, SUN_DIR) > SUN_COS) { sum += sun_rad(); }
  return sum * ((1.0 - f_in) * (1.0 - f_out) * cos_n * omega / PI);
}

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

// Sphere occludes if it is hit before tmax; an origin inside it (surface points within ~0.003 of a
// sphere touching the floor) counts as occluded, matching the old march.
fn sph_occ(ro: vec3<f32>, dir: vec3<f32>, c: vec3<f32>, r: f32, tmax: f32) -> bool {
  let s = isect_sphere(ro, dir, c, r);
  return s.y > 0.0 && s.x < tmax;
}

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
  if (m.kind == 3u) {
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

