//! Cloth Simulation Demo
//!
//! Demonstrates soft body physics with a hanging cloth.
//! Uses mass-spring system for realistic fabric behavior.
//!
//! Features:
//! - Mass-spring network for cloth deformation
//! - Gravity and damping
//! - Pinned constraints (cloth hangs from top)
//! - Structural and shear springs for stability
//! - Real-time interaction
//!
//! Controls:
//! - W: Apply wind force (right)
//! - S: Apply wind force (left)
//! - G: Toggle gravity
//! - R: Reset cloth
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
    let mut window = Window::new("Cloth Simulation", &output_settings);

    let mut engine = K3dengine::new(320, 240);
    engine.camera.set_position(Point3::new(0.0, 2.0, 8.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Create cloth (8x8 grid)
    let cloth_width = 8;
    let cloth_height = 8;
    let spacing = 0.4;
    let stiffness = 200.0;
    let damping = 1.0;

    let mut cloth = SoftBody::<64, 256>::create_cloth(
        cloth_width,
        cloth_height,
        spacing,
        stiffness,
        damping,
    ).expect("Failed to create cloth");

    // Position cloth at nice viewing height
    for particle in cloth.particles.iter_mut() {
        particle.position.y += 4.0;
        particle.previous_position.y += 4.0;
    }

    cloth.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    // Create faces for rendering
    let mut faces = Vec::new();
    for y in 0..cloth_height - 1 {
        for x in 0..cloth_width - 1 {
            let tl = y * cloth_width + x;
            let tr = y * cloth_width + (x + 1);
            let bl = (y + 1) * cloth_width + x;
            let br = (y + 1) * cloth_width + (x + 1);

            // Two triangles per quad
            faces.push([tl, tr, bl]);
            faces.push([tr, br, bl]);
        }
    }

    let mut vertex_buffer = vec![[0.0f32; 3]; cloth.particles.len()];
    let mut gravity_enabled = true;
    let mut wind_force;

    println!("Cloth Simulation Demo");
    println!("Controls:");
    println!("  W: Wind right");
    println!("  S: Wind left");
    println!("  G: Toggle gravity");
    println!("  R: Reset");
    println!("  ESC: Exit");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        // Handle events
        wind_force = Vector3::zeros();
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::W => wind_force = Vector3::new(50.0, 0.0, 0.0),
                    Keycode::S => wind_force = Vector3::new(-50.0, 0.0, 0.0),
                    Keycode::G => {
                        gravity_enabled = !gravity_enabled;
                        cloth.set_gravity(if gravity_enabled {
                            Vector3::new(0.0, -9.81, 0.0)
                        } else {
                            Vector3::zeros()
                        });
                    }
                    Keycode::R => {
                        // Reset cloth
                        cloth = SoftBody::<64, 256>::create_cloth(
                            cloth_width,
                            cloth_height,
                            spacing,
                            stiffness,
                            damping,
                        ).expect("Failed to create cloth");

                        for particle in cloth.particles.iter_mut() {
                            particle.position.y += 4.0;
                            particle.previous_position.y += 4.0;
                        }
                        cloth.set_gravity(Vector3::new(0.0, -9.81, 0.0));
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Apply wind
        if wind_force.magnitude() > 0.0 {
            cloth.apply_global_force(wind_force);
        }

        // Step simulation
        cloth.step(0.016);

        // Get deformed vertices for rendering
        cloth.get_vertex_positions(&mut vertex_buffer);

        // Create mesh
        let geometry = Geometry {
            vertices: &vertex_buffer,
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
        mesh.set_color(Rgb565::CSS_CYAN);

        // Clear and render
        display.clear(Rgb565::BLACK).unwrap();

        engine.render(std::iter::once(&mesh), |prim| {
            draw(prim, &mut display);
        });

        // Display info
        let gravity_text = if gravity_enabled { "Gravity: ON" } else { "Gravity: OFF" };
        Text::new(gravity_text, Point::new(10, 10), text_style)
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
