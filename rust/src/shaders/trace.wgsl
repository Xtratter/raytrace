// Realtime raytracer, SDF scene.
// mode 0: hybrid (analytic glass caustics, 1-bounce GI)
// mode 1: path tracing (true caustics via BSDF sampling)
// Params and helpers come from common.wgsl (prepended at load).

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var out_rad: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var out_g: texture_storage_2d<rgba32float, write>;
var<private> g_depth: f32;
var<private> g_n: vec3<f32>;
var<private> g_id: f32;

const PI: f32 = 3.14159265;
const MAX_STEPS: i32 = 72;

const MIRROR_C: vec3<f32> = vec3<f32>(-1.8, 1.0, 0.2);
const GLASS_C: vec3<f32> = vec3<f32>(1.2, 0.8, 1.0);
const GLASS_R: f32 = 0.8;
const LA_C: vec3<f32> = vec3<f32>(-2.5, 5.0, 1.5);
const LA_R: f32 = 0.8;
const LB_C: vec3<f32> = vec3<f32>(4.0, 3.0, -3.5);
const LB_R: f32 = 0.45;

// ---------------------------------------------------------------- RNG
var<private> rng: u32;

fn pcg(v: u32) -> u32 {
  let s = v * 747796405u + 2891336453u;
  let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}

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
  var t = 0.0;
  for (var i = 0; i < MAX_STEPS; i = i + 1) {
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

// Next-event estimation: returns E/pi (multiply by albedo). Glass is opaque for shadows,
// light passing through it is handled by caustic() (hybrid) or BSDF paths (path tracing).
fn direct_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
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
  if (has(P, F_SHADOWS)) {
    let sh = march(ro, dir, 1.0, hs.x - 0.01);
    if (sh.z > 0.5) {
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

// One-bounce diffuse GI (hybrid mode): returns incoming radiance (multiply by albedo).
fn indirect(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
  let d = cos_hemi(n);
  let ro = p + n * 0.004;
  let h = march(ro, d, 1.0, 14.0);
  if (h.z < 0.5) {
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
  let nq = calc_normal(q);
  return alb * direct_light(q, nq);
}

// ---------------------------------------------------------------- integrator
fn trace(ro_in: vec3<f32>, rd_in: vec3<f32>) -> vec3<f32> {
  var ro = ro_in;
  var rd = rd_in;
  var thr = vec3<f32>(1.0);
  var col = vec3<f32>(0.0);
  var inside = false;
  var spec = true; // only specular vertices since camera / last diffuse vertex
  let pt = P.mode == 1u;
  let max_b = i32(P.bounces);
  var diff_seen = false;

  for (var b = 0; b < max_b; b = b + 1) {
    let sgn = select(1.0, -1.0, inside);
    let h = march(ro, rd, sgn, 60.0);
    if (h.z < 0.5) {
      if (b == 0) { g_depth = 60.0; g_n = -rd; g_id = -1.0; }
      col += thr * sky(rd);
      break;
    }
    let p = ro + rd * h.x;
    var m = material(p, h.y);
    if (b == 0) { g_depth = h.x; g_id = h.y; g_n = -rd; }
    if (!has(P, F_REFLECT) && m.kind != 0u) {
      if (m.kind == 2u) { m.albedo = vec3<f32>(0.7, 0.8, 0.9); }
      m.kind = 0u;
      m.rough = 0.0;
    }

    if (m.emit.x > 0.0) {
      if (spec) {
        col += thr * m.emit;
      }
      break;
    }

    let n = calc_normal(p);
    if (b == 0) { g_n = n; }
    let front = dot(rd, n) < 0.0;
    let nf = select(-n, n, front);

    if (m.kind == 0u) {
      if (!pt) {
        var lo = direct_light(p, nf);
        if (has(P, F_CAUSTICS)) { lo += caustic(p, nf); }
        if (has(P, F_GI)) { lo += indirect(p, nf); }
        col += thr * m.albedo * lo;
        break;
      }
      col += thr * m.albedo * direct_light(p, nf);
      if (!has(P, F_GI)) { break; }
      diff_seen = true;
      thr = thr * m.albedo;
      ro = p + nf * 0.003;
      rd = cos_hemi(nf);
      spec = false;
      if (b >= 3) {
        let q = clamp(max(thr.x, max(thr.y, thr.z)), 0.1, 0.95);
        if (rnd() > q) {
          break;
        }
        thr = thr / q;
      }
    } else if (m.kind == 1u) {
      var r = reflect(rd, nf);
      if (m.rough > 0.0) {
        r = normalize(r + m.rough * rand_unit());
        if (dot(r, nf) < 0.0) {
          r = r - 2.0 * dot(r, nf) * nf;
        }
      }
      thr = thr * fresnel3(clamp(-dot(rd, nf), 0.0, 1.0), m.albedo);
      ro = p + nf * 0.003;
      rd = r;
      spec = true;
    } else {
      if (diff_seen && !has(P, F_CAUSTICS)) { break; }
      let cosi = clamp(-dot(rd, nf), 0.0, 1.0);
      let eta = select(1.5, 1.0 / 1.5, front);
      let rr = refract(rd, nf, eta);
      var fr = schlick(cosi, 0.04);
      if (dot(rr, rr) < 0.5) {
        fr = 1.0;
      }
      if (rnd() < fr) {
        rd = reflect(rd, nf);
        ro = p + nf * 0.003;
      } else {
        rd = rr;
        ro = p - nf * 0.003;
        inside = front;
      }
      spec = true;
    }
  }
  return col;
}

fn clamp_lum(c: vec3<f32>, m: f32) -> vec3<f32> {
  let l = max(c.x, max(c.y, c.z));
  if (l > m) {
    return c * (m / l);
  }
  return c;
}


fn shade_pixel(pix: vec2<f32>) -> vec3<f32> {
  let rd = cam_ray(pix + vec2<f32>(0.5) + P.jitter, P.res, P.cam_pos, P.cam_target, P.fov);
  var sum = vec3<f32>(0.0);
  let n = max(P.spp, 1u);
  for (var s = 0u; s < n; s = s + 1u) {
    var c = trace(P.cam_pos, rd);
    if (any(c != c)) { c = vec3<f32>(0.0); }
    sum += clamp_lum(c, select(10.0, 24.0, P.mode == 1u));
  }
  return sum / f32(n);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let w = u32(P.res.x);
  let h = u32(P.res.y);
  if (gid.x >= w || gid.y >= h) { return; }
  let idx = gid.y * w + gid.x;
  rng = pcg(idx + P.seed * 747796405u);
  rng = pcg(rng ^ 2747636419u);
  let pix = vec2<f32>(f32(gid.x), f32(gid.y));
  let ip = vec2<i32>(gid.xy);

  var col = vec3<f32>(0.0);
  var traced = 1.0;
  let skip = has(P, F_CHECKER) && (((gid.x + gid.y + P.frame) & 1u) == 1u);
  if (skip) {
    // visibility only: gbuffer for reprojection, no shading
    let rd = cam_ray(pix + vec2<f32>(0.5) + P.jitter, P.res, P.cam_pos, P.cam_target, P.fov);
    let hh = march(P.cam_pos, rd, 1.0, 60.0);
    if (hh.z > 0.5) {
      let p = P.cam_pos + rd * hh.x;
      g_depth = hh.x; g_id = hh.y; g_n = calc_normal(p);
    } else {
      g_depth = 60.0; g_id = -1.0; g_n = -rd;
    }
    traced = 0.0;
  } else {
    col = shade_pixel(pix);
  }
  textureStore(out_rad, ip, vec4<f32>(col, traced));
  textureStore(out_g, ip, vec4<f32>(g_depth, oct_encode(g_n), g_id));
}
