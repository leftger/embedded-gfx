//! Vertex Animation Demonstration
//!
//! Shows keyframe-based vertex animation with linear interpolation.
//! Demonstrates multiple animated objects with different animation types:
//! - Pulsing cube (scale animation)
//! - Waving flag (sine wave deformation)
//! - Morphing object (keyframe interpolation)
//!
//! Controls:
//! - SPACE: Toggle animation playback
//! - R: Reset animation time
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::animation::{Keyframe, VertexAnimation};
use embedded_3dgfx::draw::draw_zbuffered;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
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
use std::f32::consts::PI;
use std::thread;
use std::time::Duration;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Vertex Animation Demo", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    engine.camera.set_position(Point3::new(0.0, 3.0, 12.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; 800 * 600];

    // Create cube keyframes (pulsing animation)
    let cube_base = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    let mut cube_expanded = cube_base;
    for v in &mut cube_expanded {
        v[0] *= 1.3;
        v[1] *= 1.3;
        v[2] *= 1.3;
    }

    let cube_keyframes = [
        Keyframe {
            vertices: &cube_base,
            time: 0.0,
        },
        Keyframe {
            vertices: &cube_expanded,
            time: 0.5,
        },
        Keyframe {
            vertices: &cube_base,
            time: 1.0,
        },
    ];

    let cube_animation = VertexAnimation::new(&cube_keyframes, true);

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

    // Create animated vertices buffer
    let mut cube_animated_verts = [[0.0f32; 3]; 8];

    // Create flag mesh (will be animated with sine wave)
    let flag_width = 10;
    let flag_height = 6;
    let mut flag_base_verts = Vec::new();
    let mut flag_faces = Vec::new();

    for y in 0..flag_height {
        for x in 0..flag_width {
            flag_base_verts.push([x as f32 * 0.4 - 2.0, y as f32 * 0.4 - 1.2, 0.0]);
        }
    }

    // Generate flag faces (triangulated grid)
    for y in 0..(flag_height - 1) {
        for x in 0..(flag_width - 1) {
            let i = y * flag_width + x;
            flag_faces.push([i, i + flag_width, i + 1]);
            flag_faces.push([i + 1, i + flag_width, i + flag_width + 1]);
        }
    }

    // Create waving flag keyframes
    let mut flag_wave1 = flag_base_verts.clone();
    let mut flag_wave2 = flag_base_verts.clone();

    for (i, v) in flag_wave1.iter_mut().enumerate() {
        let x = i % flag_width;
        let wave = (x as f32 / flag_width as f32 * 2.0 * PI).sin();
        v[2] = wave * 0.5;
    }

    for (i, v) in flag_wave2.iter_mut().enumerate() {
        let x = i % flag_width;
        let wave = (x as f32 / flag_width as f32 * 2.0 * PI + PI).sin();
        v[2] = wave * 0.5;
    }

    let flag_keyframes = [
        Keyframe {
            vertices: &flag_base_verts,
            time: 0.0,
        },
        Keyframe {
            vertices: &flag_wave1,
            time: 0.5,
        },
        Keyframe {
            vertices: &flag_wave2,
            time: 1.0,
        },
        Keyframe {
            vertices: &flag_base_verts,
            time: 1.5,
        },
    ];

    let flag_animation = VertexAnimation::new(&flag_keyframes, true);
    let mut flag_animated_verts = vec![[0.0f32; 3]; flag_base_verts.len()];

    // Create sphere that morphs into a cube
    let mut sphere_verts = Vec::new();
    let mut cube_morph_verts = Vec::new();
    let mut morph_faces = Vec::new();

    let segments = 8;
    let rings = 6;

    for ring in 0..=rings {
        let phi = (ring as f32 / rings as f32) * PI;
        for segment in 0..=segments {
            let theta = (segment as f32 / segments as f32) * 2.0 * PI;

            let x = phi.sin() * theta.cos();
            let y = phi.cos();
            let z = phi.sin() * theta.sin();

            sphere_verts.push([x, y, z]);

            // Cube version (normalized cube coordinates)
            let cx = if x.abs() > y.abs() && x.abs() > z.abs() {
                x.signum()
            } else if y.abs() > z.abs() {
                x / y.abs()
            } else {
                x / z.abs()
            };
            let cy = if y.abs() > x.abs() && y.abs() > z.abs() {
                y.signum()
            } else if x.abs() > z.abs() {
                y / x.abs()
            } else {
                y / z.abs()
            };
            let cz = if z.abs() > x.abs() && z.abs() > y.abs() {
                z.signum()
            } else if x.abs() > y.abs() {
                z / x.abs()
            } else {
                z / y.abs()
            };

            cube_morph_verts.push([cx, cy, cz]);
        }
    }

    for ring in 0..rings {
        for segment in 0..segments {
            let current = ring * (segments + 1) + segment;
            let next = current + segments + 1;

            morph_faces.push([current, next, current + 1]);
            morph_faces.push([current + 1, next, next + 1]);
        }
    }

    let morph_keyframes = [
        Keyframe {
            vertices: &sphere_verts,
            time: 0.0,
        },
        Keyframe {
            vertices: &cube_morph_verts,
            time: 1.0,
        },
        Keyframe {
            vertices: &sphere_verts,
            time: 2.0,
        },
    ];

    let morph_animation = VertexAnimation::new(&morph_keyframes, true);
    let mut morph_animated_verts = vec![[0.0f32; 3]; sphere_verts.len()];

    let mut anim_playing = true;
    let mut anim_time = 0.0f32;

    println!("Controls:");
    println!("  SPACE       - Toggle animation playback");
    println!("  R           - Reset animation");
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
                        anim_playing = !anim_playing;
                        println!(
                            "Animation: {}",
                            if anim_playing { "PLAYING" } else { "PAUSED" }
                        );
                    }
                    Keycode::R => {
                        anim_time = 0.0;
                        println!("Animation reset");
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Update animation time
        if anim_playing {
            let dt = perf.get_frametime() as f32 / 1_000_000.0;
            anim_time += dt;
        }

        // Sample animations
        cube_animation.sample(anim_time, &mut cube_animated_verts);
        flag_animation.sample(anim_time * 2.0, &mut flag_animated_verts);
        morph_animation.sample(anim_time * 0.8, &mut morph_animated_verts);

        // Create meshes with animated vertices
        let cube_geom = Geometry {
            vertices: &cube_animated_verts,
            faces: &cube_faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut cube = K3dMesh::new(cube_geom);
        cube.set_position(-3.5, 0.0, 0.0);
        cube.set_color(Rgb565::CSS_CYAN);
        cube.set_render_mode(RenderMode::Lines);

        let flag_geom = Geometry {
            vertices: &flag_animated_verts,
            faces: &flag_faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut flag = K3dMesh::new(flag_geom);
        flag.set_position(0.0, 0.0, 0.0);
        flag.set_color(Rgb565::CSS_RED);
        flag.set_render_mode(RenderMode::Solid);

        let morph_geom = Geometry {
            vertices: &morph_animated_verts,
            faces: &morph_faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut morph = K3dMesh::new(morph_geom);
        morph.set_position(3.5, 0.0, 0.0);
        morph.set_color(Rgb565::CSS_GREEN);
        morph.set_render_mode(RenderMode::Lines);

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        // Render all meshes
        engine.render([&cube, &flag, &morph].iter().copied(), |prim| {
            draw_zbuffered(prim, &mut display, &mut zbuffer, 800);
        });

        // Display info
        perf.print();
        let info_text = format!(
            "{}\nVertex Animation: Keyframe interpolation\nTime: {:.2}s | Status: {}\nCube: Pulse | Flag: Wave | Sphere: Morph",
            perf.get_text(),
            anim_time,
            if anim_playing { "PLAYING" } else { "PAUSED" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "SPACE: Play/Pause | R: Reset | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
