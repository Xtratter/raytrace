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

/// Timestamp writes for one pass: wgpu rejects a pass whose begin and end indices are both absent,
/// so such a pass must get no timestamp writes at all (None).
pub fn pass_indices(b: Option<u32>, e: Option<u32>) -> Option<(Option<u32>, Option<u32>)> {
    if b.is_none() && e.is_none() { None } else { Some((b, e)) }
}

/// Query indices for a-trous pass `i` of `n` (the chain shares slot 5: begin on first, end on last pass).
pub fn atrous_query_indices(i: u32, n: u32) -> (Option<u32>, Option<u32>) {
    (if i == 0 { Some(10) } else { None }, if i + 1 == n { Some(11) } else { None })
}

/// Query indices for GI a-trous pass `i` (0..2; slot 3: begin on the first, end on the second).
pub fn gi_atrous_query_indices(i: u32) -> (Option<u32>, Option<u32>) {
    if i == 0 { (Some(6), None) } else { (None, Some(7)) }
}

/// Log suffix with per-pass GPU times (tr gi gt ga tm at cm pr); empty when all are zero.
pub fn format_pass_ms(p: &[f32; 8]) -> String {
    if p.iter().all(|&v| v == 0.0) { return String::new(); }
    const N: [&str; 8] = ["tr", "gi", "gt", "ga", "tm", "at", "cm", "pr"];
    let mut s = String::from(" |");
    for (n, v) in N.iter().zip(p) { s.push_str(&format!(" {} {:.1}", n, v)); }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pass_indices_none_when_empty() {
        assert_eq!(pass_indices(None, None), None);
        assert_eq!(pass_indices(Some(1), None), Some((Some(1), None)));
        assert_eq!(pass_indices(None, Some(2)), Some((None, Some(2))));
    }
    #[test]
    fn schedules_valid_and_unique() {
        let mut cases: Vec<Vec<(Option<u32>, Option<u32>)>> = vec![];
        for n in 0..=3u32 { cases.push((0..n).map(|i| atrous_query_indices(i, n)).collect()); }
        cases.push((0..2u32).map(gi_atrous_query_indices).collect());
        for passes in cases {
            let mut used = std::collections::HashSet::new();
            for (b, e) in passes {
                if let Some((b, e)) = pass_indices(b, e) {
                    assert!(b.is_some() || e.is_some());
                    for q in [b, e].into_iter().flatten() { assert!(used.insert(q), "query {q} reused"); }
                }
            }
        }
    }
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

    #[test]
    fn format_pass_ms_cases() {
        assert_eq!(format_pass_ms(&[0.0; 8]), "");
        assert_eq!(format_pass_ms(&[51.94, 0.4, 0.3, 3.4, 0.7, 2.0, 0.5, 2.4]),
            " | tr 51.9 gi 0.4 gt 0.3 ga 3.4 tm 0.7 at 2.0 cm 0.5 pr 2.4");
    }
}
