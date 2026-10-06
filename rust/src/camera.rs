const TARGET: [f32; 3] = [0.0, 1.2, -1.8];

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub auto_phase: f32,
    pub auto_off: f32,
}

impl Camera {
    pub fn new() -> Camera {
        Camera { yaw: 0.3, pitch: 0.35, dist: 10.5, auto_phase: 0.0, auto_off: 0.0 }
    }

    pub fn reset(&mut self) {
        *self = Camera::new();
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw -= dx * 0.006;
        self.pitch = (self.pitch + dy * 0.006).clamp(0.02, 1.45);
    }

    /// `f` > 1 zooms out (matches pinch ratio prev/new distance).
    pub fn zoom(&mut self, f: f32) {
        if f.is_finite() && f > 0.0 {
            self.dist = (self.dist * f).clamp(3.0, 14.0);
        }
    }

    /// auto: 0 off (offset frozen), 1 slow, 2 fast. Swings yaw by +-0.9 rad around the manual yaw.
    pub fn update(&mut self, dt: f32, auto: u32) {
        if !dt.is_finite() || dt < 0.0 {
            return;
        }
        let w = match auto {
            1 => 0.35,
            2 => 0.9,
            _ => return,
        };
        self.auto_phase = (self.auto_phase + w * dt) % std::f32::consts::TAU;
        self.auto_off = 0.9 * self.auto_phase.sin();
    }

    pub fn pose(&self) -> ([f32; 3], [f32; 3]) {
        let yaw = self.yaw + self.auto_off;
        let p = [
            (TARGET[0] + self.dist * yaw.sin() * self.pitch.cos()).clamp(-5.5, 5.5),
            (TARGET[1] + self.dist * self.pitch.sin()).clamp(0.3, 12.0),
            (TARGET[2] + self.dist * yaw.cos() * self.pitch.cos()).clamp(-6.5, 20.0),
        ];
        (p, TARGET)
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pitch_and_dist_clamped() {
        let mut c = Camera::new();
        c.orbit(0.0, 1e6);
        assert_eq!(c.pitch, 1.45);
        c.zoom(1e6);
        assert_eq!(c.dist, 14.0);
        c.zoom(1e-6);
        assert_eq!(c.dist, 3.0);
    }
    #[test]
    fn bad_input_ignored() {
        let mut c = Camera::new();
        let (y, p) = (c.yaw, c.pitch);
        c.orbit(f32::NAN, 1.0);
        c.zoom(f32::NAN);
        c.zoom(-1.0);
        assert_eq!((c.yaw, c.pitch, c.dist), (y, p, 10.5));
    }
    #[test]
    fn pose_inside_room_and_auto_orbit() {
        let mut c = Camera::new();
        c.update(1.0, 2);
        let (p, t) = c.pose();
        assert!(p[0].abs() <= 5.5 && p[1] >= 0.3 && p[2] <= 20.0);
        assert_eq!(t, [0.0, 1.2, -1.8]);
    }
    #[test]
    fn auto_orbit_never_stuck() {
        let mut c = Camera::new();
        let mut prev = c.pose().0;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for _ in 0..600 {
            c.update(0.1, 2);
            let p = c.pose().0;
            let d = (p[0] - prev[0]).abs() + (p[1] - prev[1]).abs() + (p[2] - prev[2]).abs();
            assert!(d > 1e-4, "stuck at {:?}", p);
            lo = lo.min(p[0]);
            hi = hi.max(p[0]);
            prev = p;
        }
        assert!(hi - lo > 2.0, "spread {}", hi - lo);
    }
    #[test]
    fn auto_off_keeps_pose() {
        let mut c = Camera::new();
        c.update(1.0, 2);
        let p = c.pose();
        c.update(5.0, 0);
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn bad_dt_ignored() {
        let mut c = Camera::new();
        c.update(1.0, 2);
        let p = c.pose();
        c.update(f32::NAN, 2);
        c.update(f32::INFINITY, 2);
        c.update(-1.0, 2);
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn reset_zeroes_offset() {
        let mut c = Camera::new();
        c.update(2.0, 2);
        assert!(c.auto_off != 0.0);
        c.reset();
        assert_eq!((c.auto_phase, c.auto_off), (0.0, 0.0));
    }
}
