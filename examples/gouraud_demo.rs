//! Gouraud Shading Demonstration
//!
//! Shows smooth color interpolation across triangle faces using vertex colors.
//! Gouraud shading interpolates colors from vertices to create smooth gradients,
//! giving a much better visual appearance than flat shading.
//!
//! This demo displays several objects with per-vertex coloring:
//! - A rotating pyramid with colored vertices
//! - A color wheel demonstrating smooth gradients
//!
//! Controls:
//! - SPACE: Toggle auto-rotation
//! - ESC: Exit

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::draw::draw_zbuffered;
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::mesh::{Geometry, K3dMesh};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor, WebColors};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::Point3;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Gouraud Shading Demo", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 3.0, 10.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![Z_MAX_VALUE; 800 * 600];

    // Create a pyramid with colored vertices
    let pyramid_vertices = [
        [0.0, 1.0, 0.0],    // Top
        [-1.0, -1.0, 1.0],  // Front left
        [1.0, -1.0, 1.0],   // Front right
        [1.0, -1.0, -1.0],  // Back right
        [-1.0, -1.0, -1.0], // Back left
    ];

    let pyramid_faces = [
        [0, 1, 2], // Front
        [0, 2, 3], // Right
        [0, 3, 4], // Back
        [0, 4, 1], // Left
        [1, 4, 3], // Bottom 1
        [1, 3, 2], // Bottom 2
    ];

    // Vertex colors for pyramid (top is white, base vertices are colored)
    let pyramid_colors = [
        Rgb565::CSS_WHITE,  // Top
        Rgb565::CSS_RED,    // Front left
        Rgb565::CSS_GREEN,  // Front right
        Rgb565::CSS_BLUE,   // Back right
        Rgb565::CSS_YELLOW, // Back left
    ];

    let pyramid_geom = Geometry {
        vertices: &pyramid_vertices,
        faces: &pyramid_faces,
        colors: &pyramid_colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut pyramid = K3dMesh::new(pyramid_geom);
    pyramid.set_position(-3.0, 0.0, 0.0);
    pyramid.set_scale(1.5);

    // Create a cube with Gouraud shading
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

    let cube_colors = [
        Rgb565::new(0, 0, 0),   // Black
        Rgb565::new(31, 0, 0),  // Red
        Rgb565::new(31, 63, 0), // Yellow
        Rgb565::new(0, 63, 0),  // Green
        Rgb565::new(0, 0, 31),  // Blue
        Rgb565::new(31, 0, 31), // Magenta
        Rgb565::CSS_WHITE,      // White
        Rgb565::new(0, 63, 31), // Cyan
    ];

    let cube_geom = Geometry {
        vertices: &cube_vertices,
        faces: &cube_faces,
        colors: &cube_colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut cube = K3dMesh::new(cube_geom);
    cube.set_position(3.0, 0.0, 0.0);
    cube.set_scale(1.0);

    let mut auto_rotate = true;
    let start_time = Instant::now();

    println!("Controls:");
    println!("  SPACE       - Toggle auto-rotation");
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
                        auto_rotate = !auto_rotate;
                        println!("Auto-rotate: {}", if auto_rotate { "ON" } else { "OFF" });
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Animate objects
        let time = if auto_rotate {
            start_time.elapsed().as_secs_f32()
        } else {
            0.0
        };

        pyramid.set_attitude(0.0, time * 1.5, 0.0);
        cube.set_attitude(time * 0.8, time * 0.6, time * 0.4);

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        // Render using Gouraud shading
        let meshes = [&pyramid, &cube];
        for mesh in &meshes {
            let mesh_pos = mesh.get_position();
            let distance = (mesh_pos - engine.camera.position).norm();
            let geometry = mesh.select_lod(distance);

            // Render each face with Gouraud shading
            for face in geometry.faces {
                // Transform vertices to clip space
                if let Some([p1, p2, p3]) = engine.transform_points(
                    face,
                    geometry.vertices,
                    engine.camera.vp_matrix * mesh.model_matrix,
                ) {
                    // Get vertex colors
                    let colors = if !geometry.colors.is_empty() {
                        [
                            geometry.colors[face[0]],
                            geometry.colors[face[1]],
                            geometry.colors[face[2]],
                        ]
                    } else {
                        // Fallback to mesh color
                        [mesh.color, mesh.color, mesh.color]
                    };

                    use embedded_3dgfx::primitive::DrawPrimitive;
                    draw_zbuffered(
                        DrawPrimitive::GouraudTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                            colors,
                        },
                        &mut display,
                        &mut zbuffer,
                        800,
                    );
                }
            }
        }

        // Display info
        perf.print();
        let info_text = format!(
            "{}\nGouraud Shading: Smooth vertex color interpolation\nAuto-rotate: {}",
            perf.get_text(),
            if auto_rotate { "ON" } else { "OFF" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "SPACE: Toggle rotation | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
