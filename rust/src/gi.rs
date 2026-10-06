//! Helpers shared (by formula) with the GI shaders; kept pure so the host can test them.

/// PCG hash, identical to `pcg` in common.wgsl.
pub fn pcg(v: u32) -> u32 {
    let s = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277803737);
    (w >> 22) ^ w
}

/// Which pixel of the `bs` x `bs` block (gx, gy) is traced this frame. Over `bs*bs` consecutive
/// frames every pixel of the block is visited once; a per-block hash decorrelates neighbours.
/// Identical to `block_offset` in common.wgsl.
pub fn block_offset(gx: u32, gy: u32, frame: u32, bs: u32) -> (u32, u32) {
    let n = bs * bs;
    let h = pcg(gx.wrapping_mul(73856093) ^ gy.wrapping_mul(19349663));
    let k = frame.wrapping_add(h) % n;
    (k % bs, k / bs)
}

/// Half/quarter-resolution size for a render size (ceil, never zero).
pub fn gi_size(w: u32, h: u32, bs: u32) -> (u32, u32) {
    (w.max(1).div_ceil(bs), h.max(1).div_ceil(bs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gi_size_ceils_and_never_zero() {
        assert_eq!(gi_size(353, 791, 2), (177, 396));
        assert_eq!(gi_size(8, 8, 4), (2, 2));
        assert_eq!(gi_size(1, 1, 2), (1, 1));
        assert_eq!(gi_size(264, 600, 4), (66, 150));
        assert_eq!(gi_size(0, 0, 2), (1, 1));
    }

    #[test]
    fn block_offset_in_range_and_covers_block_over_n_frames() {
        for bs in [2u32, 4] {
            let n = bs * bs;
            for (gx, gy) in [(0u32, 0u32), (1, 0), (0, 1), (37, 91), (65535, 1)] {
                for start in [0u32, 1000, u32::MAX - 1, u32::MAX - n + 1, 12345] {
                    let mut seen = vec![false; n as usize];
                    for f in 0..n {
                        let (ox, oy) = block_offset(gx, gy, start.wrapping_add(f), bs);
                        assert!(ox < bs && oy < bs);
                        seen[(oy * bs + ox) as usize] = true;
                    }
                    assert!(seen.iter().all(|&s| s), "bs {bs} block ({gx},{gy}) start {start} missed an offset");
                }
            }
        }
    }

    #[test]
    fn block_offset_differs_between_neighbouring_blocks() {
        // hash decorrelation: not all neighbours use the same offset on one frame
        let a: Vec<_> = (0..16u32).map(|gx| block_offset(gx, 3, 7, 2)).collect();
        assert!(a.iter().any(|&o| o != a[0]));
    }

    #[test]
    fn pcg_matches_known_values() {
        // reference values computed independently (python) from the same formula and constants as common.wgsl
        assert_eq!(pcg(0), 0x7bb2fe2);
        assert_eq!(pcg(1), 0xa8beea3c);
    }
}
