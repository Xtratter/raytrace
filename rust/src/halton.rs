pub fn halton(mut i: u32, base: u32) -> f32 {
    let mut f = 1.0;
    let mut r = 0.0;
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

pub fn jitter(frame: u32) -> [f32; 2] {
    let i = frame % 1024 + 1;
    [halton(i, 2) - 0.5, halton(i, 3) - 0.5]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_values() {
        assert_eq!(halton(1, 2), 0.5);
        assert_eq!(halton(2, 2), 0.25);
        assert_eq!(halton(3, 2), 0.75);
        assert!((halton(1, 3) - 1.0 / 3.0).abs() < 1e-6);
    }
    #[test]
    fn jitter_in_range_and_cycles() {
        for f in 0..3000 {
            let j = jitter(f);
            assert!(j[0] > -0.5 && j[0] < 0.5 && j[1] > -0.5 && j[1] < 0.5);
        }
        assert_eq!(jitter(5), jitter(5 + 1024));
    }
}
