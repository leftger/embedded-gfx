//! Display backend abstraction for DMA-based rendering
//!
//! This module provides a platform-agnostic interface for asynchronous
//! framebuffer transfers using DMA (Direct Memory Access). This enables
//! double-buffered rendering where the CPU can render to one buffer while
//! the display hardware transfers another buffer.

use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_framebuf::{FrameBuf, backends::DMACapableFrameBufferBackend};

/// Rectangle region for partial framebuffer presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayRegion {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl DisplayRegion {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Error types for display backend operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    /// DMA transfer is still in progress
    Busy,
    /// Hardware error during transfer
    HardwareError,
    /// Invalid buffer configuration
    InvalidBuffer,
}

/// Platform-agnostic display backend trait
///
/// Implementations of this trait handle the hardware-specific details of
/// transferring framebuffer data to the display using DMA.
pub trait DisplayBackend<const W: usize, const H: usize, FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    /// Start a non-blocking DMA transfer of the framebuffer to the display
    ///
    /// # Arguments
    /// * `framebuffer` - The framebuffer to transfer
    ///
    /// # Returns
    /// `Ok(())` if transfer started successfully, `Err(DisplayError)` otherwise
    ///
    /// # Note
    /// This function should not block. If a transfer is already in progress,
    /// it should return `Err(DisplayError::Busy)`.
    fn start_dma_transfer(
        &mut self,
        framebuffer: &FrameBuf<Rgb565, FB>,
    ) -> Result<(), DisplayError>;

    /// Start a non-blocking transfer of a framebuffer sub-region.
    ///
    /// Default implementation falls back to full-frame transfer.
    fn start_dma_transfer_region(
        &mut self,
        framebuffer: &FrameBuf<Rgb565, FB>,
        _region: DisplayRegion,
    ) -> Result<(), DisplayError> {
        self.start_dma_transfer(framebuffer)
    }

    /// Wait for the current DMA transfer to complete
    ///
    /// This function blocks until the DMA transfer finishes.
    fn wait_for_dma(&mut self);

    /// Check if DMA is ready for a new transfer
    ///
    /// # Returns
    /// `true` if no transfer is in progress, `false` otherwise
    fn is_dma_ready(&self) -> bool;

    /// Present a framebuffer to the display (convenience method)
    ///
    /// This is equivalent to calling `wait_for_dma()` followed by `start_dma_transfer()`.
    ///
    /// # Arguments
    /// * `framebuffer` - The framebuffer to display
    ///
    /// # Returns
    /// `Ok(())` if successful, `Err(DisplayError)` otherwise
    fn present(&mut self, framebuffer: &FrameBuf<Rgb565, FB>) -> Result<(), DisplayError> {
        self.wait_for_dma();
        self.start_dma_transfer(framebuffer)
    }

    /// Present a framebuffer sub-region to the display.
    ///
    /// Default implementation falls back to full-frame present.
    fn present_region(
        &mut self,
        framebuffer: &FrameBuf<Rgb565, FB>,
        region: DisplayRegion,
    ) -> Result<(), DisplayError> {
        self.wait_for_dma();
        self.start_dma_transfer_region(framebuffer, region)
    }
}

/// No-op display backend for simulators and testing
///
/// This backend immediately "completes" all transfers and is always ready.
/// It's useful for:
/// - Desktop simulators that don't have real DMA hardware
/// - Unit testing swap chain logic
/// - Development without target hardware
pub struct SimulatorBackend {
    // No state needed for no-op backend
}

impl SimulatorBackend {
    /// Create a new simulator backend
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SimulatorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl<const W: usize, const H: usize, FB> DisplayBackend<W, H, FB> for SimulatorBackend
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    fn start_dma_transfer(
        &mut self,
        _framebuffer: &FrameBuf<Rgb565, FB>,
    ) -> Result<(), DisplayError> {
        // No-op: simulator doesn't actually transfer data
        Ok(())
    }

    fn wait_for_dma(&mut self) {
        // No-op: no real DMA to wait for
    }

    fn is_dma_ready(&self) -> bool {
        // Always ready since there's no real DMA
        true
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use core::cell::Cell;
    use embedded_graphics_core::pixelcolor::RgbColor;
    use embedded_graphics_framebuf::backends::EndianCorrectedBuffer;
    use std::vec;

    // Type alias for testing
    type TestBackend = EndianCorrectedBuffer<'static, Rgb565>;

    struct RegionTrackingBackend {
        region_calls: Cell<usize>,
    }

    impl RegionTrackingBackend {
        fn new() -> Self {
            Self {
                region_calls: Cell::new(0),
            }
        }
    }

    impl<const W: usize, const H: usize, FB> DisplayBackend<W, H, FB> for RegionTrackingBackend
    where
        FB: DMACapableFrameBufferBackend<Color = Rgb565>,
    {
        fn start_dma_transfer(
            &mut self,
            _framebuffer: &FrameBuf<Rgb565, FB>,
        ) -> Result<(), DisplayError> {
            Ok(())
        }

        fn start_dma_transfer_region(
            &mut self,
            _framebuffer: &FrameBuf<Rgb565, FB>,
            _region: DisplayRegion,
        ) -> Result<(), DisplayError> {
            self.region_calls.set(self.region_calls.get() + 1);
            Ok(())
        }

        fn wait_for_dma(&mut self) {}

        fn is_dma_ready(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_simulator_backend_creation() {
        let backend = SimulatorBackend::new();
        // Backend should exist and be ready
        // Use explicit trait method call with types
        assert!(<SimulatorBackend as DisplayBackend<
            320,
            240,
            TestBackend,
        >>::is_dma_ready(&backend));
    }

    #[test]
    fn test_simulator_backend_always_ready() {
        let mut backend = SimulatorBackend::new();

        // Should always be ready
        assert!(<SimulatorBackend as DisplayBackend<
            320,
            240,
            TestBackend,
        >>::is_dma_ready(&backend));

        // Wait should be no-op
        <SimulatorBackend as DisplayBackend<320, 240, TestBackend>>::wait_for_dma(&mut backend);
        assert!(<SimulatorBackend as DisplayBackend<
            320,
            240,
            TestBackend,
        >>::is_dma_ready(&backend));
    }

    #[test]
    fn test_present_region_calls_region_transfer() {
        let mut backend = RegionTrackingBackend::new();
        let data = vec![Rgb565::BLACK; 4].leak();
        let fb = FrameBuf::new(
            EndianCorrectedBuffer::new(
                data,
                embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
            ),
            2,
            2,
        );
        let region = DisplayRegion::new(0, 0, 1, 1);
        <RegionTrackingBackend as DisplayBackend<2, 2, TestBackend>>::present_region(
            &mut backend,
            &fb,
            region,
        )
        .unwrap();
        assert_eq!(backend.region_calls.get(), 1);
    }
}
