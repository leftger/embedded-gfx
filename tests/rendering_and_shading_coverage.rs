use embedded_3dgfx::ZDepth;
use embedded_3dgfx::draw::effects::*;
use embedded_3dgfx::draw::textured::*;
use embedded_3dgfx::draw::zbuffered::*;
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::painters::*;
use embedded_3dgfx::primitive::DrawPrimitive;
use embedded_3dgfx::retro::*;
use embedded_3dgfx::shader::depth_darken::DepthDarkenConfig;
use embedded_3dgfx::texture::*;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_framebuf::{FrameBuf, backends::EndianCorrectedBuffer};
use nalgebra::{Point2, Point3};

fn make_static_slice<T: Copy + Default>(n: usize) -> &'static mut [T] {
    vec![T::default(); n].leak()
}

#[test]
fn test_draw_primitives_bounds_and_rendering() {
    let data = make_static_slice::<Rgb565>(64 * 64);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let mut fb = FrameBuf::new(backend, 64, 64);
    let mut zbuf = vec![ZDepth::MAX; 64 * 64];

    let p0 = Point2::new(10, 10);
    let p1 = Point2::new(30, 10);
    let p2 = Point2::new(20, 30);

    let prims = vec![
        DrawPrimitive::ColoredPoint(p0, Rgb565::RED),
        DrawPrimitive::Line([p0, p1], Rgb565::GREEN),
        DrawPrimitive::ColoredTriangle([p0, p1, p2], Rgb565::BLUE),
        DrawPrimitive::ColoredTriangleWithDepth {
            points: [p0, p1, p2],
            depths: [1.0, 2.0, 1.5],
            color: Rgb565::WHITE,
        },
        DrawPrimitive::TranslucentTriangleWithDepth {
            points: [p0, p1, p2],
            depths: [1.0, 2.0, 1.5],
            color: Rgb565::YELLOW,
            alpha: 128,
        },
        DrawPrimitive::ScreenDoorTriangleWithDepth {
            points: [p0, p1, p2],
            depths: [1.0, 2.0, 1.5],
            color: Rgb565::CYAN,
            alpha: 128,
        },
        DrawPrimitive::GouraudTriangle {
            points: [p0, p1, p2],
            colors: [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE],
        },
        DrawPrimitive::GouraudTriangleWithDepth {
            points: [p0, p1, p2],
            depths: [1.0, 2.0, 1.5],
            colors: [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE],
        },
    ];

    for prim in &prims {
        let (min_x, min_y, max_x, max_y) = prim.bounds();
        assert!(min_x <= max_x);
        assert!(min_y <= max_y);
        draw_zbuffered(prim.clone(), &mut fb, &mut zbuf, 64);
    }
}

#[test]
fn test_textured_and_lightmapped_rendering_exhaustive() {
    let data = make_static_slice::<Rgb565>(64 * 64);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let mut fb = FrameBuf::new(backend, 64, 64);
    let mut zbuf = vec![ZDepth::MAX; 64 * 64];

    let mut tex_mgr = TextureManager::<4>::new();
    let tex_pixels = make_static_slice::<Rgb565>(4);
    tex_pixels[0] = Rgb565::RED;
    tex_pixels[1] = Rgb565::GREEN;
    tex_pixels[2] = Rgb565::BLUE;
    tex_pixels[3] = Rgb565::WHITE;

    let tex = Texture::new(tex_pixels, 2, 2);
    let tex_id = tex_mgr.add_texture(tex).unwrap();

    let p0 = Point2::new(5, 5);
    let p1 = Point2::new(25, 5);
    let p2 = Point2::new(15, 25);
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];

    let textured_prims = vec![
        DrawPrimitive::TexturedTriangle {
            points: [p0, p1, p2],
            uvs,
            texture_id: tex_id,
        },
        DrawPrimitive::TexturedTriangleWithDepth {
            points: [p0, p1, p2],
            depths: [1.0, 1.0, 1.0],
            ws: [1.0, 1.0, 1.0],
            uvs,
            texture_id: tex_id,
        },
        DrawPrimitive::TexturedGouraudTriangleWithDepth {
            points: [p0, p1, p2],
            depths: [1.0, 1.0, 1.0],
            ws: [1.0, 1.0, 1.0],
            uvs,
            colors: [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE],
            texture_id: tex_id,
        },
        DrawPrimitive::LightmappedTriangle {
            points: [p0, p1, p2],
            depths: [1.0, 1.0, 1.0],
            ws: [1.0, 1.0, 1.0],
            surface_uvs: uvs,
            lm_uvs: uvs,
            texture_id: tex_id,
            lightmap_id: tex_id,
            brightness: 200,
            dynamic_tint: Rgb565::CYAN,
        },
        DrawPrimitive::LightmappedTriangle {
            points: [p0, p1, p2],
            depths: [1.0, 1.0, 1.0],
            ws: [1.0, 1.0, 1.0],
            surface_uvs: uvs,
            lm_uvs: uvs,
            texture_id: tex_id,
            lightmap_id: u32::MAX,
            brightness: 255,
            dynamic_tint: Rgb565::BLACK,
        },
    ];

    let fog = FogConfig::new(Rgb565::BLUE, 1.0, 100.0);
    let dither = DitherConfig { intensity: 64 };

    let tints = [
        ScreenTint {
            color: Rgb565::RED,
            strength: 128,
        },
        ScreenTint {
            color: Rgb565::GREEN,
            strength: 64,
        },
    ];

    for prim in &textured_prims {
        for mapping in [TextureMapping::Affine, TextureMapping::PerspectiveCorrect] {
            for stipple in [StippleMode::Off, StippleMode::Checkerboard] {
                for palette in [PaletteMode::Off, PaletteMode::Rgb332] {
                    for tint in &tints {
                        draw_zbuffered_with_textures_mapped(
                            prim.clone(),
                            &mut fb,
                            &mut zbuf,
                            64,
                            &tex_mgr,
                            Some(&fog),
                            Some(&dither),
                            mapping,
                            stipple,
                            Some(*tint),
                            palette,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_depth_darken_shader() {
    let config = DepthDarkenConfig::new(255, 1.0, 20.0);
    let color = config.apply(Rgb565::WHITE, 65536 * 10);
    assert_ne!(color, Rgb565::WHITE);
}

#[test]
fn test_painters_algorithm_rendering() {
    let mut engine = K3dengine::new(64, 64);
    engine.camera.set_position(Point3::new(0.0, 0.0, 0.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 5.0));

    let positions = vec![[-1.0f32, -1.0, 5.0], [1.0, -1.0, 5.0], [0.0, 1.0, 5.0]];
    let faces = vec![[0, 1, 2]];

    let geom = Geometry {
        vertices: &positions,
        faces: &faces,
        lines: &[],
        colors: &[],
        uvs: &[],
        normals: &[],
        vertex_normals: &[],
        texture_id: Some(0),
    };
    let mut mesh = K3dMesh::new(geom);
    mesh.render_mode = RenderMode::Solid;

    let mut scratch = vec![DepthSortedTriangle::default(); 16];
    let mut count_rendered = 0;

    let meshes = [mesh];
    let total = engine.render_painters_algorithm(&meshes, &mut scratch, |_prim| {
        count_rendered += 1;
    });

    assert!(total > 0);
    assert_eq!(count_rendered, total);
}
