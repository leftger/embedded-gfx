//! Basic rendering example
//!
//! Demonstrates rendering simple 3D shapes in different modes:
//! - Points mode
//! - Lines mode
//! - Solid mode
//!
//! Press SPACE to cycle through render modes

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::Point3;
use std::thread;
use std::time::Duration;

fn make_cube_vertices() -> Vec<[f32; 3]> {
    vec![
        // Front face
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        // Back face
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
    ]
}

fn make_cube_faces() -> Vec<[usize; 3]> {
    vec![
        // Front
        [0, 1, 2],
        [0, 2, 3],
        // Back
        [5, 4, 7],
        [5, 7, 6],
        // Top
        [3, 2, 6],
        [3, 6, 7],
        // Bottom
        [4, 5, 1],
        [4, 1, 0],
        // Right
        [1, 5, 6],
        [1, 6, 2],
        // Left
        [4, 0, 3],
        [4, 3, 7],
    ]
}

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new(
        "Basic Rendering - Press SPACE to change render mode",
        &output_settings,
    );

    // Create 3D engine
    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 2.0, 5.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Create cube mesh
    let vertices = make_cube_vertices();
    let faces = make_cube_faces();

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

    let mut cube = K3dMesh::new(geometry);
    cube.set_color(Rgb565::CSS_CYAN);

    let mut current_mode = 0;
    let modes = [
        ("Points", RenderMode::Points),
        ("Lines", RenderMode::Lines),
        ("Solid", RenderMode::Solid),
    ];

    println!("Controls:");
    println!("  SPACE - Change render mode");
    println!("  ESC   - Exit");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    engine.render(std::iter::once(&cube), |prim| {
        draw(prim, &mut display);
    });
    window.update(&display);

    'running: loop {
        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Space => {
                        current_mode = (current_mode + 1) % modes.len();
                        cube.set_render_mode(modes[current_mode].1.clone());
                        println!("Render mode: {}", modes[current_mode].0);
                    }
                    Keycode::Escape => {
                        break 'running;
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Clear display
        display.clear(Rgb565::BLACK).unwrap();

        // Render the cube
        engine.render(std::iter::once(&cube), |prim| {
            draw(prim, &mut display);
        });

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
}
