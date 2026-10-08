//! WGSL sources: every pass is `common.wgsl` + its own body. Ungated so the host
//! (CI) can validate all shaders with naga without a GPU.

const COMMON: &str = include_str!("shaders/common.wgsl");
const SCENE: &str = include_str!("shaders/scene.wgsl");

fn join(body: &str) -> String { format!("{}\n{}", COMMON, body) }
fn join_scene(body: &str) -> String { format!("{}\n{}\n{}\n{}", COMMON, SCENE, scene_gen(), body) }

/// WGSL that unrolls the scene's primitive and material lists with constant indices (Adreno miscompiles array reads indexed by
/// a variable, so every `S.prims[k]` / `S.mats[k]` uses a literal k). Scenes are pure data: this text is the same for all of them.
fn scene_gen() -> String {
    use crate::scene::{MAX_MATS, MAX_PRIMS, SIMPLE_SLOTS, SLOT_MENGER, SLOT_PAIR, SLOT_TORUS};
    use std::fmt::Write;
    let mut s = String::new();
    for (name, fast) in [("map", "false"), ("map_gi", "true")] {
        let _ = writeln!(s, "fn {name}(p: vec3<f32>) -> vec2<f32> {{\n  var r = vec2<f32>(1e9, -1.0);");
        for k in 0..MAX_PRIMS {
            let q = format!("S.prims[{k}]");
            let call = match k {
                _ if k < SIMPLE_SLOTS => format!("eval_simple({q}.p0, {q}.p1, {q}.p2, p)"),
                SLOT_MENGER => format!("eval_menger({q}.p0, {q}.p1, {q}.p2, p, {fast})"),
                SLOT_TORUS => format!("eval_torus({q}.p0, {q}.p1, {q}.p2, p)"),
                _ => format!("eval_pair({q}.p0, {q}.p1, {q}.p2, {q}.p3, p)"),
            };
            let _ = writeln!(s, "  if ({q}.p0.w > 0.5) {{ r = opU(r, {call}, {k}.0); }}");
        }
        s.push_str("  return r;\n}\n");
    }
    for cheap in [false, true] {
        let name = if cheap { "occ_cheap" } else { "occ_all" };
        let _ = writeln!(s, "fn {name}(ro: vec3<f32>, dir: vec3<f32>, tmax: f32, id: i32) -> vec3<f32> {{\n  var tr = vec3<f32>(1.0);");
        for k in 0..MAX_PRIMS {
            let q = format!("S.prims[{k}]");
            let call = match k {
                _ if k < SIMPLE_SLOTS => format!("occ_simple({q}.p0, {q}.p1, {q}.p2, ro, dir, tmax)"),
                SLOT_MENGER if !cheap => format!("occ_menger({q}.p0, {q}.p1, {q}.p2, ro, dir, tmax)"),
                SLOT_TORUS if !cheap => format!("occ_torus({q}.p0, {q}.p1, {q}.p2, ro, dir, tmax)"),
                SLOT_PAIR => format!("occ_pair({q}.p0, {q}.p1, {q}.p3, ro, dir, tmax, id == {k})"),
                _ => continue,
            };
            let _ = writeln!(s, "  if ({q}.p0.w > 0.5) {{\n    let o = {call};\n    if (o < 0.5) {{ return vec3<f32>(0.0); }}\n    if (o > 1.5) {{ tr = tr * mat_a(prim_mat({k})).xyz * 0.92; }}\n  }}");
        }
        s.push_str("  return tr;\n}\n");
    }
    s.push_str("fn prim_mat(id: i32) -> i32 {\n  switch (id) {\n");
    for k in 0..MAX_PRIMS {
        let _ = writeln!(s, "    case {k}: {{ return i32(S.prims[{k}].p2.x + 0.5); }}");
    }
    s.push_str("    default: { return 0; }\n  }\n}\n");
    for f in ["a", "b", "c", "d"] {
        let _ = writeln!(s, "fn mat_{f}(i: i32) -> vec4<f32> {{\n  switch (i) {{");
        for k in 0..MAX_MATS {
            let _ = writeln!(s, "    case {k}: {{ return S.mats[{k}].{f}; }}");
        }
        let _ = writeln!(s, "    default: {{ return S.mats[0].{f}; }}\n  }}\n}}");
    }
    s
}

pub fn trace() -> String { join_scene(include_str!("shaders/trace.wgsl")) }
pub fn temporal() -> String { join(include_str!("shaders/temporal.wgsl")) }
pub fn atrous() -> String { join(include_str!("shaders/atrous.wgsl")) }
pub fn gi_trace() -> String { join_scene(include_str!("shaders/gi_trace.wgsl")) }
pub fn gi_temporal() -> String { join(include_str!("shaders/gi_temporal.wgsl")) }
pub fn gi_atrous() -> String { join(include_str!("shaders/gi_atrous.wgsl")) }
pub fn composite() -> String { join(include_str!("shaders/composite.wgsl")) }
pub fn present() -> String { join(include_str!("shaders/present.wgsl")) }

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga;

    fn validate(name: &str, src: String) {
        let m = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&m).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }

    #[test]
    fn shaders_validate() {
        validate("trace", trace());
        validate("temporal", temporal());
        validate("atrous", atrous());
        validate("present", present());
        validate("gi_trace", gi_trace());
        validate("gi_temporal", gi_temporal());
        validate("gi_atrous", gi_atrous());
        validate("composite", composite());
    }
}
