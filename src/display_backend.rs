//! Display backend abstraction for DMA-based rendering
//!
//! This module provides a platform-agnostic interface for asynchronous
//! framebuffer transfers using DMA (Direct Memory Access). This enables
//! double-buffered rendering where the CPU can render to one buffer while
//! the display hardware transfers another buffer.
//!
//! # Safety
//!
//! The key safety property of this API is that `start_dma_transfer` takes
//! ownership of the framebuffer and returns a [`DmaTransfer`] token. The
//! buffer is locked inside the token for the duration of the transfer —
//! the compiler prevents any access to it until [`DmaTransfer::wait`]
//! returns it. This eliminates the data race that arises from the
//! previous borrow-based API, where DMA could be reading from memory that
//! the CPU was free to overwrite.

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

/// Trait for offloading 2D/3D fill, blit, and clear operations to silicon acceleration units (e.g. DMA2D / Chrom-ART).
pub trait HardwareAccelerator {
    /// Accelerated fill of a rectangular area with a solid color.
    fn fill_rect(&mut self, x: u16, y: u16, w: u16, h: u16, color: Rgb565) -> bool;

    /// Accelerated memory copy / blit from source to destination buffer.
    fn blit(
        &mut self,
        src: &[Rgb565],
        src_stride: usize,
        dst_x: u16,
        dst_y: u16,
        w: u16,
        h: u16,
    ) -> bool;
}

/// Default CPU-fallback implementation of `HardwareAccelerator`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuAccelerator;

impl HardwareAccelerator for CpuAccelerator {
    #[inline(always)]
    fn fill_rect(&mut self, _x: u16, _y: u16, _w: u16, _h: u16, _color: Rgb565) -> bool {
        false // Fallback to software rasterizer
    }

    #[inline(always)]
    fn blit(
        &mut self,
        _src: &[Rgb565],
        _src_stride: usize,
        _dst_x: u16,
        _dst_y: u16,
        _w: u16,
        _h: u16,
    ) -> bool {
        false // Fallback to software rasterizer
    }
}

/// Error types for display backend operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    /// DMA transfer is still in progress.
    Busy,
    /// Hardware error during transfer.
    HardwareError,
    /// Invalid buffer configuration.
    InvalidBuffer,
}

/// Returned when a DMA transfer fails to start.
///
/// Carries both the error code and the framebuffer back to the caller so
/// the buffer is not lost on failure.
pub struct TransferError<FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    /// The framebuffer that could not be transferred.
    pub framebuffer: FrameBuf<Rgb565, FB>,
    /// The reason the transfer failed.
    pub error: DisplayError,
}

impl<FB> core::fmt::Debug for TransferError<FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TransferError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// An in-progress DMA transfer that holds the framebuffer until completion.
///
/// The buffer is inaccessible while this token is live — the only way to
/// get it back is to call [`wait`](DmaTransfer::wait), which blocks until
/// the hardware has finished reading.
///
/// Implementors for real hardware should cancel the DMA in their [`Drop`]
/// impl so that dropping a token without waiting is always safe.
pub trait DmaTransfer {
    /// The buffer type that is returned when the transfer completes.
    type Buffer;

    /// Returns `true` if the DMA hardware has finished the transfer.
    fn is_done(&self) -> bool;

    /// Block until the transfer is complete and return the framebuffer.
    ///
    /// Consuming `self` ensures the buffer cannot be accessed while DMA
    /// is still reading it.
    fn wait(self) -> Self::Buffer;
}

/// An asynchronous DMA transfer that can be awaited as a future.
///
/// This allows integration with async executors (e.g. Embassy, RTOS task executors)
/// without blocking the CPU while the DMA transfer is in progress.
pub trait AsyncDmaTransfer: DmaTransfer {
    /// The future type returned by `wait_async`.
    type WaitFuture: core::future::Future<Output = Self::Buffer>;

    /// Return a future that resolves to the framebuffer once the DMA transfer completes.
    fn wait_async(self) -> Self::WaitFuture;
}

/// Platform-agnostic display backend trait.
///
/// Implementations handle the hardware-specific details of transferring a
/// framebuffer to the display. The API is intentionally ownership-based:
/// `start_dma_transfer` takes the framebuffer **by value** and returns a
/// [`DmaTransfer`] token. The buffer is held inside the token and cannot
/// be accessed again until [`DmaTransfer::wait`] returns it. This
/// prevents write-after-submit data races at compile time.
///
/// # Implementing for real hardware
///
/// 1. Define a concrete transfer type that holds the buffer and any
///    hardware state needed to poll or cancel the DMA.
/// 2. In `start_dma_transfer`: program the DMA controller, then move the
///    buffer into your transfer type.
/// 3. In `DmaTransfer::wait`: spin or sleep until the done flag is set by
///    the DMA interrupt, then return the buffer.
/// 4. In `DmaTransfer::drop`: if the transfer is still running, cancel
///    it. This ensures that dropping a forgotten token never leaves the
///    DMA controller pointing at freed memory.
pub trait DisplayBackend<const W: usize, const H: usize, FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    /// The transfer token type returned by this backend.
    type Transfer: DmaTransfer<Buffer = FrameBuf<Rgb565, FB>>;

    /// Start a non-blocking DMA transfer of the full framebuffer.
    ///
    /// Takes ownership of the framebuffer. The caller cannot access the
    /// buffer again until [`DmaTransfer::wait`] returns it.
    ///
    /// If starting the transfer fails the buffer is returned inside
    /// [`TransferError`] so it is not lost.
    fn start_dma_transfer(
        &mut self,
        framebuffer: FrameBuf<Rgb565, FB>,
    ) -> Result<Self::Transfer, TransferError<FB>>;

    /// Start a non-blocking DMA transfer of a framebuffer sub-region.
    ///
    /// Backends that do not support partial transfers may ignore `region`
    /// and fall back to a full-frame transfer.
    fn start_dma_transfer_region(
        &mut self,
        framebuffer: FrameBuf<Rgb565, FB>,
        _region: DisplayRegion,
    ) -> Result<Self::Transfer, TransferError<FB>> {
        self.start_dma_transfer(framebuffer)
    }

    /// Optional: Clear the depth buffer using hardware acceleration (e.g. DMA2D / Chrom-ART).
    /// Returns `true` if the hardware successfully cleared the depth buffer,
    /// or `false` to let the engine fall back to CPU slice filling.
    #[cfg(feature = "dma2d")]
    fn hardware_clear_depth(&mut self, _zbuffer: &mut [crate::ZDepth]) -> bool {
        false
    }
}

// ── SimulatorBackend ──────────────────────────────────────────────────────────

/// A transfer token that is already complete.
///
/// Used by [`SimulatorBackend`]: since there is no real DMA hardware, the
/// framebuffer is simply held here until `wait` returns it.
pub struct CompletedTransfer<FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    framebuffer: Option<FrameBuf<Rgb565, FB>>,
}

impl<FB> DmaTransfer for CompletedTransfer<FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    type Buffer = FrameBuf<Rgb565, FB>;

    fn is_done(&self) -> bool {
        true
    }

    fn wait(mut self) -> FrameBuf<Rgb565, FB> {
        self.framebuffer
            .take()
            .expect("CompletedTransfer polled after completion")
    }
}

impl<FB> core::future::Future for CompletedTransfer<FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    type Output = FrameBuf<Rgb565, FB>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let buf = unsafe { self.get_unchecked_mut() }
            .framebuffer
            .take()
            .expect("CompletedTransfer polled after completion");
        core::task::Poll::Ready(buf)
    }
}

impl<FB> AsyncDmaTransfer for CompletedTransfer<FB>
where
    FB: DMACapableFrameBufferBackend<Color = Rgb565>,
{
    type WaitFuture = Self;

    fn wait_async(self) -> Self::WaitFuture {
        self
    }
}

/// No-op display backend for simulators and testing.
///
/// All transfers complete immediately — there is no real DMA hardware.
/// Useful for:
/// - Desktop simulators
/// - Unit testing swap chain logic
/// - Development without target hardware
pub struct SimulatorBackend;

impl SimulatorBackend {
    pub fn new() -> Self {
        Self
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
    type Transfer = CompletedTransfer<FB>;

    fn start_dma_transfer(
        &mut self,
        framebuffer: FrameBuf<Rgb565, FB>,
    ) -> Result<CompletedTransfer<FB>, TransferError<FB>> {
        Ok(CompletedTransfer {
            framebuffer: Some(framebuffer),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use core::cell::Cell;
    use embedded_graphics_core::pixelcolor::RgbColor;
    use embedded_graphics_framebuf::backends::EndianCorrectedBuffer;
    use std::vec;

    type TestBackend = EndianCorrectedBuffer<'static, Rgb565>;

    fn make_fb<const W: usize, const H: usize>() -> FrameBuf<Rgb565, TestBackend> {
        use embedded_graphics_framebuf::backends::EndianCorrection;
        let data: &'static mut [Rgb565] = vec![Rgb565::BLACK; W * H].leak();
        FrameBuf::new(
            EndianCorrectedBuffer::new(data, EndianCorrection::ToLittleEndian),
            W,
            H,
        )
    }

    // ── RegionTrackingBackend ─────────────────────────────────────────────────

    struct RegionTransfer<FB: DMACapableFrameBufferBackend<Color = Rgb565>> {
        framebuffer: Option<FrameBuf<Rgb565, FB>>,
    }

    impl<FB: DMACapableFrameBufferBackend<Color = Rgb565>> DmaTransfer for RegionTransfer<FB> {
        type Buffer = FrameBuf<Rgb565, FB>;
        fn is_done(&self) -> bool {
            true
        }
        fn wait(mut self) -> FrameBuf<Rgb565, FB> {
            self.framebuffer.take().unwrap()
        }
    }

    impl<FB: DMACapableFrameBufferBackend<Color = Rgb565>> core::future::Future for RegionTransfer<FB> {
        type Output = FrameBuf<Rgb565, FB>;
        fn poll(
            self: core::pin::Pin<&mut Self>,
            _cx: &mut core::task::Context<'_>,
        ) -> core::task::Poll<Self::Output> {
            core::task::Poll::Ready(
                unsafe { self.get_unchecked_mut() }
                    .framebuffer
                    .take()
                    .unwrap(),
            )
        }
    }

    impl<FB: DMACapableFrameBufferBackend<Color = Rgb565>> AsyncDmaTransfer for RegionTransfer<FB> {
        type WaitFuture = Self;
        fn wait_async(self) -> Self::WaitFuture {
            self
        }
    }

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
        type Transfer = RegionTransfer<FB>;

        fn start_dma_transfer(
            &mut self,
            framebuffer: FrameBuf<Rgb565, FB>,
        ) -> Result<RegionTransfer<FB>, TransferError<FB>> {
            Ok(RegionTransfer {
                framebuffer: Some(framebuffer),
            })
        }

        fn start_dma_transfer_region(
            &mut self,
            framebuffer: FrameBuf<Rgb565, FB>,
            _region: DisplayRegion,
        ) -> Result<RegionTransfer<FB>, TransferError<FB>> {
            self.region_calls.set(self.region_calls.get() + 1);
            Ok(RegionTransfer {
                framebuffer: Some(framebuffer),
            })
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_simulator_backend_transfer_completes_immediately() {
        let mut backend = SimulatorBackend::new();
        let fb = make_fb::<2, 2>();
        let xfer = <SimulatorBackend as DisplayBackend<2, 2, TestBackend>>::start_dma_transfer(
            &mut backend,
            fb,
        )
        .unwrap();
        assert!(xfer.is_done());
        let _fb = xfer.wait();
    }

    #[test]
    fn test_simulator_backend_returns_buffer_on_wait() {
        let mut backend = SimulatorBackend::new();
        let fb = make_fb::<4, 4>();
        let xfer = <SimulatorBackend as DisplayBackend<4, 4, TestBackend>>::start_dma_transfer(
            &mut backend,
            fb,
        )
        .unwrap();
        // wait() should return the framebuffer
        let fb_back = xfer.wait();
        assert_eq!(fb_back.width(), 4);
        assert_eq!(fb_back.height(), 4);
    }

    #[test]
    fn test_region_tracking_backend_counts_region_transfers() {
        let mut backend = RegionTrackingBackend::new();
        let fb = make_fb::<2, 2>();
        let region = DisplayRegion::new(0, 0, 1, 1);
        let xfer = <RegionTrackingBackend as DisplayBackend<2, 2, TestBackend>>::start_dma_transfer_region(
            &mut backend,
            fb,
            region,
        )
        .unwrap();
        assert_eq!(backend.region_calls.get(), 1);
        let _ = xfer.wait();
    }

    #[test]
    fn test_full_transfer_does_not_increment_region_counter() {
        let mut backend = RegionTrackingBackend::new();
        let fb = make_fb::<2, 2>();
        let xfer =
            <RegionTrackingBackend as DisplayBackend<2, 2, TestBackend>>::start_dma_transfer(
                &mut backend,
                fb,
            )
            .unwrap();
        assert_eq!(backend.region_calls.get(), 0);
        let _ = xfer.wait();
    }

    #[test]
    fn test_cpu_accelerator_and_region_helpers() {
        let mut accel = CpuAccelerator::default();
        assert!(!accel.fill_rect(0, 0, 1, 1, Rgb565::BLACK));
        assert!(!accel.blit(&[], 0, 0, 0, 0, 0));
        let region = DisplayRegion::new(1, 2, 3, 4);
        assert_eq!(region.x, 1);
        assert_eq!(region.y, 2);
        assert_eq!(region.width, 3);
        assert_eq!(region.height, 4);
        assert_eq!(DisplayRegion::new(1, 2, 3, 4), region);
        assert_eq!(DisplayError::Busy, DisplayError::Busy);
    }
}
