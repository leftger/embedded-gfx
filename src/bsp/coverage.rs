//! Per-pixel coverage buffer for BSP front-to-back rendering (Milestone 5).
//!
//! In front-to-back BSP traversal the *first* draw to any pixel is always the
//! correct (nearest) surface.  A 1-bit coverage bitmap then replaces the full
//! 32-bit z-buffer, saving ~128 KB RAM at 240×135.
//!
//! # Usage
//! ```rust,ignore
//! const W: usize = 240;
//! const H: usize = 135;
//! let mut cov_data = [0u8; CoverageBuffer::bytes_for(W, H)];
//! let mut cov = CoverageBuffer::new(&mut cov_data, W, H).unwrap();
//!
//! // each frame:
//! cov.clear();
//! engine.render_bsp_coverage(&world, &mut scratch, &texture_mgr, fb, &mut cov, tel);
//! ```

/// Single-bit-per-pixel coverage bitmap.
///
/// Bits are packed LSB-first within each byte:
/// pixel `(x, y)` occupies bit `(y*width + x) % 8` of byte `(y*width + x) / 8`.
pub struct CoverageBuffer<'a> {
    data: &'a mut [u8],
    pub width: usize,
    pub height: usize,
}

impl<'a> CoverageBuffer<'a> {
    /// Returns the number of bytes needed for `width × height` pixels.
    pub const fn bytes_for(width: usize, height: usize) -> usize {
        (width * height + 7) / 8
    }

    /// Create from a caller-owned byte slice.
    ///
    /// Returns `None` if `data` is too short.
    pub fn new(data: &'a mut [u8], width: usize, height: usize) -> Option<Self> {
        let needed = Self::bytes_for(width, height);
        if data.len() < needed {
            return None;
        }
        Some(Self {
            data,
            width,
            height,
        })
    }

    /// Zero all coverage bits (call once per frame before rendering).
    #[inline]
    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Returns `true` if pixel `(x, y)` has already been drawn this frame.
    #[inline]
    pub fn is_covered(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let idx = y * self.width + x;
        let byte = idx >> 3;
        let bit = idx & 7;
        // Safety: byte < bytes_for(width, height) ≤ data.len()
        (self.data[byte] >> bit) & 1 != 0
    }

    /// Mark pixel `(x, y)` as drawn.
    #[inline]
    pub fn mark_covered(&mut self, x: usize, y: usize) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = y * self.width + x;
        let byte = idx >> 3;
        let bit = idx & 7;
        self.data[byte] |= 1 << bit;
    }

    /// Returns `true` when all pixels are covered (early-exit for the renderer).
    pub fn is_full(&self) -> bool {
        let pixels = self.width * self.height;
        let full_bytes = pixels >> 3;
        let rem_bits = pixels & 7;

        for &b in &self.data[..full_bytes] {
            if b != 0xFF {
                return false;
            }
        }
        if rem_bits > 0 {
            let mask = (1u8 << rem_bits) - 1;
            let last = self.data.get(full_bytes).copied().unwrap_or(0);
            if last & mask != mask {
                return false;
            }
        }
        true
    }

    /// Returns the fraction of pixels covered, as a value in [0.0, 1.0].
    /// Useful for telemetry / performance tuning.
    #[cfg(feature = "std")]
    pub fn fill_ratio(&self) -> f32 {
        let pixels = self.width * self.height;
        let covered = self.data.iter().map(|b| b.count_ones() as usize).sum::<usize>();
        let covered = covered.min(pixels);
        covered as f32 / pixels as f32
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn bytes_for_240x135() {
        assert_eq!(CoverageBuffer::bytes_for(240, 135), (240 * 135 + 7) / 8);
    }

    #[test]
    fn new_rejects_undersized_buffer() {
        let mut buf = [0u8; 1];
        assert!(CoverageBuffer::new(&mut buf, 240, 135).is_none());
    }

    #[test]
    fn mark_and_query() {
        let mut buf = [0u8; CoverageBuffer::bytes_for(8, 8)];
        let mut cov = CoverageBuffer::new(&mut buf, 8, 8).unwrap();

        assert!(!cov.is_covered(3, 2));
        cov.mark_covered(3, 2);
        assert!(cov.is_covered(3, 2));
        // Neighbours untouched
        assert!(!cov.is_covered(2, 2));
        assert!(!cov.is_covered(4, 2));
        assert!(!cov.is_covered(3, 1));
        assert!(!cov.is_covered(3, 3));
    }

    #[test]
    fn clear_resets_coverage() {
        let mut buf = [0u8; CoverageBuffer::bytes_for(4, 4)];
        let mut cov = CoverageBuffer::new(&mut buf, 4, 4).unwrap();
        cov.mark_covered(1, 1);
        assert!(cov.is_covered(1, 1));
        cov.clear();
        assert!(!cov.is_covered(1, 1));
    }

    #[test]
    fn is_full_after_marking_all() {
        let mut buf = [0u8; CoverageBuffer::bytes_for(4, 4)];
        let mut cov = CoverageBuffer::new(&mut buf, 4, 4).unwrap();
        assert!(!cov.is_full());
        for y in 0..4 {
            for x in 0..4 {
                cov.mark_covered(x, y);
            }
        }
        assert!(cov.is_full());
    }

    #[test]
    fn out_of_bounds_is_covered_returns_false() {
        let mut buf = [0u8; CoverageBuffer::bytes_for(4, 4)];
        let cov = CoverageBuffer::new(&mut buf, 4, 4).unwrap();
        assert!(!cov.is_covered(4, 0));
        assert!(!cov.is_covered(0, 4));
        assert!(!cov.is_covered(100, 100));
    }
}
