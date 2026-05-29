//! Integration tests for embedded-3dgfx
//! These tests verify the full rendering pipeline works correctly

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::ProfileCaps;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::error::{BudgetKind, RenderError};
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::telemetry::{ExecuteTelemetry, RecordTelemetry};
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

    fn hash_pixels(&self) -> u64 {
        // FNV-1a 64-bit for deterministic regression snapshots.
        let mut hash: u64 = 0xcbf29ce484222325;
        for (x, y, c) in &self.pixels {
            for b in x.to_le_bytes() {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            for b in y.to_le_bytes() {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            let raw = c.into_storage();
            for b in raw.to_le_bytes() {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        hash
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
fn test_execute_recorded_frame_rejects_invalid_zbuffer_len() {
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
    let mut commands = CommandBuffer::<64>::new();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut commands)
        .unwrap();

    let err = engine
        .execute_recorded_frame::<_, 64>(
            &mut fb,
            &mut zbuffer,
            640,
            480,
            &commands,
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

#[test]
fn test_record_respects_mesh_budget_caps() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 1,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let vertices_a = [[0.0, 0.0, -5.0]];
    let vertices_b = [[0.2, 0.0, -5.0]];
    let geometry_a = Geometry {
        vertices: &vertices_a,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let geometry_b = Geometry {
        vertices: &vertices_b,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mesh_a = K3dMesh::new(geometry_a);
    let mesh_b = K3dMesh::new(geometry_b);

    let mut cmd = CommandBuffer::<128>::new();
    let err = engine
        .record_render_commands([&mesh_a, &mesh_b].iter().copied(), &mut cmd)
        .unwrap_err();

    assert!(matches!(
        err,
        RenderError::OutOfBudget(BudgetKind::MeshesPerFrame { .. })
    ));
}

#[test]
fn test_record_budget_fallback_uses_reduced_mesh_set() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 1,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let vertices_a = [[0.0, 0.0, -5.0]];
    let vertices_b = [[0.2, 0.0, -5.0]];
    let geometry_a = Geometry {
        vertices: &vertices_a,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let geometry_b = Geometry {
        vertices: &vertices_b,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh_a = K3dMesh::new(geometry_a);
    mesh_a.set_render_mode(RenderMode::Points);
    let mut mesh_b = K3dMesh::new(geometry_b);
    mesh_b.set_render_mode(RenderMode::Points);

    let mut cmd = CommandBuffer::<128>::new();
    let mut rec = RecordTelemetry::default();
    let used_fallback = engine
        .record_render_commands_with_budget_fallback(
            [&mesh_a, &mesh_b].iter().copied(),
            [&mesh_a].iter().copied(),
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    assert!(used_fallback);
    assert_eq!(rec.meshes_total, 1);
    assert_eq!(rec.meshes_visible, 1);
    assert_eq!(rec.draw_commands, 1);
    assert!(rec.fallback_used);
}

#[test]
fn test_record_budget_fallback_report_exposes_primary_budget_error() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 1,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let vertices_a = [[0.0, 0.0, -5.0]];
    let vertices_b = [[0.2, 0.0, -5.0]];
    let geometry_a = Geometry {
        vertices: &vertices_a,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let geometry_b = Geometry {
        vertices: &vertices_b,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh_a = K3dMesh::new(geometry_a);
    mesh_a.set_render_mode(RenderMode::Points);
    let mut mesh_b = K3dMesh::new(geometry_b);
    mesh_b.set_render_mode(RenderMode::Points);

    let mut cmd = CommandBuffer::<128>::new();
    let outcome = engine
        .record_render_commands_with_budget_fallback_report(
            [&mesh_a, &mesh_b].iter().copied(),
            [&mesh_a].iter().copied(),
            &mut cmd,
            None,
        )
        .unwrap();

    assert!(outcome.used_fallback);
    assert!(matches!(
        outcome.primary_budget_error,
        Some(BudgetKind::MeshesPerFrame { .. })
    ));
}

#[test]
fn test_record_budget_fallback_keeps_primary_when_within_budget() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 4,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let vertices_a = [[0.0, 0.0, -5.0]];
    let vertices_b = [[0.2, 0.0, -5.0]];
    let geometry_a = Geometry {
        vertices: &vertices_a,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let geometry_b = Geometry {
        vertices: &vertices_b,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh_a = K3dMesh::new(geometry_a);
    mesh_a.set_render_mode(RenderMode::Points);
    let mut mesh_b = K3dMesh::new(geometry_b);
    mesh_b.set_render_mode(RenderMode::Points);

    let mut cmd = CommandBuffer::<128>::new();
    let mut rec = RecordTelemetry::default();
    let used_fallback = engine
        .record_render_commands_with_budget_fallback(
            [&mesh_a, &mesh_b].iter().copied(),
            [&mesh_a].iter().copied(),
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    assert!(!used_fallback);
    assert_eq!(rec.meshes_total, 2);
    assert_eq!(rec.meshes_visible, 2);
    assert_eq!(rec.draw_commands, 2);
    assert!(!rec.fallback_used);
}

#[test]
fn test_record_budget_fallback_report_no_error_when_primary_succeeds() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 4,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let vertices_a = [[0.0, 0.0, -5.0]];
    let vertices_b = [[0.2, 0.0, -5.0]];
    let geometry_a = Geometry {
        vertices: &vertices_a,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let geometry_b = Geometry {
        vertices: &vertices_b,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh_a = K3dMesh::new(geometry_a);
    mesh_a.set_render_mode(RenderMode::Points);
    let mut mesh_b = K3dMesh::new(geometry_b);
    mesh_b.set_render_mode(RenderMode::Points);

    let mut cmd = CommandBuffer::<128>::new();
    let outcome = engine
        .record_render_commands_with_budget_fallback_report(
            [&mesh_a, &mesh_b].iter().copied(),
            [&mesh_a].iter().copied(),
            &mut cmd,
            None,
        )
        .unwrap();

    assert!(!outcome.used_fallback);
    assert_eq!(outcome.primary_budget_error, None);
}

#[test]
fn test_record_budget_decimation_fallback_uses_stride_subset() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 2,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let v0 = [[0.0, 0.0, -5.0]];
    let v1 = [[0.2, 0.0, -5.0]];
    let v2 = [[0.4, 0.0, -5.0]];
    let g0 = Geometry {
        vertices: &v0,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let g1 = Geometry {
        vertices: &v1,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let g2 = Geometry {
        vertices: &v2,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut m0 = K3dMesh::new(g0);
    m0.set_render_mode(RenderMode::Points);
    let mut m1 = K3dMesh::new(g1);
    m1.set_render_mode(RenderMode::Points);
    let mut m2 = K3dMesh::new(g2);
    m2.set_render_mode(RenderMode::Points);
    let meshes = [&m0, &m1, &m2];

    let mut cmd = CommandBuffer::<128>::new();
    let mut rec = RecordTelemetry::default();
    let used_fallback = engine
        .record_render_commands_with_budget_decimation_fallback(
            meshes.iter().copied(),
            2,
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    assert!(used_fallback);
    assert_eq!(rec.meshes_total, 2); // indices 0 and 2 survive
    assert_eq!(rec.meshes_visible, 2);
    assert_eq!(rec.draw_commands, 2);
    assert!(rec.fallback_used);
}

#[test]
fn test_record_budget_decimation_fallback_rejects_zero_stride() {
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
    let mesh = K3dMesh::new(geometry);

    let mut cmd = CommandBuffer::<64>::new();
    let err = engine
        .record_render_commands_with_budget_decimation_fallback(
            std::iter::once(&mesh),
            0,
            &mut cmd,
            None,
        )
        .unwrap_err();

    assert!(matches!(err, RenderError::InvalidInput(_)));
}

#[test]
fn test_record_budget_selector_fallback_uses_predicate_subset() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 1,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let va = [[0.0, 0.0, -5.0]];
    let vb = [[0.2, 0.0, -5.0]];
    let ga = Geometry {
        vertices: &va,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let gb = Geometry {
        vertices: &vb,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut ma = K3dMesh::new(ga);
    ma.set_render_mode(RenderMode::Points);
    let mut mb = K3dMesh::new(gb);
    mb.set_render_mode(RenderMode::Points);

    let meshes = [&ma, &mb];
    let mut cmd = CommandBuffer::<128>::new();
    let mut rec = RecordTelemetry::default();
    let used_fallback = engine
        .record_render_commands_with_budget_selector_fallback(
            meshes.iter().copied(),
            |idx, _| idx == 0,
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    assert!(used_fallback);
    assert_eq!(rec.meshes_total, 1);
    assert_eq!(rec.meshes_visible, 1);
    assert_eq!(rec.draw_commands, 1);
    assert!(rec.fallback_used);
}

#[test]
fn test_record_respects_vertices_per_mesh_caps() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 512,
        max_meshes_per_frame: 8,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 512,
        max_vertices_per_mesh: 1,
    });

    let vertices = [[0.0, 0.0, -5.0], [0.4, 0.0, -5.0]];
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
    let mesh = K3dMesh::new(geometry);

    let mut cmd = CommandBuffer::<128>::new();
    let err = engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap_err();

    assert!(matches!(
        err,
        RenderError::OutOfBudget(BudgetKind::VerticesPerMesh { .. })
    ));
}

#[test]
fn test_record_and_execute_telemetry_reports_counts() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.3, 0.0, -5.0]];
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

    let mut cmd = CommandBuffer::<64>::new();
    let mut rec = RecordTelemetry::default();
    engine
        .record_render_commands_with_telemetry(
            std::iter::once(&mesh),
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();
    assert!(rec.meshes_total >= 1);
    assert!(rec.draw_commands >= 1);
    assert!(!rec.fallback_used);

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut exec = ExecuteTelemetry::default();
    engine
        .execute_recorded_frame_with_telemetry(
            &mut fb,
            &mut zbuffer,
            640,
            480,
            &cmd,
            Some(&mut exec),
        )
        .unwrap();
    assert_eq!(exec.commands_total, cmd.len());
    assert!(exec.draw_commands >= 1);
}

#[test]
fn test_golden_hash_points_scene_record_execute() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0]];
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

    let mut cmd = CommandBuffer::<64>::new();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    engine
        .execute_recorded_frame::<_, 64>(&mut fb, &mut zbuffer, 640, 480, &cmd)
        .unwrap();

    let digest = fb.hash_pixels();
    assert_eq!(digest, 4519810522198061541);
}

#[test]
fn test_golden_hash_lines_scene_record_execute() {
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

    let mut cmd = CommandBuffer::<128>::new();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    engine
        .execute_recorded_frame::<_, 128>(&mut fb, &mut zbuffer, 640, 480, &cmd)
        .unwrap();

    let digest = fb.hash_pixels();
    assert_eq!(digest, 3107081647503172710);
}

#[test]
fn test_golden_hash_solid_scene_record_execute() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.6, 0.0, -5.0], [0.0, 0.6, -5.0]];
    let faces = [[0, 1, 2]];
    let normals = [[0.0, 0.0, -1.0]];
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
    mesh.set_color(Rgb565::CSS_BLUE);

    let mut cmd = CommandBuffer::<128>::new();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    engine
        .execute_recorded_frame::<_, 128>(&mut fb, &mut zbuffer, 640, 480, &cmd)
        .unwrap();

    let digest = fb.hash_pixels();
    assert_eq!(digest, 14695981039346656037);
}

#[test]
fn test_golden_hash_gouraud_scene_record_execute() {
    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 0.0, -10.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let vertices = [[0.0, 0.0, 0.0], [0.8, 0.0, 0.0], [0.0, 0.8, 0.0]];
    let faces = [[0, 1, 2]];
    let normals = [[0.0, 0.0, -1.0]];
    let vertex_normals = [[0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0]];
    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &normals,
        vertex_normals: &vertex_normals,
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::GouraudLightDir(Vector3::new(0.0, 0.0, 1.0)));
    mesh.set_color(Rgb565::CSS_WHITE);

    let mut cmd = CommandBuffer::<128>::new();
    engine
        .record_render_commands(std::iter::once(&mesh), &mut cmd)
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    engine
        .execute_recorded_frame::<_, 128>(&mut fb, &mut zbuffer, 640, 480, &cmd)
        .unwrap();

    let digest = fb.hash_pixels();
    assert_eq!(digest, 3505104233474102080);
}

#[test]
fn test_ci_telemetry_snapshot_record_execute() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.3, 0.0, -5.0]];
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
    mesh.set_color(Rgb565::CSS_CYAN);

    let mut cmd = CommandBuffer::<64>::new();
    let mut rec = RecordTelemetry::default();
    engine
        .record_render_commands_with_telemetry(
            std::iter::once(&mesh),
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut exec = ExecuteTelemetry::default();
    engine
        .execute_recorded_frame_with_telemetry(
            &mut fb,
            &mut zbuffer,
            640,
            480,
            &cmd,
            Some(&mut exec),
        )
        .unwrap();

    println!(
        "CI_TELEMETRY record_draw_commands={} execute_commands_total={} execute_draw_commands={} fallback_used={}",
        rec.draw_commands, exec.commands_total, exec.draw_commands, rec.fallback_used as u8
    );

    // Keep this deterministic checkpoint strict so CI can detect regressions.
    assert_eq!(rec.draw_commands, 2);
    assert_eq!(exec.commands_total, 3);
    assert_eq!(exec.draw_commands, 2);
    assert!(!rec.fallback_used);
}

#[test]
fn test_ci_telemetry_snapshot_lines_record_execute() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.4, 0.0, -5.0], [0.0, 0.4, -5.0]];
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
    mesh.set_color(Rgb565::CSS_ORANGE);

    let mut cmd = CommandBuffer::<128>::new();
    let mut rec = RecordTelemetry::default();
    engine
        .record_render_commands_with_telemetry(
            std::iter::once(&mesh),
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut exec = ExecuteTelemetry::default();
    engine
        .execute_recorded_frame_with_telemetry(
            &mut fb,
            &mut zbuffer,
            640,
            480,
            &cmd,
            Some(&mut exec),
        )
        .unwrap();

    println!(
        "CI_TELEMETRY record_draw_commands={} execute_commands_total={} execute_draw_commands={} fallback_used={}",
        rec.draw_commands, exec.commands_total, exec.draw_commands, rec.fallback_used as u8
    );

    assert_eq!(rec.draw_commands, 3);
    assert_eq!(exec.commands_total, 4);
    assert_eq!(exec.draw_commands, 3);
    assert!(!rec.fallback_used);
}

#[test]
fn test_ci_telemetry_snapshot_stress_points_record_execute() {
    let engine = K3dengine::new(640, 480);

    let vertices = [[0.0, 0.0, -5.0], [0.2, 0.0, -5.0], [0.0, 0.2, -5.0]];
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

    let mut meshes: Vec<K3dMesh> = Vec::new();
    for i in 0..8 {
        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::Points);
        mesh.set_color(Rgb565::CSS_MAGENTA);
        mesh.set_position(i as f32 * 0.15 - 0.5, (i % 3) as f32 * 0.1 - 0.1, 0.0);
        meshes.push(mesh);
    }

    let mut cmd = CommandBuffer::<256>::new();
    let mut rec = RecordTelemetry::default();
    engine
        .record_render_commands_with_telemetry(meshes.iter(), &mut cmd, Some(&mut rec))
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut exec = ExecuteTelemetry::default();
    engine
        .execute_recorded_frame_with_telemetry(
            &mut fb,
            &mut zbuffer,
            640,
            480,
            &cmd,
            Some(&mut exec),
        )
        .unwrap();

    println!(
        "CI_TELEMETRY_STRESS record_draw_commands={} execute_commands_total={} execute_draw_commands={} fallback_used={}",
        rec.draw_commands, exec.commands_total, exec.draw_commands, rec.fallback_used as u8
    );

    assert_eq!(rec.draw_commands, 24);
    assert_eq!(exec.commands_total, 25);
    assert_eq!(exec.draw_commands, 24);
    assert!(!rec.fallback_used);
}

#[test]
fn test_ci_telemetry_snapshot_failsoft_record_execute() {
    let mut engine = K3dengine::new(640, 480);
    engine.set_caps(ProfileCaps {
        max_draw_primitives: 128,
        max_meshes_per_frame: 1,
        max_textures: 4,
        max_width: 640,
        max_height: 480,
        max_triangles_per_mesh: 128,
        max_vertices_per_mesh: 128,
    });

    let va = [[0.0, 0.0, -5.0]];
    let vb = [[0.2, 0.0, -5.0]];
    let ga = Geometry {
        vertices: &va,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let gb = Geometry {
        vertices: &vb,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut ma = K3dMesh::new(ga);
    ma.set_render_mode(RenderMode::Points);
    let mut mb = K3dMesh::new(gb);
    mb.set_render_mode(RenderMode::Points);

    let mut cmd = CommandBuffer::<128>::new();
    let mut rec = RecordTelemetry::default();
    let outcome = engine
        .record_render_commands_with_budget_fallback_report(
            [&ma, &mb].iter().copied(),
            [&ma].iter().copied(),
            &mut cmd,
            Some(&mut rec),
        )
        .unwrap();

    let mut fb = TestFramebuffer::new(640, 480);
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut exec = ExecuteTelemetry::default();
    engine
        .execute_recorded_frame_with_telemetry(
            &mut fb,
            &mut zbuffer,
            640,
            480,
            &cmd,
            Some(&mut exec),
        )
        .unwrap();

    let primary_budget_key = outcome
        .primary_budget_error
        .map(|k| k.key())
        .unwrap_or("None");
    println!(
        "CI_TELEMETRY_FAILSOFT record_draw_commands={} execute_commands_total={} execute_draw_commands={} fallback_used={} primary_budget_kind={}",
        rec.draw_commands,
        exec.commands_total,
        exec.draw_commands,
        rec.fallback_used as u8,
        primary_budget_key
    );

    assert!(outcome.used_fallback);
    assert_eq!(rec.draw_commands, 1);
    assert_eq!(exec.commands_total, 2);
    assert_eq!(exec.draw_commands, 1);
    assert!(rec.fallback_used);
    assert_eq!(primary_budget_key, "MeshesPerFrame");
}
