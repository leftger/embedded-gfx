//! Jelly Cube Demo
//!
//! Demonstrates 3D soft body physics with a deformable cube.
//! Shows volume preservation and bouncy behavior.
//!
//! Features:
//! - 3D mass-spring network
//! - Pressure/volume preservation
//! - Ground collision and bouncing
//! - Interactive forces
//!
//! Controls:
//! - SPACE: Drop cube
//! - UP: Apply upward force
//! - P: Toggle pressure (volume preservation)
//! - R: Reset
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::softbody::SoftBody;
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};
use std::thread;
use std::time::Duration;

fn main() {
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(320, 240));

    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Jelly Cube Demo", &output_settings);

    let mut engine = K3dengine::new(320, 240);
    engine.camera.set_position(Point3::new(0.0, 2.0, 8.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Create jelly cube (3x3x3 grid)
    let cube_size = 3;
    let spacing = 0.5;
    let stiffness = 150.0;
    let damping = 0.5;

    let mut jelly = SoftBody::<64, 256>::create_jelly_cube(
        cube_size,
        spacing,
        stiffness,
        damping,
    ).expect("Failed to create jelly cube");

    // Position cube above ground
    for particle in jelly.particles.iter_mut() {
        particle.position.y += 4.0;
        particle.previous_position.y += 4.0;
    }

    jelly.ground_restitution = 0.6; // Bouncy
    jelly.ground_friction = 0.3;

    // Create surface faces (just outer surface for visualization)
    let mut faces = Vec::new();

    // Simple cube surface (front, back, left, right, top, bottom faces)
    // For a 3x3x3 cube, we'll create triangles for visible surfaces
    for z in 0..cube_size {
        for y in 0..cube_size {
            for x in 0..cube_size {
                // Only add faces on the outer surface
                let is_surface = x == 0 || x == cube_size - 1 ||
                                y == 0 || y == cube_size - 1 ||
                                z == 0 || z == cube_size - 1;

                if is_surface {
                    let idx = z * cube_size * cube_size + y * cube_size + x;

                    // Add some faces (simplified - just connecting nearby vertices)
                    if x < cube_size - 1 && y < cube_size - 1 {
                        faces.push([idx, idx + 1, idx + cube_size]);
                        faces.push([idx + 1, idx + cube_size + 1, idx + cube_size]);
                    }
                }
            }
        }
    }

    let mut vertex_buffer = vec![[0.0f32; 3]; jelly.particles.len()];

    println!("Jelly Cube Demo");
    println!("Controls:");
    println!("  SPACE: Drop cube");
    println!("  UP: Apply upward force");
    println!("  P: Toggle pressure");
    println!("  R: Reset");
    println!("  ESC: Exit");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => {
                        // Drop cube from height
                        for particle in jelly.particles.iter_mut() {
                            particle.position.y += 2.0;
                            particle.previous_position.y += 2.0;
                            particle.velocity = Vector3::zeros();
                        }
                    }
                    Keycode::Up => {
                        // Apply upward impulse
                        jelly.apply_global_force(Vector3::new(0.0, 500.0, 0.0));
                    }
                    Keycode::P => {
                        jelly.pressure_config.enabled = !jelly.pressure_config.enabled;
                    }
                    Keycode::R => {
                        // Reset jelly cube
                        jelly = SoftBody::<64, 256>::create_jelly_cube(
                            cube_size,
                            spacing,
                            stiffness,
                            damping,
                        ).expect("Failed to create jelly cube");

                        for particle in jelly.particles.iter_mut() {
                            particle.position.y += 4.0;
                            particle.previous_position.y += 4.0;
                        }
                        jelly.ground_restitution = 0.6;
                        jelly.ground_friction = 0.3;
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Step simulation
        jelly.step(0.016);

        // Get deformed vertices
        jelly.get_vertex_positions(&mut vertex_buffer);

        // Create mesh
        let geometry = Geometry {
            vertices: &vertex_buffer,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::Lines);
        mesh.set_color(Rgb565::CSS_MAGENTA);

        // Clear and render
        display.clear(Rgb565::BLACK).unwrap();

        engine.render(std::iter::once(&mesh), |prim| {
            draw(prim, &mut display);
        });

        // Display info
        let pressure_text = if jelly.pressure_config.enabled {
            "Pressure: ON"
        } else {
            "Pressure: OFF"
        };
        Text::new(pressure_text, Point::new(10, 10), text_style)
            .draw(&mut display)
            .unwrap();

        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 230), text_style)
                .draw(&mut display)
                .unwrap();
        }

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
