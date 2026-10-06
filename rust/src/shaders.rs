//! WGSL sources: every pass is `common.wgsl` + its own body. Ungated so the host
//! (CI) can validate all shaders with naga without a GPU.

const COMMON: &str = include_str!("shaders/common.wgsl");
const SCENE: &str = include_str!("shaders/scene.wgsl");

fn join(body: &str) -> String { format!("{}\n{}", COMMON, body) }
fn join_scene(body: &str) -> String { format!("{}\n{}\n{}", COMMON, SCENE, body) }

pub fn trace() -> String { join_scene(include_str!("shaders/trace.wgsl")) }
pub fn temporal() -> String { join(include_str!("shaders/temporal.wgsl")) }
pub fn atrous() -> String { join(include_str!("shaders/atrous.wgsl")) }
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
    }
}
