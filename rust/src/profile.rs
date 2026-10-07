//! Timestamp helpers (pure, host-tested).

/// GPU ticks -> milliseconds. `period_ns` is `Queue::get_timestamp_period()` (ns per tick).
pub fn ticks_to_ms(begin: u64, end: u64, period_ns: f32) -> f32 {
    if begin == 0 || end == 0 || end < begin || !period_ns.is_finite() { return 0.0; }
    ((end - begin) as f64 * period_ns as f64 / 1.0e6) as f32
}

/// wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT (wgpu-types 24, lib.rs:57).
pub const RESOLVE_ALIGN: u64 = 256;

/// Per pass slot: (query index range, resolve-buffer byte offset, readback byte offset).
pub fn slot_offsets(slot: u32) -> (std::ops::Range<u32>, u64, u64) {
    (2 * slot..2 * slot + 2, slot as u64 * RESOLVE_ALIGN, slot as u64 * 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slot_offsets_aligned() {
        assert_eq!(slot_offsets(0), (0..2, 0, 0));
        assert_eq!(slot_offsets(3), (6..8, 768, 48));
        for s in 0..8 { assert_eq!(slot_offsets(s).1 % RESOLVE_ALIGN, 0); }
    }
    #[test]
    fn converts_and_guards() {
        assert!((ticks_to_ms(1_000, 2_001_000, 1.0) - 2.0).abs() < 1e-4);
        assert!((ticks_to_ms(10, 20, 52.083333) - 0.000521).abs() < 1e-5);
        assert_eq!(ticks_to_ms(0, 5, 1.0), 0.0);
        assert_eq!(ticks_to_ms(9, 5, 1.0), 0.0);
        assert_eq!(ticks_to_ms(1, 5, f32::NAN), 0.0);
    }
}
