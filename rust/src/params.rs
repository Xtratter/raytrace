//! Parameter table shared with Kotlin (see ../params.json). Ids are stable; do not renumber.

pub const N: usize = 31;

#[derive(Clone, Copy)]
pub struct Def {
    pub id: u32,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

const fn d(id: u32, default: f32, min: f32, max: f32) -> Def {
    Def { id, default, min, max }
}

pub const DEFS: [Def; N] = [
    d(0, 0.0, 0.0, 1.0),   // mode
    d(1, 1.0, 0.0, 4.0),   // scale_idx
    d(2, 1.0, 0.0, 1.0),   // adaptive
    d(3, 60.0, 30.0, 90.0), // target_fps
    d(4, 4.0, 1.0, 9.0),   // bounces
    d(5, 1.0, 1.0, 4.0),   // spp
    d(6, 1.0, 0.0, 1.0),   // temporal
    d(7, 3.0, 1.0, 5.0),   // strength
    d(8, 1.0, 0.0, 3.0),   // denoise
    d(9, 1.0, 0.0, 3.0),   // sharpen
    d(10, 0.0, 0.0, 1.0),  // checker
    d(11, 1.0, 0.0, 1.0),  // shadows
    d(12, 1.0, 0.0, 1.0),  // gi
    d(13, 1.0, 0.0, 1.0),  // caustics
    d(14, 1.0, 0.0, 1.0),  // reflections
    d(15, 3.0, 1.0, 5.0),  // light
    d(16, 0.0, 0.0, 5.0),  // col_a
    d(17, 0.0, 0.0, 5.0),  // col_b
    d(18, 1.0, 0.0, 1.0),  // anim
    d(19, 0.0, 0.0, 2.0),  // orbit
    d(20, 60.0, 40.0, 90.0), // fov
    d(21, 0.0, -4.0, 4.0), // exposure (half-stops)
    d(22, 0.0, 0.0, 2.0),  // tonemap
    d(23, 0.0, 0.0, 2.0),  // sky
    d(24, 1.0, 0.0, 2.0),  // hud
    d(25, 1.0, 0.0, 1.0),  // frame_limit
    d(26, 0.0, 0.0, 1.0),  // cam_mode (0 orbit, 1 fly)
    d(27, 1.0, 0.0, 1.0),  // sticks (on-screen sticks visible; Kotlin only)
    d(28, 3.0, 1.0, 5.0),  // move_speed
    d(29, 3.0, 1.0, 5.0),  // look_speed
    d(30, 1.0, 0.0, 2.0),  // gi_res
];

pub mod id {
    pub const MODE: usize = 0;
    pub const SCALE_IDX: usize = 1;
    pub const ADAPTIVE: usize = 2;
    pub const TARGET_FPS: usize = 3;
    pub const BOUNCES: usize = 4;
    pub const SPP: usize = 5;
    pub const TEMPORAL: usize = 6;
    pub const STRENGTH: usize = 7;
    pub const DENOISE: usize = 8;
    pub const SHARPEN: usize = 9;
    pub const CHECKER: usize = 10;
    pub const SHADOWS: usize = 11;
    pub const GI: usize = 12;
    pub const CAUSTICS: usize = 13;
    pub const REFLECTIONS: usize = 14;
    pub const LIGHT: usize = 15;
    pub const COL_A: usize = 16;
    pub const COL_B: usize = 17;
    pub const ANIM: usize = 18;
    pub const ORBIT: usize = 19;
    pub const FOV: usize = 20;
    pub const EXPOSURE: usize = 21;
    pub const TONEMAP: usize = 22;
    pub const SKY: usize = 23;
    pub const HUD: usize = 24;
    pub const FRAME_LIMIT: usize = 25;
    pub const CAM_MODE: usize = 26;
    pub const STICKS: usize = 27;
    pub const MOVE_SPEED: usize = 28;
    pub const LOOK_SPEED: usize = 29;
    pub const GI_RES: usize = 30;
}

/// Bits of `Params.flags` (mirrored in common.wgsl).
pub mod flags {
    pub const SRGB: u32 = 1;
    pub const SHADOWS: u32 = 2;
    pub const GI: u32 = 4;
    pub const CAUSTICS: u32 = 8;
    pub const REFLECT: u32 = 16;
    pub const CHECKER: u32 = 32;
    pub const TEMPORAL: u32 = 64;
    pub const STILL: u32 = 128; // camera + scene unchanged this frame
    pub const MOVED: u32 = 256; // camera moved this frame
    pub const GI_SPLIT: u32 = 512; // deferred half/quarter-res GI active
}

pub const SCALES: [f32; 5] = [0.25, 0.33, 0.5, 0.75, 1.0];
pub const LIGHT_K: [f32; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
pub const LIGHT_A: [[f32; 3]; 6] = [
    [45.0, 38.0, 28.0], [40.0, 40.0, 40.0], [20.0, 38.0, 50.0],
    [48.0, 22.0, 46.0], [22.0, 46.0, 26.0], [50.0, 30.0, 12.0],
];
pub const LIGHT_B: [[f32; 3]; 6] = [
    [10.0, 20.0, 45.0], [40.0, 40.0, 40.0], [45.0, 38.0, 28.0],
    [48.0, 22.0, 46.0], [22.0, 46.0, 26.0], [50.0, 30.0, 12.0],
];
/// Strength 1..5 -> minimum history weight (higher strength = smoother).
pub const HIST_FLOOR: [f32; 5] = [0.5, 0.33, 0.2, 0.12, 0.07];

/// Move/look speed level 1..5 -> multiplier.
pub const SPEED_K: [f32; 5] = [0.4, 0.7, 1.0, 1.5, 2.2];
pub fn speed_factor(level: f32) -> f32 {
    if !level.is_finite() {
        return 1.0;
    }
    SPEED_K[(level.round().clamp(1.0, 5.0) as usize) - 1]
}

/// Params that change what the image looks like, so accumulated history must be dropped.
pub fn affects_history(id: usize) -> bool {
    matches!(
        id,
        id::MODE | id::BOUNCES | id::SPP | id::SHADOWS | id::GI | id::CAUSTICS | id::REFLECTIONS
            | id::GI_RES | id::LIGHT | id::COL_A | id::COL_B | id::FOV | id::SKY
    )
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Change {
    Rejected,
    Same,
    Changed,
}

pub struct Store {
    v: [f32; N],
}

impl Store {
    pub fn new() -> Store {
        let mut v = [0.0; N];
        for (i, d) in DEFS.iter().enumerate() {
            v[i] = d.default;
        }
        Store { v }
    }

    pub fn set(&mut self, id: i32, val: f32) -> Change {
        if id < 0 || id as usize >= N || !val.is_finite() {
            log::warn!("param rejected: id={id} v={val}");
            return Change::Rejected;
        }
        let i = id as usize;
        let c = val.clamp(DEFS[i].min, DEFS[i].max);
        if c == self.v[i] {
            return Change::Same;
        }
        self.v[i] = c;
        Change::Changed
    }

    pub fn get(&self, id: usize) -> f32 {
        self.v[id]
    }

    pub fn on(&self, id: usize) -> bool {
        self.v[id] > 0.5
    }

    /// Pixels per GI block side: 0 = deferred GI off, 2 = half resolution, 4 = quarter resolution.
    pub fn gi_block(&self) -> u32 {
        match self.v[id::GI_RES] as u32 { 0 => 0, 1 => 2, _ => 4 }
    }

    /// True when GI is computed in the separate half/quarter-res pass (hybrid mode only).
    pub fn gi_split(&self) -> bool {
        self.on(id::GI) && self.gi_block() > 0 && self.v[id::MODE] < 0.5
    }

    pub fn flags(&self) -> u32 {
        let mut f = 0;
        let t = self.on(id::TEMPORAL);
        if self.on(id::SHADOWS) { f |= flags::SHADOWS; }
        if self.on(id::GI) { f |= flags::GI; }
        if self.on(id::CAUSTICS) { f |= flags::CAUSTICS; }
        if self.on(id::REFLECTIONS) { f |= flags::REFLECT; }
        if self.gi_split() { f |= flags::GI_SPLIT; }
        if t { f |= flags::TEMPORAL; }
        if t && self.on(id::CHECKER) { f |= flags::CHECKER; }
        f
    }

    pub fn light_emission(&self) -> ([f32; 3], [f32; 3]) {
        let k = LIGHT_K[self.v[id::LIGHT] as usize - 1];
        let a = LIGHT_A[self.v[id::COL_A] as usize].map(|x| x * k);
        let b = LIGHT_B[self.v[id::COL_B] as usize].map(|x| x * k);
        (a, b)
    }

    pub fn hist_floor(&self) -> f32 {
        HIST_FLOOR[self.v[id::STRENGTH] as usize - 1]
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_json() {
        let j: serde_json::Value = serde_json::from_str(include_str!("../../params.json")).unwrap();
        let mut native = 0;
        for e in j.as_array().unwrap() {
            if e["native"].as_bool().unwrap() {
                let id = e["id"].as_u64().unwrap() as usize;
                let d = &DEFS[id];
                assert_eq!(d.id as usize, id);
                assert_eq!(d.default as f64, e["default"].as_f64().unwrap(), "{}", e["key"]);
                assert_eq!(d.min as f64, e["min"].as_f64().unwrap());
                assert_eq!(d.max as f64, e["max"].as_f64().unwrap());
                native += 1;
            }
        }
        assert_eq!(native, N);
    }

    #[test]
    fn set_clamps_and_rejects() {
        let mut s = Store::new();
        assert_eq!(s.set(4, 99.0), Change::Changed);
        assert_eq!(s.get(4), 9.0);
        assert_eq!(s.set(4, 9.0), Change::Same);
        assert_eq!(s.set(999, 1.0), Change::Rejected);
        assert_eq!(s.set(-1, 1.0), Change::Rejected);
        assert_eq!(s.set(4, f32::NAN), Change::Rejected);
        assert_eq!(s.get(4), 9.0);
        assert_eq!(s.set(21, -100.0), Change::Changed);
        assert_eq!(s.get(21), -4.0);
    }

    #[test]
    fn history_ids() {
        assert!(affects_history(0)); // mode
        assert!(affects_history(13)); // caustics
        assert!(!affects_history(22)); // tonemap
        assert!(!affects_history(9)); // sharpen
    }

    #[test]
    fn flags_follow_params() {
        let mut s = Store::new();
        s.set(10, 1.0); // checker requires temporal
        s.set(6, 0.0);
        assert_eq!(s.flags() & flags::CHECKER, 0);
        s.set(6, 1.0);
        assert_ne!(s.flags() & flags::CHECKER, 0);
        assert_ne!(s.flags() & flags::TEMPORAL, 0);
    }

    #[test]
    fn speed_factor_table() {
        assert_eq!(speed_factor(1.0), 0.4);
        assert_eq!(speed_factor(3.0), 1.0);
        assert_eq!(speed_factor(5.0), 2.2);
        assert_eq!(speed_factor(99.0), 2.2);
        assert_eq!(speed_factor(-3.0), 0.4);
        assert_eq!(speed_factor(f32::NAN), 1.0);
    }

    #[test]
    fn new_params_defs_and_history() {
        assert_eq!(N, 31);
        assert_eq!(id::CAM_MODE, 26);
        assert_eq!(id::STICKS, 27);
        assert_eq!(id::MOVE_SPEED, 28);
        assert_eq!(id::LOOK_SPEED, 29);
        let s = Store::new();
        assert_eq!((s.get(26), s.get(27), s.get(28), s.get(29)), (0.0, 1.0, 3.0, 3.0));
        assert!(!affects_history(id::CAM_MODE));
    }

    #[test]
    fn light_colors_scale() {
        let s = Store::new();
        let (a, b) = s.light_emission();
        assert_eq!(a, [45.0, 38.0, 28.0]); // default light=3 -> k=1.0
        assert_eq!(b, [10.0, 20.0, 45.0]);
    }

    #[test]
    fn gi_res_param_and_flags() {
        let mut s = Store::new();
        assert_eq!(s.get(id::GI_RES), 1.0);
        assert_eq!(s.gi_block(), 2);
        assert!(s.gi_split());
        assert_ne!(s.flags() & flags::GI_SPLIT, 0);
        s.set(id::GI_RES as i32, 2.0);
        assert_eq!(s.gi_block(), 4);
        s.set(id::GI_RES as i32, 0.0);
        assert_eq!(s.gi_block(), 0);
        assert!(!s.gi_split());
        assert_eq!(s.flags() & flags::GI_SPLIT, 0);
        s.set(id::GI_RES as i32, 1.0);
        s.set(id::GI as i32, 0.0); // GI off -> no deferred GI
        assert!(!s.gi_split());
        s.set(id::GI as i32, 1.0);
        s.set(id::MODE as i32, 1.0); // path tracing keeps inline GI
        assert!(!s.gi_split());
        assert!(affects_history(id::GI_RES));
        assert_eq!(s.set(id::GI_RES as i32, 99.0), Change::Changed);
        assert_eq!(s.get(id::GI_RES), 2.0);
    }
}
