//! Fog and Dithering Effects Demonstration
//!
//! Shows depth-based fog effects and ordered dithering applied to 3D scenes.
//! Fog smoothly blends objects into a fog color based on their distance from the camera.
//! Dithering adds a retro visual effect using a 4x4 Bayer matrix pattern.
//!
//! This demo displays multiple cubes at varying depths to show:
//! - Depth-based fog that increases with distance
//! - Ordered dithering for a retro aesthetic
//! - Both effects working together with Gouraud shading
//!
//! Controls:
//! - F: Toggle fog on/off
//! - D: Toggle dithering on/off
//! - +/-: Adjust fog distance
//! - [/]: Adjust dither intensity
//! - SPACE: Toggle auto-rotation
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::draw::{DitherConfig, FogConfig, draw_zbuffered_with_effects};
use embedded_3dgfx::mesh::{Geometry, K3dMesh};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
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

    let mut window = Window::new("Fog and Dithering Demo", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 3.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; 800 * 600];

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
        Rgb565::new(0, 0, 10),  // Dark blue
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

    // Create multiple cubes at different depths
    let mut cubes: Vec<K3dMesh> = Vec::new();
    for i in 0..7 {
        let mut cube = K3dMesh::new(cube_geom);
        let x = ((i % 3) as f32 - 1.0) * 3.0;
        let y = ((i / 3) as f32 - 1.0) * 3.0;
        let z = -i as f32 * 4.0;
        cube.set_position(x, y, z);
        cube.set_scale(1.0);
        cubes.push(cube);
    }

    // Effect settings
    let mut fog_enabled = true;
    let mut fog_near = 5.0f32;
    let mut fog_far = 25.0f32;
    let fog_color = Rgb565::new(8, 12, 16); // Dark blue-gray fog

    let mut dither_enabled = false;
    let mut dither_intensity = 32u8;

    let mut auto_rotate = true;
    let start_time = Instant::now();

    println!("Controls:");
    println!("  F           - Toggle fog");
    println!("  D           - Toggle dithering");
    println!("  +/-         - Adjust fog distance");
    println!("  [/]         - Adjust dither intensity");
    println!("  SPACE       - Toggle auto-rotation");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(fog_color).unwrap();
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
                    Keycode::F => {
                        fog_enabled = !fog_enabled;
                        println!("Fog: {}", if fog_enabled { "ON" } else { "OFF" });
                    }
                    Keycode::D => {
                        dither_enabled = !dither_enabled;
                        println!("Dither: {}", if dither_enabled { "ON" } else { "OFF" });
                    }
                    Keycode::Plus | Keycode::Equals => {
                        fog_near = (fog_near + 1.0).min(fog_far - 1.0);
                        fog_far = (fog_far + 1.0).min(50.0);
                        println!("Fog range: {:.1} - {:.1}", fog_near, fog_far);
                    }
                    Keycode::Minus => {
                        fog_near = (fog_near - 1.0).max(1.0);
                        fog_far = (fog_far - 1.0).max(fog_near + 1.0);
                        println!("Fog range: {:.1} - {:.1}", fog_near, fog_far);
                    }
                    Keycode::LeftBracket => {
                        dither_intensity = dither_intensity.saturating_sub(8);
                        println!("Dither intensity: {}", dither_intensity);
                    }
                    Keycode::RightBracket => {
                        dither_intensity = dither_intensity.saturating_add(8);
                        println!("Dither intensity: {}", dither_intensity);
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

        for (i, cube) in cubes.iter_mut().enumerate() {
            let offset = i as f32 * 0.3;
            cube.set_attitude(
                time * 0.5 + offset,
                time * 0.7 + offset,
                time * 0.3 + offset,
            );
        }

        // Clear display and Z-buffer
        display.clear(fog_color).unwrap();
        zbuffer.fill(u32::MAX);

        // Setup fog and dither configs
        let fog_config = if fog_enabled {
            Some(FogConfig::new(fog_color, fog_near, fog_far))
        } else {
            None
        };

        let dither_config = if dither_enabled {
            Some(DitherConfig::new(dither_intensity))
        } else {
            None
        };

        // Render with effects
        for cube in &cubes {
            let mesh_pos = cube.get_position();
            let distance = (mesh_pos - engine.camera.position).norm();
            let geometry = cube.select_lod(distance);

            // Render each face with Gouraud shading and effects
            for face in geometry.faces {
                // Transform vertices to clip space
                if let Some([p1, p2, p3]) = engine.transform_points(
                    face,
                    geometry.vertices,
                    engine.camera.vp_matrix * cube.model_matrix,
                ) {
                    // Get vertex colors
                    let colors = if !geometry.colors.is_empty() {
                        [
                            geometry.colors[face[0]],
                            geometry.colors[face[1]],
                            geometry.colors[face[2]],
                        ]
                    } else {
                        [cube.color, cube.color, cube.color]
                    };

                    use embedded_3dgfx::DrawPrimitive;
                    draw_zbuffered_with_effects(
                        DrawPrimitive::GouraudTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                            colors,
                        },
                        &mut display,
                        &mut zbuffer,
                        800,
                        fog_config.as_ref(),
                        dither_config.as_ref(),
                    );
                }
            }
        }

        // Display info
        perf.print();
        let info_text = format!(
            "{}\nFog: {} (range: {:.1}-{:.1})\nDither: {} (intensity: {})\nAuto-rotate: {}",
            perf.get_text(),
            if fog_enabled { "ON" } else { "OFF" },
            fog_near,
            fog_far,
            if dither_enabled { "ON" } else { "OFF" },
            dither_intensity,
            if auto_rotate { "ON" } else { "OFF" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text =
            "F: Fog | D: Dither | +/-: Fog range | [/]: Dither | SPACE: Rotate | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
