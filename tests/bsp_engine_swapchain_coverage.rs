use embedded_3dgfx::ZDepth;
use embedded_3dgfx::bridge::*;
use embedded_3dgfx::bsp::coverage::*;
use embedded_3dgfx::bsp::scratch::*;
use embedded_3dgfx::bsp::*;
use embedded_3dgfx::command_buffer::*;
use embedded_3dgfx::display_backend::*;
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::primitive::DrawPrimitive;
use embedded_3dgfx::renderer::*;
use embedded_3dgfx::swapchain::*;
use embedded_3dgfx::texture::*;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_framebuf::{FrameBuf, backends::EndianCorrectedBuffer};
use nalgebra::Point2;

fn make_static_slice<T: Copy + Default>(n: usize) -> &'static mut [T] {
    vec![T::default(); n].leak()
}

#[test]
fn test_bsp_recording_direct_and_coverage() {
    let world = test_level::world();

    let mut engine = K3dengine::new(320, 240);
    engine
        .camera
        .set_position(nalgebra::Point3::new(-2.0, 0.0, 0.0));
    engine
        .camera
        .set_target(nalgebra::Point3::new(0.0, 0.0, 0.0));

    let mut visframe = [0u32; 4];
    let mut scratch = BspScratch::new(&mut visframe);
    let mut cmds = CommandBuffer::<512>::new();
    let mut tel = BspTelemetry::default();

    let res = engine.record_bsp(&world, &mut scratch, &mut cmds, Some(&mut tel));
    assert!(res.is_ok());
    assert!(tel.faces_visible > 0);

    let mut tex_mgr = TextureManager::<4>::new();
    let tex_pixels = make_static_slice::<Rgb565>(4);
    let tex = Texture::new(tex_pixels, 2, 2);
    let _ = tex_mgr.add_texture(tex);

    let data = make_static_slice::<Rgb565>(320 * 240);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let mut fb = FrameBuf::new(backend, 320, 240);
    let mut zbuf = vec![ZDepth::MAX; 320 * 240];
    let mut frame = FrameCtx {
        zbuffer: &mut zbuf,
        width: 320,
        height: 240,
    };

    let direct_res = engine.render_bsp_direct(
        &world,
        &mut scratch,
        &tex_mgr,
        &mut fb,
        &mut frame,
        Some(&mut tel),
    );
    assert!(direct_res.is_ok());

    let exec_res = engine.execute_bsp_textured(&mut fb, &mut frame, &cmds, &tex_mgr, None);
    assert!(exec_res.is_ok());

    let mut cov_data = vec![0u8; 320 * 240];
    let mut cov = CoverageBuffer::new(&mut cov_data, 320, 240).unwrap();
    let cov_res = engine.render_bsp_coverage(
        &world,
        &mut scratch,
        &tex_mgr,
        &mut fb,
        &mut cov,
        Some(&mut tel),
    );
    assert!(cov_res.is_ok());
}

#[test]
fn test_bridge_conversions_and_draw_to() {
    use embedded_graphics::geometry::Point;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::{Primitive, PrimitiveStyle, Rectangle};

    let eg_pt = Point::new(12, 34);
    let na_pt = eg_pt.as_nalgebra();
    assert_eq!(na_pt.x, 12);
    assert_eq!(na_pt.y, 34);

    let eg_pt_back = na_pt.as_eg_point();
    assert_eq!(eg_pt_back, eg_pt);

    assert_eq!(nalgebra_to_eg(na_pt), eg_pt);
    assert_eq!(eg_to_nalgebra(eg_pt), na_pt);

    let data = make_static_slice::<Rgb565>(16 * 16);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let mut fb = FrameBuf::new(backend, 16, 16);

    let prim = DrawPrimitive::ColoredPoint(Point2::new(5, 5), Rgb565::RED);
    draw_to(prim, &mut fb);

    let mut tex_buffer = vec![Rgb565::BLACK; 8 * 8];
    let rect = Rectangle::new(Point::new(0, 0), Size::new(8, 8))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN));

    let _ = render_drawable_to_buffer(&rect, &mut tex_buffer, 8, 8);
    assert_eq!(tex_buffer[0], Rgb565::GREEN);
}

#[test]
fn test_swapchain_creation() {
    let buf1 = make_static_slice::<Rgb565>(16 * 16);
    let buf2 = make_static_slice::<Rgb565>(16 * 16);

    let backend = SimulatorBackend::new();
    let swapchain = SwapChain::<16, 16, _, _>::from_static_slices(buf1, buf2, false, backend);
    assert_eq!(swapchain.frame_count(), 0);
}
