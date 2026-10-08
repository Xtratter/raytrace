// Realtime raytracer, SDF scene.
// mode 0: hybrid (analytic glass caustics, 1-bounce GI)
// mode 1: path tracing (true caustics via BSDF sampling)
// Params and helpers come from common.wgsl (prepended at load).

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var out_rad: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var out_g: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var out_alb: texture_storage_2d<rgba16float, write>;
var<private> g_depth: f32;
var<private> g_n: vec3<f32>;
var<private> g_id: f32;
var<private> g_skip: bool; // checkerboard: pixel only fills the g-buffer
var<private> g_alb: vec4<f32>; // rgb albedo of the primary diffuse hit, a = 1 when its GI is deferred to the GI pass

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
    let h = march_n(ro, rd, sgn, 60.0, select(MAX_STEPS, 48, b > 0));
    if (h.z < 0.5) {
      if (b == 0) { g_depth = 60.0; g_n = -rd; g_id = -1.0; }
      col += thr * sky(rd);
      break;
    }
    let p = ro + rd * h.x;
    var m = material(p, h.y);
    if (b == 0) { g_depth = h.x; g_id = h.y; g_n = -rd; }
    if (!has(F_REFLECT) && m.kind != 0u && m.kind != 3u) {
      if (m.kind == 2u) { m.albedo = vec3<f32>(0.7, 0.8, 0.9); }
      m.kind = 0u;
      m.rough = 0.0;
    }

    if (b == 0 && !pt && m.kind == 0u && has(F_GI) && has(F_GI_SPLIT)) {
      g_alb = vec4<f32>(m.albedo, 1.0);
    }

    if (m.kind == 3u) {
      if (spec) {
        col += thr * m.emit;
      }
      break;
    }

    let n = calc_normal(p);
    if (b == 0) { g_n = n; }
    if (m.kind == 4u) {
      // glossy: Fresnel-weighted choice between a mirror-like coat and the diffuse base (probabilities carry the weights)
      let fr = schlick(clamp(-dot(rd, n), 0.0, 1.0), 0.05);
      if (has(F_REFLECT) && rnd() < fr) {
        m.kind = 1u;
        m.albedo = vec3<f32>(1.0);
        m.rough = max(m.rough, 0.02);
      } else {
        m.kind = 0u;
      }
    }
    if (g_skip) { break; }
    let front = dot(rd, n) < 0.0;
    let nf = select(-n, n, front);

    if (m.kind == 0u) {
      let dl = direct_light(p, nf, h.y); // one call site: the GPU compiler inlines every call
      if (!pt) {
        var lo = dl;
        if (has(F_CAUSTICS)) { lo += caustic(p, nf); }
        if (has(F_GI) && g_alb.w < 0.5) { lo += indirect(p, nf); }
        col += thr * m.albedo * lo;
        break;
      }
      col += thr * m.albedo * dl;
      if (!has(F_GI)) { break; }
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
      if (diff_seen && !has(F_CAUSTICS)) { break; }
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
        thr = thr * sqrt(m.albedo);
        inside = front;
      }
      spec = true;
    }
  }
  return col;
}


fn shade_pixel(pix: vec2<f32>) -> vec3<f32> {
  let rd = cam_ray(pix + vec2<f32>(0.5) + P.jitter, P.res, P.cam_pos, P.cam_target, P.fov);
  var sum = vec3<f32>(0.0);
  let n = select(max(P.spp, 1u), 1u, g_skip);
  for (var s = 0u; s < n; s = s + 1u) {
    var c = trace(P.cam_pos, rd);
    if (is_nan(c.x) || is_nan(c.y) || is_nan(c.z) || any(c != c)) { c = vec3<f32>(0.0); }
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
  let skip = has(F_CHECKER) && (((gid.x + gid.y + P.frame) & 1u) == 1u);
  g_skip = skip;
  g_alb = vec4<f32>(0.0);
  col = shade_pixel(pix); // for skipped pixels trace() stops after the primary hit (g-buffer only)
  if (skip) { col = vec3<f32>(0.0); traced = 0.0; }
  textureStore(out_rad, ip, vec4<f32>(col, traced));
  textureStore(out_g, ip, vec4<f32>(g_depth, oct_encode(g_n), g_id));
  textureStore(out_alb, ip, g_alb);
}
