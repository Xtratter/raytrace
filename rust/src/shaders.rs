//! WGSL sources: every pass is `common.wgsl` + its own body. Ungated so the host
//! (CI) can validate all shaders with naga without a GPU.

const COMMON: &str = include_str!("shaders/common.wgsl");
const SCENE: &str = include_str!("shaders/scene.wgsl");

fn join(body: &str) -> String { format!("{}\n{}", COMMON, body) }
fn join_scene(sc: &crate::scene::SceneData, body: &str) -> String { format!("{}\n{}\n{}\n{}", COMMON, SCENE, crate::scenegen::scene_wgsl(sc), body) }

pub fn trace(sc: &crate::scene::SceneData) -> String { join_scene(sc, include_str!("shaders/trace.wgsl")) }
pub fn temporal() -> String { join(include_str!("shaders/temporal.wgsl")) }
pub fn atrous() -> String { join(include_str!("shaders/atrous.wgsl")) }
pub fn gi_trace(sc: &crate::scene::SceneData) -> String { join_scene(sc, include_str!("shaders/gi_trace.wgsl")) }
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
        for i in 0..crate::scene::BUILTIN.len() {
            let sc = crate::scene::builtin(i);
            validate(&format!("trace/{}", sc.name), trace(&sc));
            validate(&format!("gi_trace/{}", sc.name), gi_trace(&sc));
        }
        validate("temporal", temporal());
        validate("atrous", atrous());
        validate("present", present());
        validate("gi_temporal", gi_temporal());
        validate("gi_atrous", gi_atrous());
        validate("composite", composite());
    }
}
