//! Scene description: JSON -> `SceneData`; scenegen.rs turns it into shader code. Built-in scenes are JSON files too (rust/scenes/).
//! Format reference: docs/scenes.md.
use serde_json::Value;

use crate::camera::SceneCam;

/// Limits keep the generated shader small enough for mobile GPU compilers.
pub const MAX_OBJECTS: usize = 32;
pub const MAX_MATS: usize = 24;
pub const MAX_COMPLEX: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Sphere { r: f32 },
    Box { half: [f32; 3], round: f32 },
    Torus { big: f32, small: f32 },
    Menger { s: f32 },
    Cylinder { r: f32, h: f32 },
    Blobs { pos2: [f32; 3], r1: f32, r2: f32, k: f32 },
}

impl Shape {
    pub fn complex(&self) -> bool { !matches!(self, Shape::Sphere { .. } | Shape::Box { .. }) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Obj {
    pub pos: [f32; 3],
    pub shape: Shape,
    pub spin: f32,
    pub mat: usize,
    /// glass box: tints the light passing through (shadow rays) instead of blocking it
    pub filter: bool,
    /// radius of a sphere around `pos` that contains the shape
    pub bound: f32,
}

/// kind: 0 diffuse, 1 metal, 2 glass (colour = tint), 3 emissive, 4 glossy (diffuse under a specular coat).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub color: [f32; 3],
    pub kind: u8,
    pub rough: f32,
    pub emit: [f32; 3],
    /// (second colour, tile size)
    pub checker: Option<([f32; 3], f32)>,
    /// emissive: 0 fixed colour, 1 / 2 = the "Light A / B colour" setting
    pub slot: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sun { pub dir: [f32; 3], /** radiance */ pub col: [f32; 3], pub cos: f32 }

#[derive(Debug, Clone, PartialEq)]
pub struct Lamp { pub pos: [f32; 3], pub r: f32, pub col: [f32; 3], pub slot: u8 }

#[derive(Debug)]
pub struct SceneData {
    pub name: String,
    pub cam: SceneCam,
    pub sun: Option<Sun>,
    pub lamps: Vec<Lamp>,
    pub mats: Vec<Material>,
    pub objs: Vec<Obj>,
    /// first glass sphere: gets the analytic caustic
    pub caustic: Option<([f32; 3], f32)>,
}

/// Scene used until a settings push selects another (matches the `scene` default in params.rs).
pub const DEFAULT_SCENE: usize = 1;

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

fn user_slot(o: &Value) -> Result<u8, String> {
    match o.get("user").and_then(|v| v.as_str()) {
        None => Ok(0),
        Some("a") => Ok(1),
        Some("b") => Ok(2),
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

    // sun
    let mut sun = None;
    if let Some(sj) = root.get("sun") {
        if !sj.is_null() {
            let d = norm(vec3(sj.get("dir").ok_or("sun: missing dir")?, "sun.dir")?)?;
            let col = opt3(sj, "color", [1.0, 0.95, 0.85])?;
            let ang = num(sj, "angle_deg", 1.5)?.clamp(0.05, 20.0);
            let cos = ang.to_radians().cos();
            // `color` = radiance of a white diffuse surface facing the sun; the shader needs the sun's own radiance
            let k = std::f32::consts::PI / (2.0 * std::f32::consts::PI * (1.0 - cos));
            sun = Some(Sun { dir: d, col: [col[0] * k, col[1] * k, col[2] * k], cos });
        }
    }

    // lamps
    let mut lamps = vec![];
    if let Some(ls) = root.get("lamps") {
        let ls = ls.as_array().ok_or("lamps: expected a list")?;
        if ls.len() > 2 { return Err("lamps: at most 2".into()); }
        for l in ls {
            let pos = vec3(l.get("pos").ok_or("lamp: missing pos")?, "lamp.pos")?;
            let r = num(l, "radius", 0.5)?;
            if r <= 0.0 { return Err("lamp: radius must be positive".into()); }
            lamps.push(Lamp { pos, r, col: opt3(l, "color", [30.0, 30.0, 30.0])?, slot: user_slot(l)? });
        }
    }

    // materials
    let mj = root.get("materials").and_then(|v| v.as_array()).ok_or("materials: expected a list")?;
    if mj.is_empty() || mj.len() > MAX_MATS { return Err(format!("materials: 1..{MAX_MATS} required")); }
    let mut names: Vec<String> = vec![];
    let mut mats = vec![];
    for m in mj {
        let n = m.get("name").and_then(|v| v.as_str()).ok_or("material: missing name")?;
        if names.iter().any(|x| x == n) { return Err(format!("material \"{n}\" defined twice")); }
        names.push(n.to_string());
        let kind = match m.get("kind").and_then(|v| v.as_str()).unwrap_or("diffuse") {
            "diffuse" => 0, "metal" => 1, "glass" => 2, "emissive" => 3, "glossy" => 4,
            k => return Err(format!("material \"{n}\": unknown kind \"{k}\"")),
        };
        let checker = if m.get("checker").is_some() { Some((opt3(m, "checker", [0.0; 3])?, num(m, "checker_scale", 1.0)?.max(0.01))) } else { None };
        mats.push(Material {
            color: opt3(m, "color", [0.8, 0.8, 0.8])?, kind, rough: num(m, "rough", 0.0)?.clamp(0.0, 1.0),
            emit: opt3(m, "emit", [0.0; 3])?, checker, slot: user_slot(m)?,
        });
    }

    // objects
    let objs_j = root.get("objects").and_then(|v| v.as_array()).ok_or("objects: expected a list")?;
    if objs_j.is_empty() { return Err("objects: at least one required".into()); }
    if objs_j.len() > MAX_OBJECTS { return Err(format!("objects: at most {MAX_OBJECTS}")); }
    let mut objs = vec![];
    let mut caustic = None;
    for (i, o) in objs_j.iter().enumerate() {
        let ty = o.get("type").and_then(|v| v.as_str()).ok_or("object: missing type")?;
        let mname = o.get("material").and_then(|v| v.as_str()).ok_or_else(|| format!("object {i}: missing material"))?;
        let mi = names.iter().position(|x| x == mname).ok_or_else(|| format!("object {i}: unknown material \"{mname}\""))?;
        let pos = vec3(o.get("pos").ok_or_else(|| format!("object {i}: missing pos"))?, "pos")?;
        let spin = num(o, "spin", 0.0)?;
        let pair = |key: &str| -> Result<(f32, f32), String> {
            let r = o.get(key).and_then(|v| v.as_array()).filter(|a| a.len() == 2).ok_or_else(|| format!("object {i}: {ty} needs {key} [a, b]"))?;
            let (a, b) = (r[0].as_f64().unwrap_or(0.0) as f32, r[1].as_f64().unwrap_or(0.0) as f32);
            if a <= 0.0 || b <= 0.0 { return Err(format!("object {i}: {key} must be positive")); }
            Ok((a, b))
        };
        let (shape, bound) = match ty {
            "sphere" => {
                let r = num(o, "radius", 1.0)?;
                if r <= 0.0 { return Err(format!("object {i}: radius must be positive")); }
                (Shape::Sphere { r }, r)
            }
            "box" => {
                let h = vec3(o.get("size").ok_or_else(|| format!("object {i}: box needs size [half-x, half-y, half-z]"))?, "size")?;
                if h.iter().any(|v| *v <= 0.0) { return Err(format!("object {i}: size must be positive")); }
                let round = num(o, "round", 0.0)?.clamp(0.0, h[0].min(h[1]).min(h[2]));
                (Shape::Box { half: h, round }, (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt())
            }
            "torus" => { let (a, b) = pair("radii")?; (Shape::Torus { big: a, small: b }, a + b + 0.05) }
            "menger" => {
                let s = num(o, "size", 1.0)?;
                if s <= 0.0 { return Err(format!("object {i}: size must be positive")); }
                (Shape::Menger { s }, s * 1.8)
            }
            "cylinder" => {
                let r = num(o, "radius", 0.5)?;
                let h = num(o, "half_height", 0.5)?;
                if r <= 0.0 || h <= 0.0 { return Err(format!("object {i}: radius and half_height must be positive")); }
                (Shape::Cylinder { r, h }, (r * r + h * h).sqrt() + 0.02)
            }
            "blobs" => {
                let pos2 = vec3(o.get("pos2").ok_or_else(|| format!("object {i}: blobs need pos2"))?, "pos2")?;
                let (a, b) = pair("radii")?;
                let k = num(o, "smooth", 0.35)?.max(0.01);
                let d = ((pos2[0] - pos[0]).powi(2) + (pos2[1] - pos[1]).powi(2) + (pos2[2] - pos[2]).powi(2)).sqrt();
                (Shape::Blobs { pos2, r1: a, r2: b, k }, d + a.max(b) + k)
            }
            t => return Err(format!("object {i}: unknown type \"{t}\"")),
        };
        // glass boxes are tinted-transparent for shadow rays; glass spheres block them (their light comes from the caustic term)
        let glass = mats[mi].kind == 2;
        let filter = glass && matches!(shape, Shape::Box { .. });
        if glass && caustic.is_none() { if let Shape::Sphere { r } = shape { caustic = Some((pos, r)); } }
        objs.push(Obj { pos, shape, spin, mat: mi, filter, bound });
    }
    if objs.iter().filter(|o| o.shape.complex()).count() > MAX_COMPLEX {
        return Err(format!("objects: at most {MAX_COMPLEX} of torus, menger, cylinder and blobs"));
    }
    Ok(SceneData { name, cam, sun, lamps, mats, objs, caustic })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        for (n, j) in BUILTIN {
            let s = parse(j).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert!(!s.objs.is_empty(), "{n}");
        }
    }

    #[test]
    fn classic_matches_old_hardcoded_scene() {
        let s = builtin(0);
        assert_eq!(s.cam, SceneCam::default());
        assert_eq!((s.lamps[0].pos, s.lamps[0].r), ([-2.5, 5.0, 1.5], 0.8));
        assert_eq!((s.lamps[1].pos, s.lamps[1].r), ([4.0, 3.0, -3.5], 0.45));
        assert_eq!(s.caustic, Some(([1.2, 0.8, 1.0], 0.8)));
        assert!(s.sun.is_none());
    }

    #[test]
    fn sun_room_has_sun_and_tinted_glass() {
        let s = builtin(1);
        let sun = s.sun.as_ref().unwrap();
        let l = sun.dir.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((l - 1.0).abs() < 1e-4);
        assert!(s.objs.iter().any(|o| o.filter));
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
        let one = |t: &str| format!(r#"{{"type":"{t}","pos":[0,0,0],"size":{},"material":"a"}}"#, if t == "menger" { "1" } else { "[1,1,1]" });
        let mk = |n: usize, t: &str| format!(r#"{{"materials":[{{"name":"a"}}],"objects":[{}]}}"#, (0..n).map(|_| one(t)).collect::<Vec<_>>().join(","));
        assert!(parse(&mk(MAX_OBJECTS + 1, "sphere")).unwrap_err().contains("at most"));
        assert!(parse(&mk(MAX_COMPLEX + 1, "menger")).unwrap_err().contains("at most"));
        assert!(parse(&mk(MAX_COMPLEX, "menger")).is_ok());
    }
}
