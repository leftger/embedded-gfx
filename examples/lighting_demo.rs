//! Lighting demonstration
//!
//! Shows directional lighting on 3D meshes with animated light direction.
//! This demo uses simple cubes positioned to show clear lighting differences.

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};
use std::thread;
use std::time::{Duration, Instant};

fn calculate_face_normal(v0: &[f32; 3], v1: &[f32; 3], v2: &[f32; 3]) -> [f32; 3] {
    let edge1 = Vector3::new(v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]);
    let edge2 = Vector3::new(v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]);
    let normal = edge1.cross(&edge2).normalize();
    [normal.x, normal.y, normal.z]
}

fn make_cube() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let vertices = vec![
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
    ];

    let faces = vec![
        [0, 1, 2],
        [0, 2, 3], // Front
        [5, 4, 7],
        [5, 7, 6], // Back
        [3, 2, 6],
        [3, 6, 7], // Top
        [4, 5, 1],
        [4, 1, 0], // Bottom
        [1, 5, 6],
        [1, 6, 2], // Right
        [4, 0, 3],
        [4, 3, 7], // Left
    ];

    // Calculate per-face normals
    let mut normals = Vec::new();
    for face in &faces {
        let v0 = &vertices[face[0]];
        let v1 = &vertices[face[1]];
        let v2 = &vertices[face[2]];
        normals.push(calculate_face_normal(v0, v1, v2));
    }

    (vertices, faces, normals)
}

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Lighting Demo - Arrow keys to move light", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 3.0, 10.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Create three cubes - this makes lighting super obvious
    let (cube_verts, cube_faces, cube_normals) = make_cube();

    let cube_geometry = Geometry {
        vertices: &cube_verts,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &cube_normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut cube1 = K3dMesh::new(cube_geometry);
    cube1.set_color(Rgb565::new(31, 0, 0)); // Bright red
    cube1.set_position(-3.0, 0.0, 0.0);

    let cube_geometry2 = Geometry {
        vertices: &cube_verts,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &cube_normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut cube2 = K3dMesh::new(cube_geometry2);
    cube2.set_color(Rgb565::new(0, 63, 0)); // Bright green
    cube2.set_position(0.0, 0.0, 0.0);

    let cube_geometry3 = Geometry {
        vertices: &cube_verts,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &cube_normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut cube3 = K3dMesh::new(cube_geometry3);
    cube3.set_color(Rgb565::new(0, 0, 31)); // Bright blue
    cube3.set_position(3.0, 0.0, 0.0);

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Create Z-buffer (using u32 for better embedded performance)
    // u32::MAX represents infinity (furthest distance)
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<8192>::new();

    let start_time = Instant::now();
    let mut auto_rotate = true;
    let mut manual_light_angle_h = 0.0f32; // horizontal
    let mut manual_light_angle_v = 0.0f32; // vertical

    println!("Lighting Demo - Watch the cube faces change brightness!");
    println!("Controls:");
    println!("  SPACE       - Toggle auto-rotation");
    println!("  Arrow Keys  - Adjust light direction");
    println!("  ESC         - Exit");
    println!();
    println!("IMPORTANT: Watch how different faces have different brightness!");
    println!("           This shows the directional lighting at work.");

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
                    Keycode::Left => {
                        manual_light_angle_h -= 0.2;
                    }
                    Keycode::Right => {
                        manual_light_angle_h += 0.2;
                    }
                    Keycode::Up => {
                        manual_light_angle_v += 0.1;
                        println!("Light height: {:.1}", manual_light_angle_v);
                    }
                    Keycode::Down => {
                        manual_light_angle_v -= 0.1;
                        println!("Light height: {:.1}", manual_light_angle_v);
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Calculate light direction
        let time = start_time.elapsed().as_secs_f32();
        let light_angle_h = if auto_rotate {
            time * 1.0
        } else {
            manual_light_angle_h
        };
        let light_angle_v = if auto_rotate {
            (time * 0.5).sin() * 0.3
        } else {
            manual_light_angle_v
        };

        // Create light direction vector
        let light_dir =
            Vector3::new(light_angle_h.cos(), light_angle_v, light_angle_h.sin()).normalize();

        // Rotate cubes for dynamic lighting demonstration
        cube1.set_attitude(time * 0.3, time * 0.5, time * 0.2);
        cube2.set_attitude(time * 0.4, time * 0.3, time * 0.6);
        cube3.set_attitude(time * 0.5, time * 0.4, time * 0.3);

        // Update all cubes with the same lighting
        cube1.set_render_mode(RenderMode::SolidLightDir(light_dir));
        cube2.set_render_mode(RenderMode::SolidLightDir(light_dir));
        cube3.set_render_mode(RenderMode::SolidLightDir(light_dir));

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        engine
            .record(
                [&cube1, &cube2, &cube3].iter().copied(),
                &mut commands,
                None,
            )
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
            .unwrap();

        // Display info
        perf.print();
        let info_text = format!(
            "{}\nLight: [{:.2}, {:.2}, {:.2}]\nAuto: {} | Use Arrow Keys to move light",
            perf.get_text(),
            light_dir.x,
            light_dir.y,
            light_dir.z,
            if auto_rotate { "ON" } else { "OFF" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Draw lighting status at bottom
        let status_text = "Watch the cube faces - they should change brightness as light moves!";
        Text::new(status_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Draw light direction indicator (top-right corner)
        let indicator_x = 750;
        let indicator_y = 50;
        let indicator_radius = 30.0;

        // Draw background circle
        for angle in 0..360 {
            let rad = (angle as f32).to_radians();
            let px = (indicator_x as f32 + rad.cos() * indicator_radius) as i32;
            let py = (indicator_y as f32 + rad.sin() * indicator_radius) as i32;
            if (0..800).contains(&px) && (0..600).contains(&py) {
                display
                    .draw_iter(std::iter::once(Pixel(
                        Point::new(px, py),
                        Rgb565::new(5, 5, 5),
                    )))
                    .ok();
            }
        }

        // Draw light direction as a cross
        let light_x = indicator_x as f32 + light_dir.x * indicator_radius;
        let light_y = indicator_y as f32 - light_dir.z * indicator_radius;

        for i in -3..=3 {
            let px = (light_x as i32 + i).clamp(0, 799);
            let py = (light_y as i32).clamp(0, 599);
            display
                .draw_iter(std::iter::once(Pixel(
                    Point::new(px, py),
                    Rgb565::CSS_YELLOW,
                )))
                .ok();

            let px = (light_x as i32).clamp(0, 799);
            let py = (light_y as i32 + i).clamp(0, 599);
            display
                .draw_iter(std::iter::once(Pixel(
                    Point::new(px, py),
                    Rgb565::CSS_YELLOW,
                )))
                .ok();
        }

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
}
