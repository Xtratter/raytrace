//! Turns a parsed scene into WGSL: constants, `material`, `map`, `map_gi`, `occ_all`, `occ_cheap`.
//! The numbers are written as literals (the scene is compiled into the shader): a data-driven uniform version was ~15x
//! slower on the target GPU, because the driver could no longer fold constants or drop unused shapes.
use std::fmt::Write;

use crate::scene::{Material, Obj, SceneData, Shape};

fn f(x: f32) -> String {
    let s = format!("{x:?}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") { s } else { format!("{s}.0") }
}

fn v3(v: [f32; 3]) -> String { format!("vec3<f32>({}, {}, {})", f(v[0]), f(v[1]), f(v[2])) }

fn vec2(a: f32, b: f32) -> String { format!("vec2<f32>({}, {})", f(a), f(b)) }

/// Distance of one object (`fast`: the cheap GI version).
fn dist(o: &Obj, fast: bool) -> String {
    let c = v3(o.pos);
    match &o.shape {
        Shape::Sphere { r } => format!("d_sphere(p, {c}, {})", f(*r)),
        Shape::Box { half, round } => format!("d_box(p, {c}, {}, {}, {})", v3(*half), f(*round), f(o.spin)),
        Shape::Torus { big, small } => format!("d_torus(p, {c}, {}, {}, {})", vec2(*big, *small), f(o.spin), f(o.bound)),
        Shape::Menger { s } => format!("d_menger(p, {c}, {}, {}, {}, {fast})", f(*s), f(o.spin), f(o.bound)),
        Shape::Cylinder { r, h } => format!("d_cyl(p, {c}, {}, {}, {}, {})", f(*r), f(*h), f(o.spin), f(o.bound)),
        Shape::Blobs { pos2, r1, r2, k } => format!("d_pair(p, {c}, {}, {}, {}, {}, {})", v3(*pos2), f(*r1), f(*r2), f(*k), f(o.bound)),
    }
}

/// Shadow-ray test of one object: true when it blocks (`ro`, `dir`, `tmax` are the function's parameters).
fn occ(o: &Obj) -> String {
    let c = v3(o.pos);
    match &o.shape {
        Shape::Sphere { r } => format!("sph_occ(ro, dir, {c}, {}, tmax)", f(*r)),
        Shape::Box { half, .. } => format!("box_occ(ro, dir, tmax, {c}, {}, {})", v3(*half), f(o.spin)),
        Shape::Torus { big, small } => format!("occ_torus(ro, dir, tmax, {c}, {}, {}, {})", vec2(*big, *small), f(o.spin), f(o.bound)),
        Shape::Menger { s } => format!("occ_menger(ro, dir, tmax, {c}, {}, {}, {})", f(*s), f(o.spin), f(o.bound)),
        Shape::Cylinder { r, h } => format!("occ_cyl(ro, dir, tmax, {c}, {}, {}, {}, {})", f(*r), f(*h), f(o.spin), f(o.bound)),
        Shape::Blobs { pos2, r1, r2, .. } => format!("occ_pair(ro, dir, tmax, {c}, {}, {}, {})", v3(*pos2), f(*r1), f(*r2)),
    }
}

fn material_case(m: &Material) -> String {
    let mut s = format!("m.albedo = {}; m.kind = {}u; m.rough = {};", v3(m.color), m.kind, f(m.rough));
    if let Some((c2, k)) = m.checker {
        let _ = write!(s, " if (((i32(floor(p.x * {k})) + i32(floor(p.z * {k}))) & 1) == 1) {{ m.albedo = {c2}; }}", c2 = v3(c2), k = f(k));
    }
    if m.kind == 3 {
        let e = match m.slot { 1 => "P.col_a".to_string(), 2 => "P.col_b".to_string(), _ => format!("{} * P.lk", v3(m.emit)) };
        let _ = write!(s, " m.emit = {e};");
    }
    s
}

pub fn scene_wgsl(sc: &SceneData) -> String {
    let mut s = String::new();
    match &sc.sun {
        Some(u) => {
            let _ = writeln!(s, "const SUN_ON: bool = true;\nconst SUN_DIR: vec3<f32> = {};\nconst SUN_COS: f32 = {};\nconst SUN_COL: vec3<f32> = {};", v3(u.dir), f(u.cos), v3(u.col));
        }
        None => {
            let _ = writeln!(s, "const SUN_ON: bool = false;\nconst SUN_DIR: vec3<f32> = vec3<f32>(0.0, 1.0, 0.0);\nconst SUN_COS: f32 = 1.0;\nconst SUN_COL: vec3<f32> = vec3<f32>(0.0);");
        }
    }
    for i in 0..2 {
        match sc.lamps.get(i) {
            Some(l) => { let _ = writeln!(s, "const L{i}_R: f32 = {};\nconst L{i}_C: vec3<f32> = {};\nconst L{i}_SLOT: i32 = {};\nconst L{i}_COL: vec3<f32> = {};", f(l.r), v3(l.pos), l.slot, v3(l.col)); }
            None => { let _ = writeln!(s, "const L{i}_R: f32 = 0.0;\nconst L{i}_C: vec3<f32> = vec3<f32>(0.0);\nconst L{i}_SLOT: i32 = 0;\nconst L{i}_COL: vec3<f32> = vec3<f32>(0.0);"); }
        }
    }
    match sc.caustic {
        Some((c, r)) => { let _ = writeln!(s, "const CAU_R: f32 = {};\nconst CAU_C: vec3<f32> = {};", f(r), v3(c)); }
        None => { let _ = writeln!(s, "const CAU_R: f32 = 0.0;\nconst CAU_C: vec3<f32> = vec3<f32>(0.0);"); }
    }
    for (name, fast) in [("map", false), ("map_gi", true)] {
        let _ = writeln!(s, "fn {name}(p: vec3<f32>) -> vec2<f32> {{\n  var r = vec2<f32>(1e9, -1.0);");
        for (i, o) in sc.objs.iter().enumerate() {
            let _ = writeln!(s, "  r = opU(r, {}, {i}.0);", dist(o, fast));
        }
        s.push_str("  return r;\n}\n");
    }
    for cheap in [false, true] {
        let name = if cheap { "occ_cheap" } else { "occ_all" };
        let _ = writeln!(s, "fn {name}(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, id: i32) -> vec3<f32> {{\n  var tr = vec3<f32>(1.0);");
        for (i, o) in sc.objs.iter().enumerate() {
            if cheap && o.shape.complex() { continue; }
            let blob = matches!(o.shape, Shape::Blobs { .. });
            let test = if blob { format!("id != {i} && {}", occ(o)) } else { occ(o) };
            if o.filter {
                let _ = writeln!(s, "  if ({test}) {{ tr = tr * {} * 0.92; }}", v3(sc.mats[o.mat].color));
            } else {
                let _ = writeln!(s, "  if ({test}) {{ return vec3<f32>(0.0); }}");
            }
        }
        s.push_str("  return tr;\n}\n");
    }
    s.push_str("fn material(p: vec3<f32>, id: f32) -> Mat {\n  var m: Mat;\n  m.albedo = vec3<f32>(0.8);\n  m.emit = vec3<f32>(0.0);\n  m.kind = 0u;\n  m.rough = 0.0;\n  switch (i32(id + 0.5)) {\n");
    for (i, o) in sc.objs.iter().enumerate() {
        let _ = writeln!(s, "    case {i}: {{ {} }}", material_case(&sc.mats[o.mat]));
    }
    s.push_str("    default: {}\n  }\n  return m;\n}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_are_valid_wgsl_literals() {
        assert_eq!(f(1.0), "1.0");
        assert_eq!(f(0.25), "0.25");
        assert_eq!(f(-3.0), "-3.0");
        assert!(f(1e-7).contains('e'));
        assert!(f(60.0).ends_with(".0"));
    }

    #[test]
    fn generated_code_mentions_every_object() {
        for i in 0..crate::scene::BUILTIN.len() {
            let sc = crate::scene::builtin(i);
            let g = scene_wgsl(&sc);
            assert!(g.contains(&format!("{}.0);", sc.objs.len() - 1)), "{}", sc.name);
            assert!(g.contains("fn occ_all") && g.contains("fn material"));
        }
    }
}
