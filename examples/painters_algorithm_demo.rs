//! Painter's Algorithm Demonstration
//!
//! Shows back-to-front triangle sorting without Z-buffer.
//! This demo saves ~1.92MB of RAM by eliminating the Z-buffer!
//!
//! Features:
//! - Multiple overlapping objects with painter-order approximation
//! - Triangle sorting by average depth
//! - Real-time statistics showing memory savings
//! - Compare with Z-buffered rendering
//!
//! Controls:
//! - SPACE: Toggle between Painter's Algorithm and Z-buffer
//! - R: Rotate objects
//! - ESC: Exit

use embedded_3dgfx::DrawPrimitive;
use embedded_3dgfx::K3dengine;
use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::{CommandBuffer, RenderCommand};
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor, WebColors};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::Point3;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
struct SortedPrimitive {
    primitive: DrawPrimitive,
    depth_key: i32,
    original_order: usize,
}

fn collect_sorted_primitives<const MAX: usize>(
    commands: &CommandBuffer<MAX>,
    out: &mut Vec<SortedPrimitive>,
) {
    out.clear();
    for (idx, cmd) in commands.iter().enumerate() {
        let RenderCommand::Draw(prim) = cmd else {
            continue;
        };
        if let Some(p) = as_painter_primitive(prim) {
            out.push(SortedPrimitive {
                primitive: p,
                depth_key: depth_key_for_primitive(prim),
                original_order: idx,
            });
        }
    }
    out.sort_by(|a, b| {
        b.depth_key
            .cmp(&a.depth_key)
            .then(a.original_order.cmp(&b.original_order))
    });
}

fn depth_key_for_primitive(prim: &DrawPrimitive) -> i32 {
    let depth = match prim {
        DrawPrimitive::ColoredTriangleWithDepth { depths, .. }
        | DrawPrimitive::GouraudTriangleWithDepth { depths, .. }
        | DrawPrimitive::TexturedTriangleWithDepth { depths, .. }
        | DrawPrimitive::TexturedGouraudTriangleWithDepth { depths, .. }
        | DrawPrimitive::LightmappedTriangle { depths, .. } => {
            (depths[0] + depths[1] + depths[2]) / 3.0
        }
        _ => 0.0,
    };
    if depth.is_finite() {
        // Quantize depth so clipped fan pieces from one source face tend to stay grouped.
        (depth * 64.0).round() as i32
    } else {
        0
    }
}

fn as_painter_primitive(prim: &DrawPrimitive) -> Option<DrawPrimitive> {
    match prim {
        DrawPrimitive::ColoredTriangleWithDepth { points, color, .. } => {
            Some(DrawPrimitive::ColoredTriangle(*points, *color))
        }
        DrawPrimitive::GouraudTriangleWithDepth { points, colors, .. } => {
            Some(DrawPrimitive::GouraudTriangle {
                points: *points,
                colors: *colors,
            })
        }
        DrawPrimitive::ColoredTriangle(..)
        | DrawPrimitive::GouraudTriangle { .. }
        | DrawPrimitive::Line(..)
        | DrawPrimitive::ColoredPoint(..) => Some(prim.clone()),
        // Painter fallback for this demo only supports flat/gouraud primitives.
        DrawPrimitive::TexturedTriangle { .. }
        | DrawPrimitive::TexturedTriangleWithDepth { .. }
        | DrawPrimitive::TexturedGouraudTriangleWithDepth { .. }
        | DrawPrimitive::TranslucentTriangleWithDepth { .. }
        | DrawPrimitive::LightmappedTriangle { .. } => None,
    }
}

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut commands = CommandBuffer::<8192>::new();
    let mut cmd_mesh_0 = CommandBuffer::<4096>::new();
    let mut cmd_mesh_1 = CommandBuffer::<4096>::new();
    let mut cmd_mesh_2 = CommandBuffer::<4096>::new();

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Painter's Algorithm Demo - NO Z-BUFFER!", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    // Desktop demo: keep framebuffer unbounded so 800x600 Z-buffer mode can run.
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 3.0, 12.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Create cube geometry
    let cube_vertices = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    let cube_faces = [
        [0, 1, 2],
        [0, 2, 3], // Front
        [1, 5, 6],
        [1, 6, 2], // Right
        [5, 4, 7],
        [5, 7, 6], // Back
        [4, 0, 3],
        [4, 3, 7], // Left
        [3, 2, 6],
        [3, 6, 7], // Top
        [4, 5, 1],
        [4, 1, 0], // Bottom
    ];

    // Per-face normals matching triangle winding. Supplying normals enables
    // stable backface culling in both painter and z-buffer paths.
    let cube_normals = [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0], // Front
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0], // Right
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0], // Back
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0], // Left
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0], // Top
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0], // Bottom
    ];

    let cube_geom = Geometry {
        vertices: &cube_vertices,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &cube_normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    // Create multiple cubes at different positions
    let mut cube1 = K3dMesh::new(cube_geom);
    cube1.set_position(-4.2, 0.0, -1.2);
    cube1.set_scale(1.1);
    cube1.set_color(Rgb565::new(31, 0, 0)); // Red
    cube1.set_render_mode(RenderMode::Solid);

    let mut cube2 = K3dMesh::new(cube_geom);
    cube2.set_position(0.0, 0.0, -3.6);
    cube2.set_scale(1.1);
    cube2.set_color(Rgb565::new(0, 63, 0)); // Green
    cube2.set_render_mode(RenderMode::Solid);

    let mut cube3 = K3dMesh::new(cube_geom);
    cube3.set_position(4.2, 0.0, -6.0);
    cube3.set_scale(1.1);
    cube3.set_color(Rgb565::new(0, 0, 31)); // Blue
    cube3.set_render_mode(RenderMode::Solid);

    // Sorted primitive buffer for painter-mode replay.
    let mut sorted: Vec<SortedPrimitive> = Vec::new();
    let mut sorted_m0: Vec<SortedPrimitive> = Vec::new();
    let mut sorted_m1: Vec<SortedPrimitive> = Vec::new();
    let mut sorted_m2: Vec<SortedPrimitive> = Vec::new();

    let mut use_painters = true;
    let mut rotation = 0.0f32;
    let mut auto_rotate = true;

    println!("Painter's Algorithm Demo");
    println!("========================");
    println!("Memory Savings: ~1.92MB (no Z-buffer needed!)");
    println!();
    println!("Controls:");
    println!("  SPACE       - Toggle Painter's Algorithm / Z-buffer mode");
    println!("  R           - Toggle auto-rotation");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        perf.start_of_frame();

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => {
                        use_painters = !use_painters;
                        println!(
                            "Rendering mode: {}",
                            if use_painters {
                                "PAINTER'S ALGORITHM (No Z-buffer)"
                            } else {
                                "Z-BUFFER (1.92MB RAM)"
                            }
                        );
                    }
                    Keycode::R => {
                        auto_rotate = !auto_rotate;
                        println!("Auto-rotate: {}", if auto_rotate { "ON" } else { "OFF" });
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Update rotation
        if auto_rotate {
            rotation += 0.02;
        }

        // Update object rotations
        // Keep motion coherent across meshes to make Painter and Z-buffer
        // comparison visually stable.
        cube1.set_attitude(0.18, rotation, 0.12);
        cube2.set_attitude(0.18, rotation + 0.35, 0.12);
        cube3.set_attitude(0.18, rotation + 0.7, 0.12);

        // Clear display
        display.clear(Rgb565::BLACK).unwrap();

        let meshes = [&cube1, &cube2, &cube3];
        let triangle_count;

        // Record once so painter and z-buffer consume the exact same primitive stream.
        engine
            .record(meshes.iter().copied(), &mut commands, None)
            .unwrap();

        if use_painters {
            // Mesh-batch painter: sort meshes back-to-front first, then sort each
            // mesh's own primitives. This avoids unstable interleaving artifacts
            // that a single global primitive sort can introduce.
            engine
                .record(std::iter::once(&cube1), &mut cmd_mesh_0, None)
                .unwrap();
            engine
                .record(std::iter::once(&cube2), &mut cmd_mesh_1, None)
                .unwrap();
            engine
                .record(std::iter::once(&cube3), &mut cmd_mesh_2, None)
                .unwrap();
            collect_sorted_primitives(&cmd_mesh_0, &mut sorted_m0);
            collect_sorted_primitives(&cmd_mesh_1, &mut sorted_m1);
            collect_sorted_primitives(&cmd_mesh_2, &mut sorted_m2);

            // Also keep a global buffer for on-screen triangle count.
            sorted.clear();
            sorted.extend(sorted_m0.iter().cloned());
            sorted.extend(sorted_m1.iter().cloned());
            sorted.extend(sorted_m2.iter().cloned());

            let mut mesh_order = [
                (
                    0usize,
                    (cube1.get_position() - engine.camera.position).norm_squared(),
                ),
                (
                    1usize,
                    (cube2.get_position() - engine.camera.position).norm_squared(),
                ),
                (
                    2usize,
                    (cube3.get_position() - engine.camera.position).norm_squared(),
                ),
            ];
            mesh_order.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));

            for (idx, _) in mesh_order {
                let bucket = match idx {
                    0 => &sorted_m0,
                    1 => &sorted_m1,
                    _ => &sorted_m2,
                };
                for item in bucket {
                    draw(item.primitive.clone(), &mut display);
                }
            }
            triangle_count = sorted.len();
        } else {
            // Traditional Z-buffered rendering (reference)
            let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
            let mut frame = FrameCtx {
                zbuffer: &mut zbuffer,
                width: WIDTH,
                height: HEIGHT,
            };
            engine
                .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
                .unwrap();

            triangle_count = commands
                .iter()
                .filter(|c| matches!(c, embedded_3dgfx::command_buffer::RenderCommand::Draw(_)))
                .count();
        }

        // Display info
        perf.print();

        let zbuffer_size_mb = (800 * 600 * 4) as f32 / (1024.0 * 1024.0);
        let mode_str = if use_painters {
            "PAINTER'S ALGORITHM"
        } else {
            "Z-BUFFER MODE"
        };

        let info_text = format!(
            "{}\nMode: {}\nTriangles: {}\nZ-Buffer: {} ({:.2} MB)\nMemory Saved: {:.2} MB",
            perf.get_text(),
            mode_str,
            triangle_count,
            if use_painters { "NONE" } else { "ACTIVE" },
            if use_painters { 0.0 } else { zbuffer_size_mb },
            if use_painters { zbuffer_size_mb } else { 0.0 }
        );

        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "SPACE: Toggle mode | R: Rotate | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
