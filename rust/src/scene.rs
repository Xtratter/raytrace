//! Scene description: JSON -> `SceneData` (GPU uniform + camera setup). Built-in scenes are JSON files too (rust/scenes/).
//! Format reference: docs/scenes.md.
use bytemuck::{Pod, Zeroable};
use serde_json::Value;

use crate::camera::SceneCam;

/// Slots 0..SIMPLE_SLOTS hold spheres and boxes (cheap to evaluate); the last three are dedicated: Menger sponge, torus, blob pair
/// (at most one of each). That keeps the generated shader small: every expensive shape's code exists exactly once.
pub const SIMPLE_SLOTS: usize = 16;
pub const COMPLEX_SLOTS: usize = 3;
pub const SLOT_MENGER: usize = SIMPLE_SLOTS;
pub const SLOT_TORUS: usize = SIMPLE_SLOTS + 1;
pub const SLOT_PAIR: usize = SIMPLE_SLOTS + 2;
pub const MAX_PRIMS: usize = SIMPLE_SLOTS + COMPLEX_SLOTS;
pub const MAX_MATS: usize = 16;

pub const KIND_SPHERE: f32 = 1.0;
pub const KIND_BOX: f32 = 2.0;
pub const KIND_TORUS: f32 = 3.0;
pub const KIND_MENGER: f32 = 4.0;
pub const KIND_PAIR: f32 = 6.0;

/// p0 = (pos, kind), p1 = (size, aux), p2 = (material, spin, glass flag, bound radius), p3 = (pos2, 0). kind 0 = unused.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug, PartialEq)]
pub struct Prim { pub p0: [f32; 4], pub p1: [f32; 4], pub p2: [f32; 4], pub p3: [f32; 4] }

/// a = (colour, kind), b = (emission, roughness), c = (checker colour, checker scale), d = (user lamp colour slot, 0, 0, 0).
/// kind: 0 diffuse, 1 metal, 2 glass (colour = tint), 3 emissive, 4 glossy (diffuse with a specular coat).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug, PartialEq)]
pub struct Material { pub a: [f32; 4], pub b: [f32; 4], pub c: [f32; 4], pub d: [f32; 4] }

/// Must match `struct SceneU` in scene.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, PartialEq)]
pub struct SceneU {
    /// xyz = direction towards the sun, w = cos of its angular radius
    pub sun_dir: [f32; 4],
    /// rgb = radiance, w = 1 when the scene has a sun
    pub sun_col: [f32; 4],
    /// xyz = centre, w = radius (0 = no lamp)
    pub lamp0: [f32; 4],
    pub lamp1: [f32; 4],
    /// rgb = fixed emission, w = user colour slot (0 none, 1 = light A setting, 2 = light B setting)
    pub lamp_c0: [f32; 4],
    pub lamp_c1: [f32; 4],
    /// glass sphere for the analytic caustic: xyz, radius (0 = none)
    pub caustic: [f32; 4],
    pub prims: [Prim; MAX_PRIMS],
    pub mats: [Material; MAX_MATS],
}

const _: () = assert!(std::mem::size_of::<SceneU>() == 7 * 16 + MAX_PRIMS * 64 + MAX_MATS * 64);

#[derive(Debug)]
pub struct SceneData {
    pub name: String,
    pub u: SceneU,
    pub cam: SceneCam,
}

pub const BUILTIN: [(&str, &str); 3] = [
    ("Classic", include_str!("../scenes/classic.json")),
    ("Sun room", include_str!("../scenes/sunroom.json")),
    ("Materials", include_str!("../scenes/materials.json")),
];

pub fn builtin(i: usize) -> SceneData {
    parse(BUILTIN[i.min(BUILTIN.len() - 1)].1).expect("built-in scene is valid")
}

fn vec3(v: &Value, what: &str) -> Result<[f32; 3], String> {
    let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| format!("{what}: expected [x, y, z]"))?;
    let mut o = [0.0; 3];
    for (i, x) in a.iter().enumerate() {
        let f = x.as_f64().ok_or_else(|| format!("{what}: not a number"))? as f32;
        if !f.is_finite() { return Err(format!("{what}: not finite")); }
        o[i] = f;
    }
    Ok(o)
}

fn opt3(o: &Value, key: &str, d: [f32; 3]) -> Result<[f32; 3], String> {
    match o.get(key) { Some(v) => vec3(v, key), None => Ok(d) }
}

fn num(o: &Value, key: &str, d: f32) -> Result<f32, String> {
    match o.get(key) {
        None => Ok(d),
        Some(v) => v.as_f64().map(|f| f as f32).filter(|f| f.is_finite()).ok_or_else(|| format!("{key}: expected a number")),
    }
}

fn norm(v: [f32; 3]) -> Result<[f32; 3], String> {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-6 { return Err("zero-length direction".into()); }
    Ok([v[0] / l, v[1] / l, v[2] / l])
}

fn user_slot(o: &Value) -> Result<f32, String> {
    match o.get("user").and_then(|v| v.as_str()) {
        None => Ok(0.0),
        Some("a") => Ok(1.0),
        Some("b") => Ok(2.0),
        Some(s) => Err(format!("user: expected \"a\" or \"b\", got \"{s}\"")),
    }
}

/// Parses a scene; errors are human-readable (shown in the app).
pub fn parse(json: &str) -> Result<SceneData, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("JSON: {e}"))?;
    let name = root.get("name").and_then(|v| v.as_str()).unwrap_or("Scene").to_string();

    // camera
    let mut cam = SceneCam::default();
    if let Some(c) = root.get("camera") {
        cam.target = opt3(c, "target", cam.target)?;
        cam.yaw = num(c, "yaw", cam.yaw)?;
        cam.pitch = num(c, "pitch", cam.pitch)?.clamp(0.02, 1.45);
        cam.dist = num(c, "dist", cam.dist)?;
        if let Some(r) = c.get("dist_range") {
            let a = r.as_array().filter(|a| a.len() == 2).ok_or("dist_range: expected [min, max]")?;
            cam.dist_range = [a[0].as_f64().ok_or("dist_range: not a number")? as f32, a[1].as_f64().ok_or("dist_range: not a number")? as f32];
        }
        cam.fly_min = opt3(c, "fly_min", cam.fly_min)?;
        cam.fly_max = opt3(c, "fly_max", cam.fly_max)?;
        cam.orbit_min = opt3(c, "orbit_min", cam.fly_min)?;
        cam.orbit_max = opt3(c, "orbit_max", cam.fly_max)?;
        if cam.dist_range[0] <= 0.0 || cam.dist_range[0] > cam.dist_range[1] { return Err("dist_range: need 0 < min <= max".into()); }
        cam.dist = cam.dist.clamp(cam.dist_range[0], cam.dist_range[1]);
        for i in 0..3 {
            if cam.fly_min[i] > cam.fly_max[i] || cam.orbit_min[i] > cam.orbit_max[i] { return Err("camera bounds: min must not exceed max".into()); }
        }
    }

    let mut u = SceneU::zeroed();
    u.sun_dir = [0.0, 1.0, 0.0, 1.0];

    // sun
    if let Some(s) = root.get("sun") {
        if !s.is_null() {
            let d = norm(vec3(s.get("dir").ok_or("sun: missing dir")?, "sun.dir")?)?;
            let col = opt3(s, "color", [1.0, 0.95, 0.85])?;
            let ang = num(s, "angle_deg", 1.5)?.clamp(0.05, 20.0);
            u.sun_dir = [d[0], d[1], d[2], (ang.to_radians()).cos()];
            // `color` = radiance of a white diffuse surface facing the sun; the shader needs the sun's own radiance
            let k = std::f32::consts::PI / (2.0 * std::f32::consts::PI * (1.0 - u.sun_dir[3]));
            u.sun_col = [col[0] * k, col[1] * k, col[2] * k, 1.0];
        }
    }

    // lamps
    if let Some(ls) = root.get("lamps") {
        let ls = ls.as_array().ok_or("lamps: expected a list")?;
        if ls.len() > 2 { return Err("lamps: at most 2".into()); }
        for (i, l) in ls.iter().enumerate() {
            let p = vec3(l.get("pos").ok_or("lamp: missing pos")?, "lamp.pos")?;
            let r = num(l, "radius", 0.5)?;
            if r <= 0.0 { return Err("lamp: radius must be positive".into()); }
            let c = opt3(l, "color", [30.0, 30.0, 30.0])?;
            let slot = user_slot(l)?;
            let (lamp, lc) = if i == 0 { (&mut u.lamp0, &mut u.lamp_c0) } else { (&mut u.lamp1, &mut u.lamp_c1) };
            *lamp = [p[0], p[1], p[2], r];
            *lc = [c[0], c[1], c[2], slot];
        }
    }

    // materials
    let mats = root.get("materials").and_then(|v| v.as_array()).ok_or("materials: expected a list")?;
    if mats.is_empty() || mats.len() > MAX_MATS { return Err(format!("materials: 1..{MAX_MATS} required")); }
    let mut names: Vec<String> = vec![];
    for (i, m) in mats.iter().enumerate() {
        let n = m.get("name").and_then(|v| v.as_str()).ok_or("material: missing name")?;
        if names.iter().any(|x| x == n) { return Err(format!("material \"{n}\" defined twice")); }
        names.push(n.to_string());
        let kind = match m.get("kind").and_then(|v| v.as_str()).unwrap_or("diffuse") {
            "diffuse" => 0.0, "metal" => 1.0, "glass" => 2.0, "emissive" => 3.0, "glossy" => 4.0,
            k => return Err(format!("material \"{n}\": unknown kind \"{k}\"")),
        };
        let col = opt3(m, "color", [0.8, 0.8, 0.8])?;
        let emit = opt3(m, "emit", [0.0; 3])?;
        let rough = num(m, "rough", 0.0)?.clamp(0.0, 1.0);
        let chk = opt3(m, "checker", [0.0; 3])?;
        let cs = if m.get("checker").is_some() { num(m, "checker_scale", 1.0)?.max(0.01) } else { 0.0 };
        let slot = user_slot(m)?;
        u.mats[i] = Material { a: [col[0], col[1], col[2], kind], b: [emit[0], emit[1], emit[2], rough], c: [chk[0], chk[1], chk[2], cs], d: [slot, 0.0, 0.0, 0.0] };
    }

    // objects
    let objs = root.get("objects").and_then(|v| v.as_array()).ok_or("objects: expected a list")?;
    let is_simple = |o: &Value| matches!(o.get("type").and_then(|v| v.as_str()), Some("sphere") | Some("box"));
    let n_simple = objs.iter().filter(|o| is_simple(o)).count();
    if n_simple > SIMPLE_SLOTS { return Err(format!("objects: at most {SIMPLE_SLOTS} spheres and boxes")); }
    let mut next_simple = 0;
    let mut used = [false; COMPLEX_SLOTS];
    for (i, o) in objs.iter().enumerate() {
        let slot = if is_simple(o) { next_simple += 1; next_simple - 1 } else {
            let s = match o.get("type").and_then(|v| v.as_str()) {
                Some("menger") => SLOT_MENGER, Some("torus") => SLOT_TORUS, Some("blobs") => SLOT_PAIR,
                t => return Err(format!("object {i}: unknown type \"{}\"", t.unwrap_or("?"))),
            };
            if used[s - SIMPLE_SLOTS] { return Err(format!("object {i}: at most one menger, one torus and one blobs object")); }
            used[s - SIMPLE_SLOTS] = true;
            s
        };
        let ty = o.get("type").and_then(|v| v.as_str()).ok_or("object: missing type")?;
        let mname = o.get("material").and_then(|v| v.as_str()).ok_or_else(|| format!("object {i}: missing material"))?;
        let mi = names.iter().position(|x| x == mname).ok_or_else(|| format!("object {i}: unknown material \"{mname}\""))?;
        let pos = vec3(o.get("pos").ok_or_else(|| format!("object {i}: missing pos"))?, "pos")?;
        let spin = num(o, "spin", 0.0)?;
        let mut p = Prim::default();
        let (kind, size, aux, bound, pos2);
        match ty {
            "sphere" => {
                let r = num(o, "radius", 1.0)?;
                if r <= 0.0 { return Err(format!("object {i}: radius must be positive")); }
                kind = KIND_SPHERE; size = [r, 0.0, 0.0]; aux = 0.0; bound = r; pos2 = [0.0; 3];
            }
            "box" => {
                let s = vec3(o.get("size").ok_or_else(|| format!("object {i}: box needs size [half-x, half-y, half-z]"))?, "size")?;
                if s.iter().any(|v| *v <= 0.0) { return Err(format!("object {i}: size must be positive")); }
                kind = KIND_BOX; size = s; aux = num(o, "round", 0.0)?.clamp(0.0, s[0].min(s[1]).min(s[2])); bound = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt(); pos2 = [0.0; 3];
            }
            "torus" => {
                let r = o.get("radii").and_then(|v| v.as_array()).filter(|a| a.len() == 2).ok_or_else(|| format!("object {i}: torus needs radii [major, minor]"))?;
                let (a, b) = (r[0].as_f64().unwrap_or(0.0) as f32, r[1].as_f64().unwrap_or(0.0) as f32);
                if a <= 0.0 || b <= 0.0 { return Err(format!("object {i}: radii must be positive")); }
                kind = KIND_TORUS; size = [a, b, 0.0]; aux = 0.0; bound = a + b + 0.05; pos2 = [0.0; 3];
            }
            "menger" => {
                let s = num(o, "size", 1.0)?;
                if s <= 0.0 { return Err(format!("object {i}: size must be positive")); }
                kind = KIND_MENGER; size = [s, 0.0, 0.0]; aux = 0.0; bound = s * 1.8; pos2 = [0.0; 3];
            }
            "blobs" => {
                let p2 = vec3(o.get("pos2").ok_or_else(|| format!("object {i}: blobs need pos2"))?, "pos2")?;
                let r = o.get("radii").and_then(|v| v.as_array()).filter(|a| a.len() == 2).ok_or_else(|| format!("object {i}: blobs need radii [r1, r2]"))?;
                let (a, b) = (r[0].as_f64().unwrap_or(0.0) as f32, r[1].as_f64().unwrap_or(0.0) as f32);
                let k = num(o, "smooth", 0.35)?.max(0.01);
                if a <= 0.0 || b <= 0.0 { return Err(format!("object {i}: radii must be positive")); }
                let d = ((p2[0] - pos[0]).powi(2) + (p2[1] - pos[1]).powi(2) + (p2[2] - pos[2]).powi(2)).sqrt();
                kind = KIND_PAIR; size = [a, b, 0.0]; aux = k; bound = d + a.max(b) + k; pos2 = p2;
            }
            t => return Err(format!("object {i}: unknown type \"{t}\"")),
        }
        // glass boxes are tinted-transparent for shadow rays; glass spheres block them (their light comes from the caustic term)
        let glass = if u.mats[mi].a[3] == 2.0 && kind == KIND_BOX { 1.0 } else { 0.0 };
        p.p0 = [pos[0], pos[1], pos[2], kind];
        p.p1 = [size[0], size[1], size[2], aux];
        p.p2 = [mi as f32, spin, glass, bound];
        p.p3 = [pos2[0], pos2[1], pos2[2], 0.0];
        u.prims[slot] = p;
        if u.mats[mi].a[3] == 2.0 && kind == KIND_SPHERE && u.caustic[3] == 0.0 { u.caustic = [pos[0], pos[1], pos[2], size[0]]; }
    }
    if objs.is_empty() { return Err("objects: at least one required".into()); }
    Ok(SceneData { name, u, cam })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        for (n, j) in BUILTIN {
            let s = parse(j).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert!(s.u.prims.iter().any(|p| p.p0[3] > 0.5), "{n}");
        }
    }

    #[test]
    fn classic_matches_old_hardcoded_scene() {
        let s = builtin(0);
        assert_eq!(s.cam, SceneCam::default());
        assert_eq!(s.u.lamp0, [-2.5, 5.0, 1.5, 0.8]);
        assert_eq!(s.u.lamp1, [4.0, 3.0, -3.5, 0.45]);
        assert_eq!(s.u.caustic, [1.2, 0.8, 1.0, 0.8]);
        assert_eq!(s.u.sun_col[3], 0.0);
    }

    #[test]
    fn sun_room_has_sun_and_tinted_glass() {
        let s = builtin(1);
        assert_eq!(s.u.sun_col[3], 1.0);
        assert!(s.u.prims.iter().any(|p| p.p0[3] > 0.5 && p.p2[2] > 0.5));
        let l = (0..3).map(|i| s.u.sun_dir[i] * s.u.sun_dir[i]).sum::<f32>().sqrt();
        assert!((l - 1.0).abs() < 1e-4);
    }

    #[test]
    fn errors_are_readable() {
        let bad = |j: &str| parse(j).err().unwrap();
        assert!(bad("{").starts_with("JSON"));
        assert!(bad(r#"{"materials":[],"objects":[]}"#).contains("materials"));
        let m = r#""materials":[{"name":"a"}]"#;
        assert!(bad(&format!(r#"{{{m},"objects":[{{"type":"cone","pos":[0,0,0],"material":"a"}}]}}"#)).contains("cone"));
        assert!(bad(&format!(r#"{{{m},"objects":[{{"type":"sphere","pos":[0,0,0],"material":"zzz"}}]}}"#)).contains("zzz"));
        assert!(bad(&format!(r#"{{{m},"objects":[]}}"#)).contains("objects"));
        assert!(bad(r#"{"materials":[{"name":"a","kind":"plastic"}],"objects":[]}"#).contains("plastic"));
    }

    #[test]
    fn limits() {
        let objs: Vec<String> = (0..17).map(|_| r#"{"type":"sphere","pos":[0,0,0],"material":"a"}"#.to_string()).collect();
        let j = format!(r#"{{"materials":[{{"name":"a"}}],"objects":[{}]}}"#, objs.join(","));
        assert!(parse(&j).unwrap_err().contains("at most"));
        let objs: Vec<String> = (0..2).map(|_| r#"{"type":"menger","pos":[0,0,0],"material":"a"}"#.to_string()).collect();
        let j = format!(r#"{{"materials":[{{"name":"a"}}],"objects":[{}]}}"#, objs.join(","));
        assert!(parse(&j).unwrap_err().contains("at most"));
        // complex shapes land in the last slots whatever the order in the file
        let j = r#"{"materials":[{"name":"a"}],"objects":[{"type":"menger","pos":[0,0,0],"material":"a"},{"type":"sphere","pos":[1,0,0],"material":"a"}]}"#;
        let s = parse(j).unwrap();
        assert_eq!(s.u.prims[0].p0[3], KIND_SPHERE);
        assert_eq!(s.u.prims[SLOT_MENGER].p0[3], KIND_MENGER);
    }
}
