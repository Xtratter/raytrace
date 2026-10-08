//! Quality presets (same table as app/.../Presets.kt) and parameter keys for the desktop settings file.
use crate::params::id::*;

pub const ORDER: [usize; 15] = [SCALE_IDX, ADAPTIVE, TARGET_FPS, BOUNCES, SPP, TEMPORAL, STRENGTH, DENOISE, SHARPEN, CHECKER, SHADOWS, GI, CAUSTICS, REFLECTIONS, GI_RES];

/// Performance, Balanced (= params.json defaults), Quality. Columns follow [`ORDER`].
pub const TABLE: [[f32; 15]; 3] = [
    [0., 1., 60., 4., 1., 1., 4., 2., 2., 0., 1., 0., 0., 1., 2.],
    [1., 1., 60., 4., 1., 1., 3., 1., 1., 0., 1., 1., 1., 1., 1.],
    [2., 0., 60., 9., 2., 1., 2., 1., 1., 0., 1., 1., 1., 1., 0.],
];

/// Preset index (0..2) if the given values match a row exactly, else 3 ("Custom").
pub fn detect(get: impl Fn(usize) -> f32) -> usize {
    TABLE.iter().position(|row| ORDER.iter().zip(row).all(|(&i, &v)| get(i) == v)).unwrap_or(3)
}

/// Keys from params.json, indexed by param id (used by the desktop settings file).
pub const KEYS: [&str; crate::params::N] = [
    "mode", "scale_idx", "adaptive", "target_fps", "bounces", "spp", "temporal", "strength", "denoise", "sharpen", "checker",
    "shadows", "gi", "caustics", "reflections", "light", "col_a", "col_b", "anim", "orbit", "fov", "exposure", "tonemap",
    "sky", "hud", "frame_limit", "cam_mode", "sticks", "move_speed", "look_speed", "gi_res",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Store, DEFS};

    #[test]
    fn balanced_equals_defaults() {
        let s = Store::new();
        assert_eq!(detect(|i| s.get(i)), 1);
    }

    #[test]
    fn rows_within_bounds() {
        for row in TABLE { for (&i, v) in ORDER.iter().zip(row) { assert!(v >= DEFS[i].min && v <= DEFS[i].max, "{i}"); } }
    }

    #[test]
    fn keys_match_params_json() {
        let j: serde_json::Value = serde_json::from_str(include_str!("../../params.json")).unwrap();
        for e in j.as_array().unwrap() {
            let id = e["id"].as_u64().unwrap() as usize;
            if id < KEYS.len() { assert_eq!(e["key"].as_str().unwrap(), KEYS[id]); }
        }
    }
}
