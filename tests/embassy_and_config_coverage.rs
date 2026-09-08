use core::future::Future;
use core::task::{Context, Poll, Waker};
use embedded_3dgfx::config::*;
use embedded_3dgfx::display_backend::{AsyncDmaTransfer, DmaTransfer};
use embedded_3dgfx::embassy::*;
use embedded_3dgfx::simplex_stroke_font::{self, SIMPLEX_STROKE_FONT};
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_framebuf::{FrameBuf, backends::EndianCorrectedBuffer};
use std::pin::Pin;
use std::task::RawWaker;
use std::task::RawWakerVTable;

fn make_static_slice<T: Copy + Default>(n: usize) -> &'static mut [T] {
    vec![T::default(); n].leak()
}

fn dummy_raw_waker() -> RawWaker {
    fn no_op(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        dummy_raw_waker()
    }
    let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
    RawWaker::new(std::ptr::null(), vtable)
}

fn dummy_waker() -> Waker {
    unsafe { Waker::from_raw(dummy_raw_waker()) }
}

#[test]
fn test_embassy_wait_transfer_sync() {
    let data = make_static_slice::<Rgb565>(256);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let fb = FrameBuf::new(backend, 16, 16);

    let mut transfer = EmbassyWaitTransfer::new(fb);
    assert!(!transfer.is_done());
    transfer.signal_complete();
    assert!(transfer.is_done());

    let reclaimed_fb = transfer.wait();
    assert_eq!(reclaimed_fb.width(), 16);
}

#[test]
fn test_embassy_wait_transfer_async() {
    let data1 = make_static_slice::<Rgb565>(256);
    let backend1 = EndianCorrectedBuffer::new(
        data1,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let fb1 = FrameBuf::new(backend1, 16, 16);

    let transfer = EmbassyWaitTransfer::new(fb1);
    let waker = dummy_waker();
    let mut cx = Context::from_waker(&waker);

    let mut fut = transfer.wait_async();
    let mut pin_fut = Pin::new(&mut fut);

    assert!(matches!(pin_fut.as_mut().poll(&mut cx), Poll::Pending));

    let data2 = make_static_slice::<Rgb565>(256);
    let backend2 = EndianCorrectedBuffer::new(
        data2,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let fb2 = FrameBuf::new(backend2, 16, 16);

    let mut transfer2 = EmbassyWaitTransfer::new(fb2);
    transfer2.signal_complete();

    let mut fut2 = transfer2.wait_async();
    let mut pin_fut2 = Pin::new(&mut fut2);
    assert!(matches!(pin_fut2.as_mut().poll(&mut cx), Poll::Ready(_)));
}

#[test]
fn test_frame_clock() {
    let mut clock = FrameClock::new();
    let dt1 = clock.tick_ms();
    assert_eq!(dt1, 0);

    std::thread::sleep(std::time::Duration::from_millis(5));
    let dt2 = clock.tick_ms();
    assert!(dt2 >= 1);

    let default_clock = FrameClock::default();
    let _ = default_clock;
}

#[test]
fn test_config_env_vars() {
    let cases = [
        ("off", None),
        ("none", None),
        ("unbounded", None),
        ("m0", Some(PROFILE_M0_MIN)),
        ("m0_min", Some(PROFILE_M0_MIN)),
        ("m0_balanced", Some(PROFILE_M0_BALANCED)),
        ("m3", Some(PROFILE_M3_BALANCED)),
        ("m3_balanced", Some(PROFILE_M3_BALANCED)),
        ("m4", Some(PROFILE_M4_BALANCED)),
        ("m4_balanced", Some(PROFILE_M4_BALANCED)),
        ("m33", Some(PROFILE_M33_BALANCED)),
        ("m33_balanced", Some(PROFILE_M33_BALANCED)),
        ("m55", Some(PROFILE_M55_PERF)),
        ("m55_perf", Some(PROFILE_M55_PERF)),
        ("unknown_invalid_val", Some(DEFAULT_PROFILE_CAPS)),
    ];

    for (val, expected) in cases {
        unsafe {
            std::env::set_var("EMBEDDED_3DGFX_CAPS", val);
        }
        let caps = default_profile_caps();
        if cfg!(feature = "desktop-unbounded") {
            assert!(caps.is_none());
        } else if let Some(exp) = expected {
            assert_eq!(caps.unwrap().max_draw_primitives, exp.max_draw_primitives);
        } else {
            assert!(caps.is_none());
        }
    }
    unsafe {
        std::env::remove_var("EMBEDDED_3DGFX_CAPS");
    }
}

#[test]
fn test_simplex_stroke_font_out_of_bounds() {
    assert!(SIMPLEX_STROKE_FONT.get_glyph_index('A').is_some());
    assert!(SIMPLEX_STROKE_FONT.get_glyph_index('\u{1f}').is_none());
    assert!(SIMPLEX_STROKE_FONT.get_glyph_index('\u{7f}').is_none());
    assert!(SIMPLEX_STROKE_FONT.get_glyph_index('ñ').is_none());

    assert!(SIMPLEX_STROKE_FONT.get_glyph('A').is_some());
    assert!(SIMPLEX_STROKE_FONT.get_glyph('ñ').is_none());

    assert!(SIMPLEX_STROKE_FONT.get_glyph_advance('A').is_some());
    assert!(SIMPLEX_STROKE_FONT.get_glyph_advance('ñ').is_none());

    assert_eq!(simplex_stroke_font::TAB_WIDTH_IN_SPACES, 4);
}
