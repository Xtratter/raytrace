const TARGET: [f32; 3] = [0.0, 1.2, -1.8];

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Camera {
    pub fn new() -> Camera {
        Camera { yaw: 0.3, pitch: 0.35, dist: 10.5 }
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

    /// auto: 0 off, 1 slow, 2 fast.
    pub fn update(&mut self, dt: f32, auto: u32) {
        let w = match auto {
            1 => 0.12,
            2 => 0.35,
            _ => 0.0,
        };
        self.yaw += w * dt;
    }

    pub fn pose(&self) -> ([f32; 3], [f32; 3]) {
        let p = [
            (TARGET[0] + self.dist * self.yaw.sin() * self.pitch.cos()).clamp(-5.5, 5.5),
            (TARGET[1] + self.dist * self.pitch.sin()).clamp(0.3, 12.0),
            (TARGET[2] + self.dist * self.yaw.cos() * self.pitch.cos()).clamp(-6.5, 20.0),
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
        let y = c.yaw;
        c.update(1.0, 0);
        assert_eq!(c.yaw, y);
    }
}
