//! Camera ↔ mesh visibility bitmasks (`RenderLayers`).
//!
//! A mesh is drawn only when `camera.layers.intersects(mesh.layers)`.
//! Layer 0 is the default for both cameras and meshes.

/// Bitmask of visibility layers (`u64` → 64 layers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderLayers(pub u64);

impl RenderLayers {
    /// Default layer set: layer 0 only.
    pub const DEFAULT: Self = Self(1);

    /// Belong to no layers (never intersects anything, including empty).
    pub const NONE: Self = Self(0);

    /// Belong to all 64 layers.
    pub const ALL: Self = Self(u64::MAX);

    /// Single layer `n` (`0..64`). Out-of-range indices yield [`Self::NONE`].
    #[inline]
    pub const fn layer(n: u8) -> Self {
        if n >= 64 { Self::NONE } else { Self(1u64 << n) }
    }

    /// Empty layer mask.
    #[inline]
    pub const fn none() -> Self {
        Self::NONE
    }

    /// Add layer `n` to this mask.
    #[inline]
    pub const fn with(self, n: u8) -> Self {
        if n >= 64 {
            self
        } else {
            Self(self.0 | (1u64 << n))
        }
    }

    /// Remove layer `n` from this mask.
    #[inline]
    pub const fn without(self, n: u8) -> Self {
        if n >= 64 {
            self
        } else {
            Self(self.0 & !(1u64 << n))
        }
    }

    /// `true` when the masks share at least one layer.
    ///
    /// Two empty masks do **not** intersect.
    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl Default for RenderLayers {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_intersects_layer0() {
        assert!(RenderLayers::DEFAULT.intersects(RenderLayers::layer(0)));
        assert!(!RenderLayers::DEFAULT.intersects(RenderLayers::layer(1)));
    }

    #[test]
    fn empty_never_intersects() {
        assert!(!RenderLayers::NONE.intersects(RenderLayers::NONE));
        assert!(!RenderLayers::NONE.intersects(RenderLayers::ALL));
    }

    #[test]
    fn with_without() {
        let layers = RenderLayers::layer(0).with(3).with(7);
        assert!(layers.intersects(RenderLayers::layer(3)));
        assert!(!layers.without(3).intersects(RenderLayers::layer(3)));
    }

    #[test]
    fn out_of_range_all_bits_and_default() {
        assert_eq!(RenderLayers::layer(64), RenderLayers::NONE);
        let unchanged = RenderLayers::layer(1).with(99).without(99);
        assert_eq!(unchanged, RenderLayers::layer(1));
        assert_eq!(RenderLayers::ALL.bits(), u64::MAX);
        assert!(RenderLayers::ALL.intersects(RenderLayers::ALL));
        assert_eq!(RenderLayers::default(), RenderLayers::DEFAULT);
        assert_eq!(RenderLayers::none(), RenderLayers::NONE);
    }
}
