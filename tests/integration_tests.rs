//! Integration tests for embedded-3dgfx
//! These tests verify the full rendering pipeline works correctly

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::error::{BudgetKind, RenderError};
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::*;
use nalgebra::{Point3, Vector3};

// Mock framebuffer for testing
struct TestFramebuffer {
    pixels: Vec<(i32, i32, Rgb565)>,
    width: u32,
    height: u32,
}

impl TestFramebuffer {
    fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: Vec::new(),
            width,
            height,
        }
    }

    fn pixel_count(&self) -> usize {
        self.pixels.len()
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        self.pixels.clear();
    }
}

impl DrawTarget for TestFramebuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            self.pixels.push((pixel.0.x, pixel.0.y, pixel.1));
        }
        Ok(())
    }
}

impl OriginDimensions for TestFramebuffer {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

#[test]
fn test_full_rendering_pipeline_points() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [1.0, 0.0, -5.0], [0.0, 1.0, -5.0]];

    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);
    mesh.set_color(Rgb565::CSS_RED);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Should have rendered some points
    assert!(fb.pixel_count() > 0);
}

#[test]
fn test_full_rendering_pipeline_lines() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

    let faces = [[0, 1, 2]];

    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Lines);
    mesh.set_color(Rgb565::CSS_GREEN);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Should have rendered line segments
    assert!(fb.pixel_count() > 10);
}

#[test]
fn test_full_rendering_pipeline_solid() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

    let faces = [[0, 1, 2]];

    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Solid);
    mesh.set_color(Rgb565::CSS_BLUE);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Should have rendered filled triangle
    assert!(fb.pixel_count() > 50);
}

#[test]
fn test_multiple_meshes() {
    let engine = K3dengine::new(640, 480);

    let vertices1 = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0]];
    let geometry1 = Geometry {
        vertices: &vertices1,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh1 = K3dMesh::new(geometry1);
    mesh1.set_render_mode(RenderMode::Points);

    let vertices2 = [[0.0, 0.5, -5.0], [0.5, 0.5, -5.0]];
    let geometry2 = Geometry {
        vertices: &vertices2,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh2 = K3dMesh::new(geometry2);
    mesh2.set_render_mode(RenderMode::Points);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render([&mesh1, &mesh2].iter().copied(), |prim| {
        draw(prim, &mut fb);
    });

    // Should have rendered points from both meshes
    assert!(fb.pixel_count() >= 4);
}

#[test]
fn test_mesh_transformation() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);
    mesh.set_position(1.0, 0.0, 0.0);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Transformed mesh should still render
    assert!(fb.pixel_count() > 0);
}

#[test]
fn test_mesh_scaling() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.1, 0.0, -5.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);
    mesh.set_scale(2.0);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Scaled mesh should still render
    assert!(fb.pixel_count() > 0);
}

#[test]
fn test_backface_culling() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

    let faces = [[0, 1, 2]];

    // Normal pointing away from camera (back face)
    let normals = [[0.0, 0.0, 1.0]];

    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Solid);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Back faces should be culled, so very few or no pixels
    // (depends on culling implementation)
    let pixel_count = fb.pixel_count();
    println!("Backface culling pixel count: {}", pixel_count);
}

#[test]
fn test_camera_movement() {
    let mut engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);

    // Move camera to look at the point
    engine.camera.set_position(Point3::new(1.0, 0.0, 0.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, -5.0));

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Point may or may not be visible depending on camera frustum
    // This test just verifies rendering doesn't panic with camera movement
}

#[test]
fn test_out_of_view_culling() {
    let engine = K3dengine::new(640, 480);

    // Vertices way outside the view frustum
    let vertices = [[100.0, 100.0, -5.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Out of view vertices should not render
    assert_eq!(fb.pixel_count(), 0);
}

#[test]
fn test_lighting_mode() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

    let faces = [[0, 1, 2]];
    let normals = [[0.0, 0.0, -1.0]]; // Normal pointing toward camera

    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    let light_dir = Vector3::new(0.0, 0.0, -1.0);
    mesh.set_render_mode(RenderMode::SolidLightDir(light_dir));
    mesh.set_color(Rgb565::CSS_WHITE);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Lighting mode should work (may or may not render depending on culling)
    // This test just verifies the lighting mode doesn't panic
    let _ = fb.pixel_count();
}

#[test]
fn test_colored_vertices() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0]];
    let colors = [Rgb565::CSS_RED, Rgb565::CSS_BLUE];

    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Should render colored points
    assert!(fb.pixel_count() >= 2);
}

#[test]
fn test_lines_from_explicit_edges() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

    let lines = [[0, 1], [1, 2]];

    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &lines,
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Lines);

    let mut fb = TestFramebuffer::new(640, 480);

    engine.render(std::iter::once(&mesh), |prim| {
        draw(prim, &mut fb);
    });

    // Should render explicit line segments
    assert!(fb.pixel_count() > 10);
}

#[test]
fn test_record_render_commands_overflow_returns_out_of_budget() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);

    // Needs at least room for ClearDepth + one Draw.
    let mut cmd = CommandBuffer::<1>::new();
    let err = engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap_err();

    assert!(matches!(
        err,
        RenderError::OutOfBudget(BudgetKind::DrawPrimitives { .. })
    ));
}

#[test]
fn test_render_frame_rejects_invalid_zbuffer_len() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 8];

    let err = engine
        .render_frame::<_, _, 64>(
            std::iter::once(&mesh),
            &mut fb,
            &mut zbuffer,
            640,
            480,
        )
        .unwrap_err();

    assert!(matches!(
        err,
        RenderError::OutOfBudget(BudgetKind::ZBufferLength { .. })
    ));
}

#[test]
fn test_record_render_commands_is_deterministic() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0]];
    let colors = [Rgb565::CSS_RED, Rgb565::CSS_BLUE];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &[],
        colors: &colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Points);

    let mut cmd_a = CommandBuffer::<64>::new();
    let mut cmd_b = CommandBuffer::<64>::new();

    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd_a)
        .unwrap();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd_b)
        .unwrap();

    assert_eq!(cmd_a.len(), cmd_b.len());

    let snapshot_a: Vec<String> = cmd_a.iter().map(|c| format!("{:?}", c)).collect();
    let snapshot_b: Vec<String> = cmd_b.iter().map(|c| format!("{:?}", c)).collect();
    assert_eq!(snapshot_a, snapshot_b);
}

#[test]
fn test_legacy_render_count_matches_recorded_draw_count() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];
    let faces = [[0, 1, 2]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Lines);

    let mut legacy_count = 0usize;
    engine.render(std::iter::once(&mesh), |_| {
        legacy_count += 1;
    });

    let mut cmd = CommandBuffer::<128>::new();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap();

    let recorded_draw_count = cmd
        .iter()
        .filter(|c| matches!(c, embedded_3dgfx::command_buffer::RenderCommand::Draw(_)))
        .count();

    assert_eq!(legacy_count, recorded_draw_count);
}
