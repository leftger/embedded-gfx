//! Painter's Algorithm Demonstration
//!
//! Shows back-to-front triangle sorting without Z-buffer.
//! This demo saves ~1.92MB of RAM by eliminating the Z-buffer!
//!
//! Features:
//! - Multiple overlapping objects rendered correctly
//! - Triangle sorting by average depth
//! - Real-time statistics showing memory savings
//! - Compare with Z-buffered rendering
//!
//! Controls:
//! - SPACE: Toggle between Painter's Algorithm and Z-buffer
//! - R: Rotate objects
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "std")]
use embedded_3dgfx::painters::DepthSortedTriangle;
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
use std::time::Duration;

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut commands = CommandBuffer::<8192>::new();

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Painter's Algorithm Demo - NO Z-BUFFER!", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
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

    let cube_geom = Geometry {
        vertices: &cube_vertices,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    // Create multiple cubes at different positions
    let mut cube1 = K3dMesh::new(cube_geom);
    cube1.set_position(-2.0, 0.0, 0.0);
    cube1.set_scale(1.5);
    cube1.set_color(Rgb565::new(31, 0, 0)); // Red
    cube1.set_render_mode(RenderMode::Solid);

    let mut cube2 = K3dMesh::new(cube_geom);
    cube2.set_position(0.0, 0.0, -1.0);
    cube2.set_scale(1.5);
    cube2.set_color(Rgb565::new(0, 63, 0)); // Green
    cube2.set_render_mode(RenderMode::Solid);

    let mut cube3 = K3dMesh::new(cube_geom);
    cube3.set_position(2.0, 0.0, -2.0);
    cube3.set_scale(1.5);
    cube3.set_color(Rgb565::new(0, 0, 31)); // Blue
    cube3.set_render_mode(RenderMode::Solid);

    // Triangle buffer for Painter's Algorithm
    let mut triangles: Vec<DepthSortedTriangle> = Vec::new();

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
        cube1.set_attitude(0.0, rotation, rotation * 0.7);
        cube2.set_attitude(rotation * 0.5, rotation, 0.0);
        cube3.set_attitude(rotation * 0.3, 0.0, rotation);

        // Clear display
        display.clear(Rgb565::BLACK).unwrap();

        let meshes = [&cube1, &cube2, &cube3];
        let triangle_count;

        if use_painters {
            // Render using Painter's Algorithm (no Z-buffer!)
            triangle_count =
                engine.render_painters_algorithm(meshes.iter().copied(), &mut triangles, |prim| {
                    draw(prim, &mut display);
                });
        } else {
            // Traditional Z-buffered rendering (for comparison)
            let mut zbuffer = vec![u32::MAX; WIDTH * HEIGHT];
            engine
                .record_render_commands(meshes.iter().copied(), &mut commands)
                .unwrap();
            engine
                .execute_recorded_frame::<_, 8192>(
                    &mut display,
                    &mut zbuffer,
                    WIDTH,
                    HEIGHT,
                    &commands,
                )
                .unwrap();

            triangle_count = triangles.len();
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
