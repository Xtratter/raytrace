//! CPU reference of the temporal blend; `temporal.wgsl` implements the same math.
/// Rgba16Float history: once n > ~1000, 1/n drops below the f16 half-ULP and the running
/// mean stalls (and can drift brighter), so the still-PT length is capped well below that.
pub const LEN_CAP_STILL: f32 = 512.0;

fn sane(c: [f32; 3]) -> [f32; 3] {
    if c.iter().any(|x| !x.is_finite() || x.abs() > 1e4) { [0.0; 3] } else { c }
}

/// `hist`/`n`: reprojected history colour and length (0 = none). `floor`: minimum weight of the new sample.
/// Returns (colour, new length).
pub fn blend(hist: [f32; 3], n: f32, cur: [f32; 3], floor: f32, still_pt: bool) -> ([f32; 3], f32) {
    let cur = sane(cur);
    if !(n > 0.0) {
        return (cur, 1.0);
    }
    let hist = sane(hist);
    let n = n.min(LEN_CAP_STILL);
    let nn = n + 1.0;
    let a = if still_pt { 1.0 / nn } else { (1.0 / nn).max(floor) };
    let cap = if still_pt { LEN_CAP_STILL } else { (1.0 / floor).ceil() };
    let out = [
        hist[0] + (cur[0] - hist[0]) * a,
        hist[1] + (cur[1] - hist[1]) * a,
        hist[2] + (cur[2] - hist[2]) * a,
    ];
    (out, nn.min(cap))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_history_takes_current() {
        assert_eq!(blend([9.0; 3], 0.0, [1.0, 2.0, 3.0], 0.2, false), ([1.0, 2.0, 3.0], 1.0));
    }

    #[test]
    fn nan_current_does_not_poison() {
        let (c, n) = blend([0.5; 3], 10.0, [f32::NAN, 1.0, f32::INFINITY], 0.2, false);
        assert!(c.iter().all(|x| x.is_finite()));
        assert!(n >= 1.0);
    }

    #[test]
    fn nan_history_does_not_poison() {
        let (c, _) = blend([f32::NAN; 3], 10.0, [1.0; 3], 0.2, false);
        assert!(c.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn floor_respected_when_moving() {
        let (c, n) = blend([0.0; 3], 500.0, [1.0; 3], 0.2, false);
        assert!((c[0] - 0.2).abs() < 1e-6);
        assert_eq!(n, 5.0); // capped at ceil(1/floor)
    }

    #[test]
    fn still_pt_converges_to_mean() {
        let mut c = [0.0; 3];
        let mut n = 0.0;
        let mut sum = 0.0f64;
        for i in 0..5000u32 {
            let s = if i % 2 == 0 { 1.0 } else { 3.0 };
            sum += s as f64;
            let (nc, nn) = blend(c, n, [s; 3], 0.2, true);
            c = nc;
            n = nn;
            assert!(c[0].is_finite());
        }
        assert!((c[0] as f64 - sum / 5000.0).abs() < 2e-2, "{}", c[0]);
        assert_eq!(n, LEN_CAP_STILL);
    }
}
