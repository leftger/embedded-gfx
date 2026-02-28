//! STL Model Viewer with First-Person Controls
//!
//! This demo loads and displays multiple STL models in a 3D scene.
//! Features first-person camera controls for navigation.
//!
//! Models included:
//! - Suzanne (Blender monkey head) - rendered in wireframe
//! - Teapot - solid shaded, animated scale
//! - Blahaj (IKEA shark) - solid shaded with directional lighting
//!
//! Controls:
//! - W/S: Move forward/backward
//! - A/D: Turn left/right
//! - Q/E: Look up/down
//! - Arrow Left/Right: Strafe left/right
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw_zbuffered;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use load_stl::embed_stl;
use nalgebra::Point3;
use std::f32::consts::PI;
use std::thread;
use std::time::Duration;

/// Create a ground plane (XZ plane) with grid vertices
fn make_xz_plane() -> Vec<[f32; 3]> {
    let step = 1.0;
    let nsteps = 10;

    let mut vertices = Vec::new();
    for i in 0..nsteps {
        for j in 0..nsteps {
            vertices.push([
                (i as f32 - nsteps as f32 / 2.0) * step,
                0.0,
                (j as f32 - nsteps as f32 / 2.0) * step,
            ]);
        }
    }

    vertices
}

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new(
        "STL Viewer - WASD to move, Arrows to look",
        &output_settings,
    );

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    engine.camera.set_fovy(PI / 4.0);

    // Create ground plane
    let ground_vertices = make_xz_plane();
    let mut ground = K3dMesh::new(Geometry {
        vertices: &ground_vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    });
    ground.set_color(Rgb565::new(0, 15, 0)); // Dim green

    // Load STL models using the embed_stl! macro
    println!("Loading Suzanne (monkey head)...");
    let mut suzanne = K3dMesh::new(embed_stl!("examples/3d_models/Suzanne.stl"));
    suzanne.set_render_mode(RenderMode::Lines);
    suzanne.set_scale(2.0);
    suzanne.set_color(Rgb565::CSS_RED);
    suzanne.set_position(0.0, 0.7, 10.0);

    println!("Loading Teapot...");
    let mut teapot = K3dMesh::new(embed_stl!("examples/3d_models/Teapot_low.stl"));
    teapot.set_position(-10.0, 0.0, 0.0);
    teapot.set_color(Rgb565::CSS_BLUE_VIOLET);

    println!("Loading Blahaj (shark)...");
    let mut blahaj = K3dMesh::new(embed_stl!("examples/3d_models/blahaj.stl"));
    blahaj.set_color(Rgb565::new(105 >> 3, 150 >> 2, 173 >> 3)); // Blahaj blue
    blahaj.set_render_mode(RenderMode::SolidLightDir(nalgebra::Vector3::new(
        -1.0, 0.0, 0.0,
    )));
    blahaj.set_position(0.0, 0.0, 0.0);

    println!("Models loaded successfully!");

    // Player (camera) state
    let mut player_pos = Point3::new(-10.0, 2.0, 0.0);
    let mut player_yaw = 0.0f32; // Horizontal rotation
    let mut player_pitch = 0.0f32; // Vertical rotation (look up/down)

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; 800 * 600];

    // Animation parameter
    let mut time = 0.0f32;

    println!("Controls:");
    println!("  W/S         - Move forward/backward");
    println!("  A/D         - Turn left/right");
    println!("  Q/E         - Look up/down");
    println!("  Arrow Left/Right - Strafe left/right");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        perf.start_of_frame();

        // Movement parameters (fixed step size)
        let walking_speed = 0.3;
        let turning_speed = 0.1;

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,

                    // Forward/backward movement
                    Keycode::W => {
                        player_pos.x += player_yaw.cos() * walking_speed;
                        player_pos.z += player_yaw.sin() * walking_speed;
                    }
                    Keycode::S => {
                        player_pos.x -= player_yaw.cos() * walking_speed;
                        player_pos.z -= player_yaw.sin() * walking_speed;
                    }

                    // Turning left/right
                    Keycode::A => {
                        player_yaw -= turning_speed;
                    }
                    Keycode::D => {
                        player_yaw += turning_speed;
                    }

                    // Look up/down
                    Keycode::Q => {
                        player_pitch += turning_speed;
                        player_pitch = player_pitch.min(PI / 2.0 - 0.1); // Clamp
                    }
                    Keycode::E => {
                        player_pitch -= turning_speed;
                        player_pitch = player_pitch.max(-PI / 2.0 + 0.1); // Clamp
                    }

                    // Strafe left/right
                    Keycode::Left => {
                        player_pos.x -= (player_yaw + PI / 2.0).cos() * walking_speed;
                        player_pos.z -= (player_yaw + PI / 2.0).sin() * walking_speed;
                    }
                    Keycode::Right => {
                        player_pos.x += (player_yaw + PI / 2.0).cos() * walking_speed;
                        player_pos.z += (player_yaw + PI / 2.0).sin() * walking_speed;
                    }

                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        let dt = perf.get_frametime() as f32 / 1_000_000.0;

        // Update camera position
        engine.camera.set_position(player_pos);

        // Calculate look-at target
        let lookat = player_pos
            + nalgebra::Vector3::new(
                player_yaw.cos() * player_pitch.cos(),
                player_pitch.sin(),
                player_yaw.sin() * player_pitch.cos(),
            );
        engine.camera.set_target(lookat);

        // Animate objects
        suzanne.set_attitude(-PI / 2.0, time * 2.0, 0.0);
        suzanne.set_position(0.0, 0.7 + (time * 3.4).sin() * 0.2, 10.0);

        blahaj.set_attitude(-PI / 2.0, time * 2.0, 0.0);
        blahaj.set_position(0.0, 0.7 + (time * 3.4).sin() * 0.2, 0.0);

        teapot.set_attitude(-PI / 2.0, time * 1.0, 0.0);
        teapot.set_scale(0.2 + 0.1 * (time * 5.0).sin());

        time += 0.3 * dt;

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        // Render all meshes with Z-buffering
        engine.render(
            [&ground, &teapot, &suzanne, &blahaj].iter().copied(),
            |prim| {
                draw_zbuffered(prim, &mut display, &mut zbuffer, 800);
            },
        );

        // Display performance info
        perf.print();
        let info_text = format!(
            "{}\\nPos: ({:.1}, {:.1}, {:.1})\\nYaw: {:.1}° Pitch: {:.1}°",
            perf.get_text(),
            player_pos.x,
            player_pos.y,
            player_pos.z,
            player_yaw.to_degrees(),
            player_pitch.to_degrees()
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text =
            "W/S: Move | A/D: Turn | Q/E: Look Up/Down | Left/Right: Strafe | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
