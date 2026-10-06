pub struct AdaptiveRes {
    min: f32,
    max: f32,
    scale: f32,
    ema: f32,
    last_change: f32,
}

impl AdaptiveRes {
    pub fn new(min: f32, max: f32, start: f32) -> Self {
        AdaptiveRes { min, max, scale: start.clamp(min, max), ema: 0.0, last_change: f32::NEG_INFINITY }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn set_max(&mut self, max: f32) {
        self.max = max.max(self.min);
        self.scale = self.scale.min(self.max);
    }

    /// `gpu_ms`: GPU time of the last frame. Returns the (possibly new) scale.
    pub fn update(&mut self, gpu_ms: f32, target_fps: f32, now_s: f32) -> f32 {
        if !gpu_ms.is_finite() || gpu_ms <= 0.0 {
            return self.scale;
        }
        self.ema = if self.ema == 0.0 { gpu_ms } else { self.ema * 0.9 + gpu_ms * 0.1 };
        if now_s - self.last_change < 0.5 {
            return self.scale;
        }
        let ratio = (1000.0 / target_fps) * 0.85 / self.ema; // keep 15% headroom
        let ns = if ratio < 0.95 {
            self.scale * ratio.sqrt().max(0.8)
        } else if ratio > 1.2 {
            self.scale * ratio.sqrt().min(1.1)
        } else {
            return self.scale;
        };
        let ns = ns.clamp(self.min, self.max);
        if (ns - self.scale).abs() > 0.004 {
            self.scale = ns;
            self.last_change = now_s;
            self.ema = 0.0; // re-measure at the new size
        }
        self.scale
    }
}

/// Render size: at least 8x8, multiple of 8 px (rounded down), never larger than the window.
pub fn render_size(win_w: u32, win_h: u32, scale: f32) -> (u32, u32) {
    let f = |v: u32| (((v as f32 * scale) as u32) & !7).clamp(8, v.max(8));
    (f(win_w), f(win_h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_when_slow() {
        let mut a = AdaptiveRes::new(0.25, 1.0, 0.5);
        let s = a.update(40.0, 60.0, 1.0);
        assert!(s < 0.5 && s >= 0.4, "{s}");
    }

    #[test]
    fn rises_when_fast() {
        let mut a = AdaptiveRes::new(0.25, 1.0, 0.5);
        let s = a.update(4.0, 60.0, 1.0);
        assert!(s > 0.5 && s <= 0.55 + 1e-6, "{s}");
    }

    #[test]
    fn rate_limited() {
        let mut a = AdaptiveRes::new(0.25, 1.0, 0.5);
        let s1 = a.update(40.0, 60.0, 1.0);
        let s2 = a.update(40.0, 60.0, 1.2);
        assert_eq!(s1, s2);
        assert!(a.update(40.0, 60.0, 1.6) < s2);
    }

    #[test]
    fn deadband_holds() {
        let mut a = AdaptiveRes::new(0.25, 1.0, 0.5);
        assert_eq!(a.update(14.0, 60.0, 1.0), 0.5); // 16.6*0.85/14 = 1.0
    }

    #[test]
    fn clamps_to_range_and_bad_input() {
        let mut a = AdaptiveRes::new(0.25, 0.5, 0.5);
        assert_eq!(a.update(1.0, 60.0, 1.0), 0.5);
        assert_eq!(a.update(f32::NAN, 60.0, 5.0), 0.5);
        let mut b = AdaptiveRes::new(0.25, 1.0, 0.26);
        for i in 0..50 { b.update(500.0, 60.0, i as f32); }
        assert_eq!(b.scale(), 0.25);
    }

    #[test]
    fn render_size_bounds() {
        assert_eq!(render_size(1080, 2400, 0.33), (352, 792));
        assert_eq!(render_size(1, 1, 0.25), (8, 8));
        assert_eq!(render_size(100, 100, 1.0), (96, 96));
        let (w, h) = render_size(4, 4000, 1.0);
        assert!(w >= 8 && h <= 4000);
    }
}
