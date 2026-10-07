//! Timestamp helpers (pure, host-tested).

/// GPU ticks -> milliseconds. `period_ns` is `Queue::get_timestamp_period()` (ns per tick).
pub fn ticks_to_ms(begin: u64, end: u64, period_ns: f32) -> f32 {
    if begin == 0 || end == 0 || end < begin || !period_ns.is_finite() { return 0.0; }
    ((end - begin) as f64 * period_ns as f64 / 1.0e6) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converts_and_guards() {
        assert!((ticks_to_ms(1_000, 2_001_000, 1.0) - 2.0).abs() < 1e-4);
        assert!((ticks_to_ms(10, 20, 52.083333) - 0.000521).abs() < 1e-5);
        assert_eq!(ticks_to_ms(0, 5, 1.0), 0.0);
        assert_eq!(ticks_to_ms(9, 5, 1.0), 0.0);
        assert_eq!(ticks_to_ms(1, 5, f32::NAN), 0.0);
    }
}
