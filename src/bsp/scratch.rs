//! Per-frame mutable scratch memory for BSP traversal.
//!
//! Create once, pass `&mut BspScratch` into [`K3dengine::record_bsp`](crate::K3dengine::record_bsp) each
//! frame.  The frame counter wraps safely and the visframe array is zero-cost
//! to "clear" — a wrapping increment of `frame` is the full reset.

/// Per-frame scratch buffer used to de-duplicate faces during BSP traversal.
///
/// A face shared by multiple leaves (possible with Quake-style marksurface
/// tables) is emitted at most once per frame.  The caller owns the backing
/// slice; length must equal `BspWorld::faces.len()`.
pub struct BspScratch<'a> {
    /// `face_visframe[i] == frame` iff face `i` was already emitted this frame.
    pub face_visframe: &'a mut [u32],
    /// Monotonically-increasing frame counter (wraps via `wrapping_add`).
    pub frame: u32,
}

impl<'a> BspScratch<'a> {
    /// Create from a caller-owned slice.
    ///
    /// The slice should be zero-initialised on first use; subsequent frames
    /// require no manual clearing.
    pub fn new(face_visframe: &'a mut [u32]) -> Self {
        Self {
            face_visframe,
            frame: 0,
        }
    }

    /// Advance to a new frame.  Call once at the start of each `record_bsp`.
    #[inline]
    pub fn mark_new_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Returns `true` if face `face_idx` was already emitted this frame.
    #[inline]
    pub fn is_marked(&self, face_idx: usize) -> bool {
        self.face_visframe.get(face_idx).copied().unwrap_or(0) == self.frame
    }

    /// Mark face `face_idx` as emitted for the current frame.
    #[inline]
    pub fn mark(&mut self, face_idx: usize) {
        if let Some(slot) = self.face_visframe.get_mut(face_idx) {
            *slot = self.frame;
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn new_frame_deduplication() {
        let mut buf = [0u32; 4];
        let mut s = BspScratch::new(&mut buf);

        s.mark_new_frame();
        assert!(!s.is_marked(0));
        s.mark(0);
        assert!(s.is_marked(0));
        assert!(!s.is_marked(1));

        // Advancing frame resets all marks
        s.mark_new_frame();
        assert!(!s.is_marked(0));
    }

    #[test]
    fn out_of_bounds_mark_is_noop() {
        let mut buf = [0u32; 2];
        let mut s = BspScratch::new(&mut buf);
        s.mark_new_frame();
        s.mark(99); // out of bounds — should not panic
        assert!(!s.is_marked(99));
    }

    #[test]
    fn frame_wraps_safely() {
        let mut buf = [0u32; 1];
        let mut s = BspScratch::new(&mut buf);
        s.frame = u32::MAX;
        s.mark_new_frame();
        assert_eq!(s.frame, 0);
    }
}
