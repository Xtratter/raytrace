const TARGET: [f32; 3] = [0.0, 1.2, -1.8];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Orbit,
    Fly,
    /// Helicopter: left stick x turns, y moves forward/back; right stick y changes height, x strafes. Pitch is fixed.
    Heli,
}

impl Mode {
    pub fn from_param(v: f32) -> Mode {
        if v > 1.5 { Mode::Heli } else if v > 0.5 { Mode::Fly } else { Mode::Orbit }
    }
}

const LOOK_RATE: f32 = 1.6; // rad/s at full deflection
const MOVE_RATE: f32 = 2.5; // units/s at full deflection
const HELI_PITCH: f32 = -0.12; // slightly down, rad
const FLY_MIN: [f32; 3] = [-5.5, 0.3, -6.5];
const FLY_MAX: [f32; 3] = [5.5, 8.0, 12.0];

/// NaN/inf -> 0, each value clamped to [-1, 1].
pub fn sanitize_sticks(s: [f32; 4]) -> [f32; 4] {
    s.map(|v| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 })
}

/// Orbit: position = TARGET + dist*(sin(yaw)cos(pitch), sin(pitch), cos(yaw)cos(pitch)).
/// Fly: forward = (sin(fly_yaw)cos(fly_pitch), sin(fly_pitch), cos(fly_yaw)cos(fly_pitch)); decreasing
/// yaw turns right, positive pitch looks up. right = (-cos(yaw), 0, sin(yaw)).
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub auto_phase: f32,
    pub auto_off: f32,
    mode: Mode,
    pub fly_pos: [f32; 3],
    pub fly_yaw: f32,
    pub fly_pitch: f32,
}

impl Camera {
    pub fn new() -> Camera {
        let mut c = Camera {
            yaw: 0.3, pitch: 0.35, dist: 10.5, auto_phase: 0.0, auto_off: 0.0,
            mode: Mode::Orbit, fly_pos: [0.0; 3], fly_yaw: 0.0, fly_pitch: 0.0,
        };
        c.enter_fly_from_orbit();
        c
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Reset the pose of the current mode (orbit: defaults; fly: default orbit pose looking at the target).
    pub fn reset(&mut self) {
        match self.mode {
            Mode::Orbit => {
                let m = self.mode;
                let (fp, fy, fpi) = (self.fly_pos, self.fly_yaw, self.fly_pitch);
                *self = Camera::new();
                self.mode = m;
                self.fly_pos = fp;
                self.fly_yaw = fy;
                self.fly_pitch = fpi;
            }
            Mode::Fly | Mode::Heli => {
                let d = Camera::new();
                self.fly_pos = d.fly_pos;
                self.fly_yaw = d.fly_yaw;
                self.fly_pitch = d.fly_pitch;
            }
        }
    }

    pub fn set_mode(&mut self, m: Mode) {
        if m == self.mode {
            return;
        }
        if self.mode == Mode::Orbit {
            self.enter_fly_from_orbit();
        }
        if m == Mode::Heli {
            self.fly_pitch = HELI_PITCH;
        }
        self.mode = m;
    }

    fn orbit_pose(&self) -> ([f32; 3], [f32; 3]) {
        let yaw = self.yaw + self.auto_off;
        let p = [
            (TARGET[0] + self.dist * yaw.sin() * self.pitch.cos()).clamp(-5.5, 5.5),
            (TARGET[1] + self.dist * self.pitch.sin()).clamp(0.3, 12.0),
            (TARGET[2] + self.dist * yaw.cos() * self.pitch.cos()).clamp(-6.5, 20.0),
        ];
        (p, TARGET)
    }

    fn enter_fly_from_orbit(&mut self) {
        let (p, t) = self.orbit_pose();
        let d = [t[0] - p[0], t[1] - p[1], t[2] - p[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
        self.fly_pos = p;
        self.fly_pitch = (d[1] / l).clamp(-1.0, 1.0).asin();
        self.fly_yaw = d[0].atan2(d[2]);
    }

    fn fly_forward(&self) -> [f32; 3] {
        let (sy, cy) = self.fly_yaw.sin_cos();
        let (sp, cp) = self.fly_pitch.sin_cos();
        [sy * cp, sp, cy * cp]
    }

    fn clamp_fly(&mut self) {
        for i in 0..3 {
            self.fly_pos[i] = self.fly_pos[i].clamp(FLY_MIN[i], FLY_MAX[i]);
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        match self.mode {
            Mode::Orbit => {
                self.yaw -= dx * 0.006;
                self.pitch = (self.pitch + dy * 0.006).clamp(0.02, 1.45);
            }
            Mode::Heli => self.fly_yaw -= dx * 0.006,
            Mode::Fly => {
                self.fly_yaw -= dx * 0.006;
                self.fly_pitch = (self.fly_pitch - dy * 0.006).clamp(-1.45, 1.45);
            }
        }
    }

    /// `f` > 1 zooms out (matches pinch ratio prev/new distance). In fly mode: moves along the look direction.
    pub fn zoom(&mut self, f: f32) {
        if !(f.is_finite() && f > 0.0) {
            return;
        }
        match self.mode {
            Mode::Orbit => self.dist = (self.dist * f).clamp(3.0, 14.0),
            Mode::Fly | Mode::Heli => {
                let fw = self.fly_forward();
                let k = (1.0 - f).clamp(-10.0, 1.0) * 2.0;
                for i in 0..3 {
                    self.fly_pos[i] += fw[i] * k;
                }
                self.clamp_fly();
            }
        }
    }

    /// Per-frame update. auto (orbit mode only): 0 off (offset frozen), 1 slow, 2 fast; swings yaw by +-0.9 rad.
    /// sticks = [lx, ly, rx, ry], +y = up. move_f / look_f are speed multipliers.
    pub fn update(&mut self, dt: f32, auto: u32, mode: Mode, sticks: [f32; 4], move_f: f32, look_f: f32) {
        self.set_mode(mode);
        if !dt.is_finite() || dt < 0.0 {
            return;
        }
        let [lx, ly, rx, ry] = sanitize_sticks(sticks);
        let move_f = if move_f.is_finite() { move_f.clamp(0.0, 10.0) } else { 1.0 };
        let look_f = if look_f.is_finite() { look_f.clamp(0.0, 10.0) } else { 1.0 };
        let lr = LOOK_RATE * look_f * dt;
        match self.mode {
            Mode::Orbit => {
                self.yaw += rx * lr; // camera moves right (as seen from the camera)
                // right stick up = camera up; left stick right = camera up at half rate
                self.pitch = (self.pitch + ry * lr + lx * 0.5 * lr).clamp(0.02, 1.45);
                self.dist = (self.dist * (-ly * dt * move_f).exp()).clamp(3.0, 14.0);
                let w = match auto {
                    1 => 0.35,
                    2 => 0.9,
                    _ => return,
                };
                self.auto_phase = (self.auto_phase + w * dt) % std::f32::consts::TAU;
                self.auto_off = 0.9 * self.auto_phase.sin();
            }
            Mode::Heli => {
                self.fly_yaw -= lx * lr;
                self.fly_pitch = HELI_PITCH;
                let (sy, cy) = self.fly_yaw.sin_cos();
                let v = MOVE_RATE * move_f * dt;
                self.fly_pos[0] += (sy * ly - cy * rx) * v;
                self.fly_pos[2] += (cy * ly + sy * rx) * v;
                self.fly_pos[1] += ry * v;
                self.clamp_fly();
            }
            Mode::Fly => {
                self.fly_yaw -= rx * lr;
                self.fly_pitch = (self.fly_pitch + ry * lr).clamp(-1.45, 1.45);
                if lx != 0.0 || ly != 0.0 {
                    let (sy, cy) = self.fly_yaw.sin_cos();
                    let v = MOVE_RATE * move_f * dt;
                    self.fly_pos[0] += (sy * ly - cy * lx) * v;
                    self.fly_pos[2] += (cy * ly + sy * lx) * v;
                    self.clamp_fly();
                }
            }
        }
    }

    pub fn pose(&self) -> ([f32; 3], [f32; 3]) {
        match self.mode {
            Mode::Orbit => self.orbit_pose(),
            Mode::Fly | Mode::Heli => {
                let f = self.fly_forward();
                let p = self.fly_pos;
                (p, [p[0] + f[0] * 3.0, p[1] + f[1] * 3.0, p[2] + f[2] * 3.0])
            }
        }
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
    const Z: [f32; 4] = [0.0; 4];
    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
    fn len(a: [f32; 3]) -> f32 { dot(a, a).sqrt() }
    fn fwd(c: &Camera) -> [f32; 3] { let (p, t) = c.pose(); let d = sub(t, p); let l = len(d); [d[0] / l, d[1] / l, d[2] / l] }
    fn fly() -> Camera { let mut c = Camera::new(); c.update(0.0, 0, Mode::Fly, Z, 1.0, 1.0); c }
    /// fly camera placed well inside the room (the default pose is outside the fly bounds)
    fn flyc() -> Camera { let mut c = fly(); c.fly_pos = [0.0, 2.0, 0.0]; c }
    fn close(a: [f32; 3], b: [f32; 3]) { assert!(len(sub(a, b)) < 1e-4, "{:?} vs {:?}", a, b); }
    fn run(c: &mut Camera, n: usize, dt: f32, m: Mode, s: [f32; 4]) { for _ in 0..n { c.update(dt, 0, m, s, 1.0, 1.0); } }
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
        c.update(1.0, 2, Mode::Orbit, Z, 1.0, 1.0);
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
            c.update(0.1, 2, Mode::Orbit, Z, 1.0, 1.0);
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
        c.update(1.0, 2, Mode::Orbit, Z, 1.0, 1.0);
        let p = c.pose();
        c.update(5.0, 0, Mode::Orbit, Z, 1.0, 1.0);
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn bad_dt_ignored() {
        let mut c = Camera::new();
        c.update(1.0, 2, Mode::Orbit, Z, 1.0, 1.0);
        let p = c.pose();
        c.update(f32::NAN, 2, Mode::Orbit, Z, 1.0, 1.0);
        c.update(f32::INFINITY, 2, Mode::Orbit, Z, 1.0, 1.0);
        c.update(-1.0, 2, Mode::Orbit, Z, 1.0, 1.0);
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn reset_zeroes_offset() {
        let mut c = Camera::new();
        c.update(2.0, 2, Mode::Orbit, Z, 1.0, 1.0);
        assert!(c.auto_off != 0.0);
        c.reset();
        assert_eq!((c.auto_phase, c.auto_off), (0.0, 0.0));
    }

    #[test]
    fn fly_forward_follows_look() {
        let mut c = flyc();
        let (p0, _) = c.pose();
        let f = fwd(&c);
        c.update(1.0, 0, Mode::Fly, [0.0, 0.5, 0.0, 0.0], 1.0, 1.0);
        let (p1, _) = c.pose();
        // horizontal only: moved 0.5*2.5 along the yaw-only direction
        let d = sub(p1, p0);
        assert!((len(d) - 1.25).abs() < 1e-3, "{}", len(d));
        assert!(d[1].abs() < 1e-6);
        let h = (f[0] * f[0] + f[2] * f[2]).sqrt();
        assert!((dot(d, [f[0] / h, 0.0, f[2] / h]) - 1.25).abs() < 1e-3);
    }
    #[test]
    fn fly_back_and_strafe_perpendicular() {
        let mut c = flyc();
        let (p0, _) = c.pose();
        let f = fwd(&c);
        c.update(0.2, 0, Mode::Fly, [1.0, 0.0, 0.0, 0.0], 1.0, 1.0);
        let d = sub(c.pose().0, p0);
        assert!((len(d) - 0.5).abs() < 1e-4);
        assert!(dot(d, f).abs() < 1e-4 && d[1].abs() < 1e-6);
        // right strafe: right = forward x up
        let r = [f[1] * 0.0 - f[2] * 1.0, 0.0, f[0] * 1.0];
        assert!(dot(d, r) > 0.0);
        let p1 = c.pose().0;
        c.update(0.2, 0, Mode::Fly, [0.0, -1.0, 0.0, 0.0], 1.0, 1.0);
        let h = (f[0] * f[0] + f[2] * f[2]).sqrt();
        assert!(dot(sub(c.pose().0, p1), [f[0] / h, 0.0, f[2] / h]) < -0.49);
    }
    #[test]
    fn move_factor_scales_speed() {
        let mut c = flyc();
        let p0 = c.pose().0;
        c.update(0.1, 0, Mode::Fly, [0.0, 1.0, 0.0, 0.0], 2.0, 1.0);
        assert!((len(sub(c.pose().0, p0)) - 0.5).abs() < 1e-4);
    }
    #[test]
    fn sticks_sanitized() {
        assert_eq!(sanitize_sticks([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.3]), [0.0, 0.0, 0.0, 0.3]);
        assert_eq!(sanitize_sticks([5.0, -5.0, 1.0, -0.5]), [1.0, -1.0, 1.0, -0.5]);
        let mut a = fly();
        let mut b = fly();
        a.update(0.1, 0, Mode::Fly, [0.0, 9.0, 0.0, 0.0], 1.0, 1.0);
        b.update(0.1, 0, Mode::Fly, [0.0, 1.0, 0.0, 0.0], 1.0, 1.0);
        assert_eq!(a.pose(), b.pose());
        let mut c = fly();
        let p = c.pose();
        c.update(0.1, 0, Mode::Fly, [f32::NAN; 4], 1.0, 1.0);
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn fly_position_clamped_in_room() {
        let mut c = fly();
        for s in [[0.0, 1.0, 0.0, 0.0], [1.0, -1.0, 0.0, 0.0], [-1.0, 1.0, 0.0, 0.0]] {
            run(&mut c, 400, 0.1, Mode::Fly, s);
            let p = c.pose().0;
            assert!(p[0].abs() <= 5.5 + 1e-5 && p[1] >= 0.3 - 1e-5 && p[1] <= 8.0 + 1e-5 && p[2] >= -6.5 - 1e-5 && p[2] <= 12.0 + 1e-5, "{:?}", p);
        }
        // touch zoom too
        for _ in 0..200 { c.zoom(0.1); }
        let p = c.pose().0;
        assert!(p[0].abs() <= 5.5 + 1e-5 && p[2] >= -6.5 - 1e-5 && p[2] <= 12.0 + 1e-5);
    }
    #[test]
    fn fly_pitch_clamped_and_target_distinct() {
        let mut c = fly();
        run(&mut c, 100, 0.1, Mode::Fly, [0.0, 0.0, 0.0, 1.0]);
        assert!((c.fly_pitch - 1.45).abs() < 1e-6);
        run(&mut c, 200, 0.1, Mode::Fly, [0.0, 0.0, 0.0, -1.0]);
        assert!((c.fly_pitch + 1.45).abs() < 1e-6);
        let (p, t) = c.pose();
        assert!((len(sub(t, p)) - 3.0).abs() < 1e-4);
    }
    #[test]
    fn fly_yaw_right_stick_turns_right() {
        let mut c = fly();
        let f0 = fwd(&c);
        c.update(0.1, 0, Mode::Fly, [0.0, 0.0, 1.0, 0.0], 1.0, 1.0);
        let f1 = fwd(&c);
        let right = [-f0[2], 0.0, f0[0]];
        assert!(dot(f1, right) > 0.1);
        assert!((c.fly_yaw - (fly().fly_yaw - 0.16)).abs() < 1e-5);
    }
    #[test]
    fn orbit_sticks_move_yaw_pitch_dist() {
        let mut c = Camera::new();
        c.update(0.1, 0, Mode::Orbit, [0.0, 0.0, 1.0, 0.0], 1.0, 1.0);
        assert!((c.yaw - (0.3 + 0.16)).abs() < 1e-5);
        let mut c = Camera::new();
        c.update(0.1, 0, Mode::Orbit, [0.0, 0.0, 0.0, 1.0], 1.0, 1.0);
        assert!((c.pitch - (0.35 + 0.16)).abs() < 1e-5);
        let mut c = Camera::new();
        c.update(0.1, 0, Mode::Orbit, [0.0, 1.0, 0.0, 0.0], 1.0, 1.0);
        assert!((c.dist - 10.5 * (-0.1f32).exp()).abs() < 1e-4);
        let mut c = Camera::new();
        c.update(0.1, 0, Mode::Orbit, [1.0, 0.0, 0.0, 0.0], 1.0, 1.0);
        assert!((c.pitch - (0.35 + 0.08)).abs() < 1e-5);
        // clamps
        run(&mut c, 500, 0.1, Mode::Orbit, [1.0, 1.0, 0.0, 0.0]);
        assert!(c.pitch <= 1.45 && c.dist >= 3.0 && c.dist <= 14.0);
        run(&mut c, 500, 0.1, Mode::Orbit, [-1.0, -1.0, 0.0, 1.0]);
        assert!(c.pitch >= 0.02 && c.dist <= 14.0 && c.dist >= 3.0);
        assert_eq!(c.dist, 14.0);
        run(&mut c, 500, 0.1, Mode::Orbit, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(c.dist, 3.0);
    }
    #[test]
    fn orbit_to_fly_keeps_pose_and_back() {
        let mut c = Camera::new();
        c.orbit(40.0, -30.0);
        c.zoom(0.7);
        c.update(0.5, 2, Mode::Orbit, Z, 1.0, 1.0);
        let before = c.pose();
        c.update(0.0, 2, Mode::Fly, Z, 1.0, 1.0);
        let after = c.pose();
        close(before.0, after.0);
        close(fwd_of(before), fwd_of(after));
        // fly around, then back: orbit pose untouched (auto phase aside, auto 0 here)
        let orbit_pose = {
            let mut o = Camera::new();
            o.orbit(40.0, -30.0);
            o.zoom(0.7);
            o.update(0.5, 2, Mode::Orbit, Z, 1.0, 1.0);
            o.pose()
        };
        run(&mut c, 10, 0.1, Mode::Fly, [0.5, 0.5, 0.5, 0.2]);
        c.update(0.0, 0, Mode::Orbit, Z, 1.0, 1.0);
        close(c.pose().0, orbit_pose.0);
        close(c.pose().1, orbit_pose.1);
    }
    fn fwd_of(p: ([f32; 3], [f32; 3])) -> [f32; 3] { let d = sub(p.1, p.0); let l = len(d); [d[0] / l, d[1] / l, d[2] / l] }
    #[test]
    fn default_orbit_to_fly_continuity() {
        let mut c = Camera::new();
        let before = c.pose();
        c.update(0.0, 0, Mode::Fly, Z, 1.0, 1.0);
        close(before.0, c.pose().0);
        close(fwd_of(before), fwd_of(c.pose()));
    }
    #[test]
    fn touch_in_fly_mode() {
        let mut c = flyc();
        let (y, p) = (c.fly_yaw, c.fly_pitch);
        c.orbit(10.0, 5.0);
        assert!((c.fly_yaw - (y - 0.06)).abs() < 1e-6 && (c.fly_pitch - (p - 0.03)).abs() < 1e-6);
        assert_eq!((c.yaw, c.pitch), (0.3, 0.35)); // orbit state untouched
        c.orbit(0.0, -1e6);
        assert_eq!(c.fly_pitch, 1.45);
        c.orbit(f32::NAN, 0.0);
        assert_eq!(c.fly_pitch, 1.45);
        let mut c = flyc();
        let (p0, _) = c.pose();
        let f = fwd(&c);
        c.zoom(0.9); // pinch in -> forward by 0.1*2.0
        close(sub(c.pose().0, p0), [f[0] * 0.2, f[1] * 0.2, f[2] * 0.2]);
        assert_eq!(c.dist, 10.5);
        c.zoom(f32::NAN);
        c.zoom(-1.0);
    }
    #[test]
    fn dt_bad_ignored_in_fly() {
        let mut c = fly();
        let p = c.pose();
        for dt in [f32::NAN, f32::INFINITY, -1.0] {
            c.update(dt, 0, Mode::Fly, [1.0; 4], 1.0, 1.0);
        }
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn auto_orbit_only_in_orbit_mode() {
        let mut c = fly();
        let p = c.pose();
        c.update(1.0, 2, Mode::Fly, Z, 1.0, 1.0);
        assert_eq!(c.pose(), p);
    }
    #[test]
    fn reset_keeps_mode_resets_that_pose() {
        let mut c = fly();
        run(&mut c, 10, 0.1, Mode::Fly, [1.0, 1.0, 1.0, 1.0]);
        c.orbit(5.0, 5.0);
        let fresh = fly().pose();
        c.reset();
        close(c.pose().0, fresh.0);
        close(c.pose().1, fresh.1);
        assert_eq!(c.mode(), Mode::Fly);
        // orbit untouched by a fly reset
        let mut o = Camera::new();
        o.orbit(30.0, 0.0); // yaw 0.3-0.18
        o.update(0.0, 0, Mode::Fly, Z, 1.0, 1.0);
        o.reset();
        o.update(0.0, 0, Mode::Orbit, Z, 1.0, 1.0);
        assert!((o.yaw - (0.3 - 0.18)).abs() < 1e-5);
        // orbit reset
        o.reset();
        assert_eq!((o.yaw, o.pitch, o.dist), (0.3, 0.35, 10.5));
        assert_eq!(o.mode(), Mode::Orbit);
    }
}

#[cfg(test)]
mod sign_tests {
    use super::*;
    const Z: [f32; 4] = [0.0; 4];
    fn cross_up(f: [f32; 3]) -> [f32; 3] { [-f[2], 0.0, f[0]] } // forward x up (horizontal part)
    fn dirs(c: &Camera) -> ([f32; 3], [f32; 3]) {
        let (p, t) = c.pose();
        let d = [t[0] - p[0], t[1] - p[1], t[2] - p[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let f = [d[0] / l, d[1] / l, d[2] / l];
        (f, cross_up(f))
    }
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
    #[test]
    fn fly_default_no_snap_on_first_move() {
        let mut c = Camera::new();
        c.update(0.0, 0, Mode::Fly, Z, 1.0, 1.0);
        let p0 = c.pose().0;
        c.update(0.016, 0, Mode::Fly, [0.0, 0.2, 0.0, 0.0], 1.0, 1.0);
        let p1 = c.pose().0;
        let d = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2) + (p1[2] - p0[2]).powi(2)).sqrt();
        assert!(d <= 2.5 * 0.2 * 0.016 + 1e-4, "snap {d}");
    }
    #[test]
    fn fly_signs_natural() {
        let mk = || { let mut c = Camera::new(); c.update(0.0, 0, Mode::Fly, Z, 1.0, 1.0); c.fly_pos = [0.0, 2.0, 0.0]; c };
        let mut c = mk();
        let (f0, r0) = dirs(&c);
        c.update(0.1, 0, Mode::Fly, [0.0, 0.0, 1.0, 0.0], 1.0, 1.0); // rx>0 turns right
        let (f1, _) = dirs(&c);
        assert!(dot(f1, r0) > 0.1 && dot(f1, f0) > 0.9);
        let mut c = mk();
        let y0 = dirs(&c).0[1];
        c.update(0.1, 0, Mode::Fly, [0.0, 0.0, 0.0, 1.0], 1.0, 1.0); // ry>0 looks up
        assert!(dirs(&c).0[1] > y0 + 0.1);
        let mut c = mk();
        let (f0, r0) = dirs(&c);
        let p0 = c.pose().0;
        c.update(0.1, 0, Mode::Fly, [1.0, 0.0, 0.0, 0.0], 1.0, 1.0); // lx>0 strafes right
        let d = [c.pose().0[0] - p0[0], 0.0, c.pose().0[2] - p0[2]];
        assert!(dot(d, r0) > 0.2 && dot(d, f0).abs() < 1e-3);
    }
    #[test]
    fn orbit_signs_natural() {
        let mut c = Camera::new();
        let (p0, _) = c.pose();
        let (_, r0) = dirs(&c);
        c.update(0.1, 0, Mode::Orbit, [0.0, 0.0, 1.0, 0.0], 1.0, 1.0); // rx>0: camera moves right
        let p1 = c.pose().0;
        assert!(dot([p1[0] - p0[0], 0.0, p1[2] - p0[2]], r0) > 0.1);
        let mut c = Camera::new();
        let h0 = c.pose().0[1];
        c.update(0.1, 0, Mode::Orbit, [0.0, 0.0, 0.0, 1.0], 1.0, 1.0); // ry>0 raises camera
        assert!(c.pose().0[1] > h0 && c.pitch > 0.35);
        let mut c = Camera::new();
        c.update(0.1, 0, Mode::Orbit, [0.0, 1.0, 0.0, 0.0], 1.0, 1.0); // ly>0 zoom in
        assert!(c.dist < 10.5);
        let mut c = Camera::new();
        c.update(0.1, 0, Mode::Orbit, [1.0, 0.0, 0.0, 0.0], 1.0, 1.0); // lx>0 raises at half rate
        assert!((c.pitch - 0.43).abs() < 1e-5);
    }

    #[test]
    fn heli_turn_height_forward() {
        const Z: [f32; 4] = [0.0; 4];
        let mut c = Camera::new();
        c.update(0.0, 0, Mode::Heli, Z, 1.0, 1.0);
        c.fly_pos = [0.0, 2.0, 0.0];
        let (y0, h0) = (c.fly_yaw, c.fly_pos[1]);
        c.update(0.2, 0, Mode::Heli, [1.0, 0.0, 0.0, 0.0], 1.0, 1.0); // left stick right: turns right, no movement
        assert!(c.fly_yaw < y0 && c.fly_pos[0] == 0.0 && c.fly_pos[2] == 0.0);
        c.update(0.2, 0, Mode::Heli, [0.0, 0.0, 0.0, 1.0], 1.0, 1.0); // right stick up: climbs, heading unchanged
        assert!(c.fly_pos[1] > h0);
        let yaw = c.fly_yaw;
        let p = c.fly_pos;
        c.update(0.2, 0, Mode::Heli, [0.0, 1.0, 0.0, 0.0], 1.0, 1.0); // left stick up: forward at constant height
        assert_eq!(c.fly_yaw, yaw);
        assert!((c.fly_pos[1] - p[1]).abs() < 1e-6 && (c.fly_pos[0] != p[0] || c.fly_pos[2] != p[2]));
        assert_eq!(Mode::from_param(2.0), Mode::Heli);
    }
}
