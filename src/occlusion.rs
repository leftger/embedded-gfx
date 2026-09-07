//! Software occlusion query for z-buffer-based culling on no_std embedded systems.
//!
//! Tests axis-aligned bounding boxes (AABBs) against the current depth buffer before
//! submitting expensive rasterization work — inspired by Doom BSP front-to-back ordering.
//!
//! # Example
//! ```ignore
//! use embedded_3dgfx::occlusion::{OcclusionQuery, OcclusionMode, ScreenAabb};
//!
//! let aabb = ScreenAabb::new(10, 10, 30, 30, 1.5);
//! let query = OcclusionQuery::new(OcclusionMode::Conservative);
//! // Pass a slice reference to zbuffer and width
//! let visible = query.is_visible(&aabb, zbuffer, width);
//! ```

/// An axis-aligned bounding box in screen (pixel) space, with a representative depth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenAabb {
    /// Left edge (inclusive), in pixels.
    pub x_min: i32,
    /// Top edge (inclusive), in pixels.
    pub y_min: i32,
    /// Right edge (inclusive), in pixels.
    pub x_max: i32,
    /// Bottom edge (inclusive), in pixels.
    pub y_max: i32,
    /// Representative depth of the AABB's front face (in the same units as the scene).
    pub depth: f32,
}

impl ScreenAabb {
    /// Constructs a new [`ScreenAabb`].
    #[inline]
    pub fn new(x_min: i32, y_min: i32, x_max: i32, y_max: i32, depth: f32) -> Self {
        Self {
            x_min,
            y_min,
            x_max,
            y_max,
            depth,
        }
    }

    /// Clips this AABB to the screen rectangle `[0, width) × [0, height)`.
    ///
    /// If the AABB lies entirely outside the screen the returned value will
    /// have `x_min > x_max` or `y_min > y_max` (detectable via [`Self::is_empty`]).
    #[inline]
    pub fn clamp(self, width: i32, height: i32) -> Self {
        Self {
            x_min: self.x_min.max(0),
            y_min: self.y_min.max(0),
            x_max: self.x_max.min(width - 1),
            y_max: self.y_max.min(height - 1),
            depth: self.depth,
        }
    }

    /// Returns `true` when the (possibly clamped) region has zero or negative area.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.x_min > self.x_max || self.y_min > self.y_max
    }
}

/// Controls how many pixels of the AABB are probed against the z-buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcclusionMode {
    /// Sample only the four corner pixels of the AABB.
    ///
    /// Very fast (O(1)), but may miss occlusion when the corners happen to be
    /// inside already-rasterised geometry while the interior is still visible.
    Conservative,

    /// Sample every pixel inside the AABB rectangle.
    ///
    /// Exact result, but O(area) — use on small or distant objects.
    Accurate,
}

/// Occlusion query tester.
///
/// Call [`is_visible`](OcclusionQuery::is_visible) to test whether any part of
/// a [`ScreenAabb`] is potentially visible given the current z-buffer contents.
#[derive(Debug, Clone, Copy)]
pub struct OcclusionQuery {
    /// Sampling strategy used for z-buffer probing.
    pub mode: OcclusionMode,
}

/// Convert a floating-point scene depth into a fixed-point `u32` representation.
#[inline(always)]
fn depth_to_fixed(d: f32) -> u32 {
    (d * 65536.0) as u32
}

impl OcclusionQuery {
    /// Creates a new [`OcclusionQuery`] with the given sampling [`OcclusionMode`].
    #[inline]
    pub fn new(mode: OcclusionMode) -> Self {
        Self { mode }
    }

    /// Tests whether the [`ScreenAabb`] is potentially visible in the current z-buffer.
    ///
    /// Returns `true` if **any** sampled pixel is either:
    /// - unwritten (`zbuffer[idx] == Z_MAX_VALUE`), or
    /// - written with a depth value **greater than** the AABB's depth (meaning
    ///   the AABB is closer to the camera than whatever was already drawn there).
    ///
    /// Returns `false` (fully occluded) only when every sampled pixel already
    /// holds a depth value that is closer than (or equal to) the AABB depth.
    ///
    /// # Arguments
    /// - `aabb`    – Screen-space bounding box to test.
    /// - `zbuffer` – Flat row-major depth buffer slice.
    /// - `width`   – Width of the framebuffer in pixels (stride of `zbuffer`).
    pub fn is_visible(&self, aabb: &ScreenAabb, zbuffer: &[crate::ZDepth], width: usize) -> bool {
        if aabb.is_empty() {
            // A degenerate / fully off-screen AABB is considered not visible.
            return false;
        }

        let aabb_zdepth = crate::to_zdepth(depth_to_fixed(aabb.depth));

        match self.mode {
            OcclusionMode::Conservative => {
                // Probe the four corners only.
                let corners = [
                    (aabb.x_min, aabb.y_min),
                    (aabb.x_max, aabb.y_min),
                    (aabb.x_min, aabb.y_max),
                    (aabb.x_max, aabb.y_max),
                ];
                for (cx, cy) in corners {
                    if Self::pixel_visible(zbuffer, width, cx, cy, aabb_zdepth) {
                        return true;
                    }
                }
                false
            }
            OcclusionMode::Accurate => {
                // Probe every pixel in the rectangle.
                for py in aabb.y_min..=aabb.y_max {
                    for px in aabb.x_min..=aabb.x_max {
                        if Self::pixel_visible(zbuffer, width, px, py, aabb_zdepth) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }

    /// Returns `true` if the pixel at `(px, py)` is potentially visible for an
    /// object at `aabb_zdepth`.
    ///
    /// A pixel is visible if:
    /// - it has never been written (`zbuffer[idx] == Z_MAX_VALUE`), **or**
    /// - the stored depth is *greater than* `aabb_zdepth` (the incoming object
    ///   is closer than what is already there).
    #[inline(always)]
    fn pixel_visible(
        zbuffer: &[crate::ZDepth],
        width: usize,
        px: i32,
        py: i32,
        aabb_zdepth: crate::ZDepth,
    ) -> bool {
        if px < 0 || py < 0 {
            return false;
        }
        let idx = py as usize * width + px as usize;
        if idx >= zbuffer.len() {
            return false;
        }
        let stored = zbuffer[idx];
        // Unwritten pixel, or AABB is closer than current occupant.
        stored == crate::Z_MAX_VALUE || stored > aabb_zdepth
    }
}

// ── Statistics ────────────────────────────────────────────────────────────────

/// Lightweight profiling counters for occlusion queries.
///
/// Call [`record`](OcclusionStats::record) after each [`OcclusionQuery::is_visible`]
/// call, then inspect [`cull_ratio`](OcclusionStats::cull_ratio) to measure how
/// effective the culling is.
#[derive(Debug, Clone, Copy, Default)]
pub struct OcclusionStats {
    /// Total number of queries submitted.
    pub queries: u32,
    /// Queries where `is_visible` returned `true`.
    pub passed: u32,
    /// Queries where `is_visible` returned `false` (culled).
    pub culled: u32,
}

impl OcclusionStats {
    /// Creates a zeroed [`OcclusionStats`].
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the result of one occlusion query.
    #[inline]
    pub fn record(&mut self, visible: bool) {
        self.queries += 1;
        if visible {
            self.passed += 1;
        } else {
            self.culled += 1;
        }
    }

    /// Returns the fraction of queries that were culled.
    ///
    /// Returns `0.0` when no queries have been submitted yet.
    #[inline]
    pub fn cull_ratio(&self) -> f32 {
        self.culled as f32 / self.queries.max(1) as f32
    }

    /// Resets all counters to zero.
    #[inline]
    pub fn reset(&mut self) {
        self.queries = 0;
        self.passed = 0;
        self.culled = 0;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::{Z_MAX_VALUE, ZDepth};

    // ── ScreenAabb::clamp ────────────────────────────────────────────────────

    #[test]
    fn test_screen_aabb_clamp() {
        let aabb = ScreenAabb::new(-5, -10, 50, 60, 1.0);
        let clamped = aabb.clamp(40, 30);

        assert_eq!(clamped.x_min, 0);
        assert_eq!(clamped.y_min, 0);
        assert_eq!(clamped.x_max, 39); // width-1
        assert_eq!(clamped.y_max, 29); // height-1
        assert!(!clamped.is_empty());

        // Entirely off-screen → is_empty()
        let off_screen = ScreenAabb::new(100, 100, 200, 200, 1.0).clamp(80, 60);
        assert!(off_screen.is_empty());
    }

    // ── OcclusionQuery on an empty z-buffer ──────────────────────────────────

    #[test]
    fn test_occlusion_empty_zbuffer() {
        const W: usize = 64;
        const H: usize = 64;
        let zbuf: std::vec::Vec<ZDepth> = std::vec![Z_MAX_VALUE; W * H];

        let aabb = ScreenAabb::new(10, 10, 20, 20, 5.0);

        // Both modes must report visible on a fresh (all-max) z-buffer.
        let conservative = OcclusionQuery::new(OcclusionMode::Conservative);
        assert!(
            conservative.is_visible(&aabb, &zbuf, W),
            "Conservative: empty z-buffer must be visible"
        );

        let accurate = OcclusionQuery::new(OcclusionMode::Accurate);
        assert!(
            accurate.is_visible(&aabb, &zbuf, W),
            "Accurate: empty z-buffer must be visible"
        );
    }

    // ── OcclusionQuery with a fully-written z-buffer ─────────────────────────

    #[test]
    fn test_occlusion_fully_occluded() {
        const W: usize = 64;
        const H: usize = 64;

        // Fill the z-buffer with depth 0 — the closest possible value.
        // Anything submitted at depth 10.0 should be behind this and invisible.
        let zbuf: std::vec::Vec<ZDepth> = std::vec![0; W * H];

        let aabb = ScreenAabb::new(10, 10, 20, 20, 10.0);

        let conservative = OcclusionQuery::new(OcclusionMode::Conservative);
        assert!(
            !conservative.is_visible(&aabb, &zbuf, W),
            "Conservative: should be fully occluded"
        );

        let accurate = OcclusionQuery::new(OcclusionMode::Accurate);
        assert!(
            !accurate.is_visible(&aabb, &zbuf, W),
            "Accurate: should be fully occluded"
        );
    }

    // ── OcclusionStats ───────────────────────────────────────────────────────

    #[test]
    fn test_occlusion_stats() {
        let mut stats = OcclusionStats::new();

        // No queries yet → ratio is 0.
        assert_eq!(stats.cull_ratio(), 0.0);

        stats.record(true); // passed
        stats.record(true); // passed
        stats.record(false); // culled
        stats.record(false); // culled

        assert_eq!(stats.queries, 4);
        assert_eq!(stats.passed, 2);
        assert_eq!(stats.culled, 2);
        assert!((stats.cull_ratio() - 0.5).abs() < 1e-6);

        stats.reset();
        assert_eq!(stats.queries, 0);
        assert_eq!(stats.passed, 0);
        assert_eq!(stats.culled, 0);
        assert_eq!(stats.cull_ratio(), 0.0);
    }
}
