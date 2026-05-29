//! Swap chain implementation for double-buffered rendering
//!
//! A swap chain manages two framebuffers (front and back) and coordinates
//! DMA transfers to eliminate visual tearing and improve performance.
//!
//! # Architecture
//! - **Back buffer**: CPU renders to this buffer
//! - **Front buffer**: Display hardware reads from this buffer (via DMA)
//! - **Swap**: When rendering completes, buffers are swapped atomically
//!
//! # Benefits
//! - No visual tearing (never display a partially-rendered frame)
//! - Better performance (CPU and display work in parallel)
//! - Predictable frame timing

use crate::display_backend::{DisplayBackend, DisplayError, DisplayRegion};
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_framebuf::{
    FrameBuf,
    backends::{DMACapableFrameBufferBackend, EndianCorrectedBuffer, EndianCorrection},
};

/// Double-buffered swap chain for tear-free rendering
///
/// Manages two framebuffers and coordinates swapping between them.
/// Uses a display backend for platform-specific DMA transfers.
///
/// # Type Parameters
/// - `W`: Framebuffer width in pixels
/// - `H`: Framebuffer height in pixels
/// - `FB`: Framebuffer backend implementing `DMACapableFrameBufferBackend`
/// - `B`: Display backend implementing `DisplayBackend<W, H, FB>`
pub struct SwapChain<const W: usize, const H: usize, FB, B>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
    B: DisplayBackend<W, H, FB>,
{
    /// Front buffer (currently being displayed)
    front: FrameBuf<Rgb565, FB>,
    /// Back buffer (currently being rendered to)
    back: FrameBuf<Rgb565, FB>,
    /// Display backend for DMA transfers
    backend: B,
    /// Frame counter for statistics
    frame_count: u64,
}

/// Type alias for SwapChain using EndianCorrectedBuffer backend
///
/// This is the most common configuration, using statically allocated
/// framebuffer memory with endianness correction.
pub type StandardSwapChain<const W: usize, const H: usize, B> =
    SwapChain<W, H, EndianCorrectedBuffer<'static, Rgb565>, B>;

/// Constructor for standard swap chain configuration
impl<const W: usize, const H: usize, B> StandardSwapChain<W, H, B>
where
    B: DisplayBackend<W, H, EndianCorrectedBuffer<'static, Rgb565>>,
{
    /// Create a new swap chain from static slices
    ///
    /// # Arguments
    /// * `front_data` - Static mutable slice for front framebuffer
    /// * `back_data` - Static mutable slice for back framebuffer
    /// * `big_endian` - Whether to use big-endian byte order for colors
    /// * `backend` - Display backend for DMA operations
    ///
    /// # Example
    /// ```ignore
    /// static mut FB0_DATA: [Rgb565; 800 * 600] = [Rgb565::BLACK; 800 * 600];
    /// static mut FB1_DATA: [Rgb565; 800 * 600] = [Rgb565::BLACK; 800 * 600];
    ///
    /// let swap_chain = unsafe {
    ///     StandardSwapChain::<800, 600, _>::from_static_slices(
    ///         &mut FB0_DATA,
    ///         &mut FB1_DATA,
    ///         false,
    ///         SimulatorBackend::new(),
    ///     )
    /// };
    /// ```
    pub fn from_static_slices(
        front_data: &'static mut [Rgb565],
        back_data: &'static mut [Rgb565],
        big_endian: bool,
        backend: B,
    ) -> Self {
        let front_backend = if big_endian {
            EndianCorrectedBuffer::new(front_data, EndianCorrection::ToBigEndian)
        } else {
            EndianCorrectedBuffer::new(front_data, EndianCorrection::ToLittleEndian)
        };

        let back_backend = if big_endian {
            EndianCorrectedBuffer::new(back_data, EndianCorrection::ToBigEndian)
        } else {
            EndianCorrectedBuffer::new(back_data, EndianCorrection::ToLittleEndian)
        };

        Self {
            front: FrameBuf::new(front_backend, W, H),
            back: FrameBuf::new(back_backend, W, H),
            backend,
            frame_count: 0,
        }
    }
}

impl<const W: usize, const H: usize, FB, B> SwapChain<W, H, FB, B>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
    B: DisplayBackend<W, H, FB>,
{
    /// Get mutable reference to the back buffer for rendering
    ///
    /// Render all graphics operations to this buffer. When ready to display,
    /// call `present()` to swap buffers.
    pub fn get_back_buffer(&mut self) -> &mut FrameBuf<Rgb565, FB> {
        &mut self.back
    }

    /// Get reference to the front buffer (currently being displayed)
    ///
    /// This is rarely needed, as you should render to the back buffer.
    pub fn get_front_buffer(&self) -> &FrameBuf<Rgb565, FB> {
        &self.front
    }

    /// Present the back buffer to the display (blocking)
    ///
    /// This function:
    /// 1. Waits for any in-progress DMA transfer to complete
    /// 2. Swaps the front and back buffers
    /// 3. Starts a new DMA transfer of the (new) front buffer
    ///
    /// After this call returns, you can immediately start rendering to the
    /// (new) back buffer while the display hardware reads the front buffer.
    ///
    /// # Returns
    /// `Ok(())` if successful, `Err(DisplayError)` if DMA fails
    pub fn present(&mut self) -> Result<(), DisplayError> {
        // Wait for any in-progress DMA to finish
        self.backend.wait_for_dma();

        // Swap front and back buffers
        core::mem::swap(&mut self.front, &mut self.back);

        // Start DMA transfer of new front buffer
        self.backend.start_dma_transfer(&self.front)?;

        self.frame_count += 1;
        Ok(())
    }

    /// Try to present the back buffer (non-blocking)
    ///
    /// Like `present()`, but returns immediately with an error if DMA is busy.
    /// Useful for variable-rate rendering where you want to skip frames
    /// if rendering is too slow.
    ///
    /// # Returns
    /// - `Ok(())` if swap succeeded
    /// - `Err(DisplayError::Busy)` if DMA is still transferring
    /// - `Err(DisplayError::HardwareError)` on other errors
    pub fn try_present(&mut self) -> Result<(), DisplayError> {
        // Check if DMA is ready
        if !self.backend.is_dma_ready() {
            return Err(DisplayError::Busy);
        }

        // Swap front and back buffers
        core::mem::swap(&mut self.front, &mut self.back);

        // Start DMA transfer of new front buffer
        self.backend.start_dma_transfer(&self.front)?;

        self.frame_count += 1;
        Ok(())
    }

    /// Present only a region of the back buffer.
    ///
    /// Backends that don't support region DMA automatically fall back to full-frame transfer.
    pub fn present_region(&mut self, region: DisplayRegion) -> Result<(), DisplayError> {
        self.backend.wait_for_dma();
        core::mem::swap(&mut self.front, &mut self.back);
        self.backend
            .start_dma_transfer_region(&self.front, region)?;
        self.frame_count += 1;
        Ok(())
    }

    /// Non-blocking partial present.
    pub fn try_present_region(&mut self, region: DisplayRegion) -> Result<(), DisplayError> {
        if !self.backend.is_dma_ready() {
            return Err(DisplayError::Busy);
        }
        core::mem::swap(&mut self.front, &mut self.back);
        self.backend
            .start_dma_transfer_region(&self.front, region)?;
        self.frame_count += 1;
        Ok(())
    }

    /// Wait for the current DMA transfer to complete
    ///
    /// Useful if you need to ensure a frame has been fully displayed
    /// before proceeding (e.g., before taking a screenshot or exiting).
    pub fn wait_for_vsync(&mut self) {
        self.backend.wait_for_dma();
    }

    /// Check if DMA is ready for a new transfer
    ///
    /// Returns `true` if `try_present()` would succeed, `false` otherwise.
    pub fn is_ready(&self) -> bool {
        self.backend.is_dma_ready()
    }

    /// Get the number of frames presented
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Reset the frame counter
    pub fn reset_frame_count(&mut self) {
        self.frame_count = 0;
    }

    /// Get framebuffer dimensions
    pub fn dimensions(&self) -> (usize, usize) {
        (W, H)
    }
}

/// Triple-buffered swap chain for smoother pacing under bursty frame times.
#[cfg(feature = "triple-buffering")]
pub struct TripleSwapChain<const W: usize, const H: usize, FB, B>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
    B: DisplayBackend<W, H, FB>,
{
    display: FrameBuf<Rgb565, FB>,
    ready: FrameBuf<Rgb565, FB>,
    render: FrameBuf<Rgb565, FB>,
    backend: B,
    frame_count: u64,
}

#[cfg(feature = "triple-buffering")]
pub type StandardTripleSwapChain<const W: usize, const H: usize, B> =
    TripleSwapChain<W, H, EndianCorrectedBuffer<'static, Rgb565>, B>;

#[cfg(feature = "triple-buffering")]
impl<const W: usize, const H: usize, B> StandardTripleSwapChain<W, H, B>
where
    B: DisplayBackend<W, H, EndianCorrectedBuffer<'static, Rgb565>>,
{
    pub fn from_static_slices(
        display_data: &'static mut [Rgb565],
        ready_data: &'static mut [Rgb565],
        render_data: &'static mut [Rgb565],
        big_endian: bool,
        backend: B,
    ) -> Self {
        let mk = |data: &'static mut [Rgb565]| {
            if big_endian {
                EndianCorrectedBuffer::new(data, EndianCorrection::ToBigEndian)
            } else {
                EndianCorrectedBuffer::new(data, EndianCorrection::ToLittleEndian)
            }
        };
        Self {
            display: FrameBuf::new(mk(display_data), W, H),
            ready: FrameBuf::new(mk(ready_data), W, H),
            render: FrameBuf::new(mk(render_data), W, H),
            backend,
            frame_count: 0,
        }
    }
}

#[cfg(feature = "triple-buffering")]
impl<const W: usize, const H: usize, FB, B> TripleSwapChain<W, H, FB, B>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
    B: DisplayBackend<W, H, FB>,
{
    pub fn get_render_buffer(&mut self) -> &mut FrameBuf<Rgb565, FB> {
        &mut self.render
    }

    pub fn present(&mut self) -> Result<(), DisplayError> {
        self.backend.wait_for_dma();
        core::mem::swap(&mut self.display, &mut self.render);
        self.backend.start_dma_transfer(&self.display)?;
        core::mem::swap(&mut self.ready, &mut self.render);
        self.frame_count += 1;
        Ok(())
    }

    pub fn try_present(&mut self) -> Result<(), DisplayError> {
        if !self.backend.is_dma_ready() {
            return Err(DisplayError::Busy);
        }
        core::mem::swap(&mut self.display, &mut self.render);
        self.backend.start_dma_transfer(&self.display)?;
        core::mem::swap(&mut self.ready, &mut self.render);
        self.frame_count += 1;
        Ok(())
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::display_backend::SimulatorBackend;
    use core::cell::Cell;
    use embedded_graphics_core::pixelcolor::RgbColor;
    use std::vec;

    // Helper to create a leaked slice for testing
    fn create_static_buffer<const SIZE: usize>() -> &'static mut [Rgb565] {
        let vec = vec![Rgb565::BLACK; SIZE];
        vec.leak()
    }

    #[test]
    fn test_swapchain_creation() {
        let fb0 = create_static_buffer::<{ 320 * 240 }>();
        let fb1 = create_static_buffer::<{ 320 * 240 }>();

        let swap_chain = StandardSwapChain::<320, 240, _>::from_static_slices(
            fb0,
            fb1,
            false,
            SimulatorBackend::new(),
        );

        assert_eq!(swap_chain.dimensions(), (320, 240));
        assert_eq!(swap_chain.frame_count(), 0);
        assert!(swap_chain.is_ready());
    }

    #[test]
    fn test_swapchain_present() {
        let fb0 = create_static_buffer::<{ 320 * 240 }>();
        let fb1 = create_static_buffer::<{ 320 * 240 }>();

        let mut swap_chain = StandardSwapChain::<320, 240, _>::from_static_slices(
            fb0,
            fb1,
            false,
            SimulatorBackend::new(),
        );

        // Present should succeed
        assert!(swap_chain.present().is_ok());
        assert_eq!(swap_chain.frame_count(), 1);
    }

    #[test]
    fn test_swapchain_multiple_presents() {
        let fb0 = create_static_buffer::<{ 320 * 240 }>();
        let fb1 = create_static_buffer::<{ 320 * 240 }>();

        let mut swap_chain = StandardSwapChain::<320, 240, _>::from_static_slices(
            fb0,
            fb1,
            false,
            SimulatorBackend::new(),
        );

        // Present multiple frames
        for _ in 0..5 {
            assert!(swap_chain.present().is_ok());
        }

        assert_eq!(swap_chain.frame_count(), 5);
    }

    #[test]
    fn test_swapchain_try_present() {
        let fb0 = create_static_buffer::<{ 320 * 240 }>();
        let fb1 = create_static_buffer::<{ 320 * 240 }>();

        let mut swap_chain = StandardSwapChain::<320, 240, _>::from_static_slices(
            fb0,
            fb1,
            false,
            SimulatorBackend::new(),
        );

        // try_present should succeed (SimulatorBackend is always ready)
        assert!(swap_chain.try_present().is_ok());
        assert_eq!(swap_chain.frame_count(), 1);
    }

    #[test]
    fn test_swapchain_frame_counter() {
        let fb0 = create_static_buffer::<{ 320 * 240 }>();
        let fb1 = create_static_buffer::<{ 320 * 240 }>();

        let mut swap_chain = StandardSwapChain::<320, 240, _>::from_static_slices(
            fb0,
            fb1,
            false,
            SimulatorBackend::new(),
        );

        assert_eq!(swap_chain.frame_count(), 0);

        swap_chain.present().unwrap();
        assert_eq!(swap_chain.frame_count(), 1);

        swap_chain.present().unwrap();
        assert_eq!(swap_chain.frame_count(), 2);

        swap_chain.reset_frame_count();
        assert_eq!(swap_chain.frame_count(), 0);
    }

    struct TrackingBackend {
        region_present_count: Cell<usize>,
    }

    impl TrackingBackend {
        fn new() -> Self {
            Self {
                region_present_count: Cell::new(0),
            }
        }
    }

    impl<const W: usize, const H: usize, FB> DisplayBackend<W, H, FB> for TrackingBackend
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
            self.region_present_count
                .set(self.region_present_count.get() + 1);
            Ok(())
        }

        fn wait_for_dma(&mut self) {}

        fn is_dma_ready(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_swapchain_present_region() {
        let fb0 = create_static_buffer::<{ 64 * 64 }>();
        let fb1 = create_static_buffer::<{ 64 * 64 }>();
        let backend = TrackingBackend::new();
        let mut swap_chain =
            StandardSwapChain::<64, 64, _>::from_static_slices(fb0, fb1, false, backend);
        swap_chain
            .present_region(DisplayRegion::new(0, 0, 8, 8))
            .unwrap();
        assert_eq!(swap_chain.backend.region_present_count.get(), 1);
    }

    #[cfg(feature = "triple-buffering")]
    #[test]
    fn test_triple_swapchain_present() {
        let fb0 = create_static_buffer::<{ 64 * 64 }>();
        let fb1 = create_static_buffer::<{ 64 * 64 }>();
        let fb2 = create_static_buffer::<{ 64 * 64 }>();
        let mut sc = StandardTripleSwapChain::<64, 64, _>::from_static_slices(
            fb0,
            fb1,
            fb2,
            false,
            SimulatorBackend::new(),
        );
        assert_eq!(sc.frame_count(), 0);
        sc.present().unwrap();
        assert_eq!(sc.frame_count(), 1);
    }
}
