//! Texture Mapping Demonstration
//!
//! Shows affine texture mapping with UV coordinates on 3D meshes.
//! Demonstrates nearest-neighbor sampling with wrapping.
//!
//! This demo displays:
//! - Textured cubes with checkerboard pattern
//! - Textured cube with brick pattern
//! - UV coordinate wrapping at texture edges
//! - Texture switching with keyboard
//!
//! Controls:
//! - 1/2/3: Switch textures
//! - SPACE: Toggle auto-rotation
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::draw::draw_zbuffered_with_textures;
use embedded_3dgfx::mesh::{Geometry, K3dMesh};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::texture::{Texture, TextureManager};
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

    let mut window = Window::new("Texture Mapping Demo", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 2.0, 8.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; 800 * 600];

    // Create textures
    let mut texture_manager = TextureManager::<4>::new();

    // Checkerboard texture (8×8)
    static CHECKERBOARD_DATA: [Rgb565; 64] = {
        let mut data = [Rgb565::CSS_WHITE; 64];
        let mut i = 0;
        while i < 64 {
            let x = i % 8;
            let y = i / 8;
            if (x + y) % 2 == 0 {
                data[i] = Rgb565::CSS_BLACK;
            }
            i += 1;
        }
        data
    };

    // Brick texture (16×16)
    static BRICK_DATA: [Rgb565; 256] = {
        let mut data = [Rgb565::new(20, 8, 4); 256]; // Brown brick color
        let mut i = 0;
        while i < 256 {
            let x = i % 16;
            let y = i / 16;
            // Mortar lines (horizontal every 4 rows, vertical offset every other row)
            if y % 4 == 0 || (y % 8 < 4 && x == 7) || (y % 8 >= 4 && x == 15) {
                data[i] = Rgb565::new(12, 12, 12); // Gray mortar
            }
            i += 1;
        }
        data
    };

    // Gradient texture (8×8)
    static GRADIENT_DATA: [Rgb565; 64] = {
        let mut data = [Rgb565::CSS_BLACK; 64];
        let mut i = 0;
        while i < 64 {
            let x = i % 8;
            let y = i / 8;
            let r = (x * 4) as u8;
            let g = (y * 8) as u8;
            let b = ((7 - x) * 4) as u8;
            data[i] = Rgb565::new(r, g, b);
            i += 1;
        }
        data
    };

    let tex_id_checker = texture_manager
        .add_texture(Texture::new(&CHECKERBOARD_DATA, 8, 8))
        .unwrap();
    let tex_id_brick = texture_manager
        .add_texture(Texture::new(&BRICK_DATA, 16, 16))
        .unwrap();
    let tex_id_gradient = texture_manager
        .add_texture(Texture::new(&GRADIENT_DATA, 8, 8))
        .unwrap();

    // Create a cube with UV coordinates
    let cube_vertices = [
        [-1.0, -1.0, -1.0], // 0
        [1.0, -1.0, -1.0],  // 1
        [1.0, 1.0, -1.0],   // 2
        [-1.0, 1.0, -1.0],  // 3
        [-1.0, -1.0, 1.0],  // 4
        [1.0, -1.0, 1.0],   // 5
        [1.0, 1.0, 1.0],    // 6
        [-1.0, 1.0, 1.0],   // 7
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

    // UV coordinates for cube (0.0-1.0 range, will wrap)
    let cube_uvs = [
        [0.0, 0.0], // 0
        [1.0, 0.0], // 1
        [1.0, 1.0], // 2
        [0.0, 1.0], // 3
        [0.0, 0.0], // 4
        [1.0, 0.0], // 5
        [1.0, 1.0], // 6
        [0.0, 1.0], // 7
    ];

    let mut current_texture_id = tex_id_checker;
    let texture_names = ["Checkerboard", "Brick", "Gradient"];
    let texture_ids = [tex_id_checker, tex_id_brick, tex_id_gradient];

    let cube_geom = Geometry {
        vertices: &cube_vertices,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &cube_uvs,
        texture_id: Some(current_texture_id),
    };

    // Create three textured cubes
    let mut cube1 = K3dMesh::new(cube_geom);
    cube1.set_position(-3.0, 0.0, 0.0);
    cube1.set_scale(1.2);

    let mut cube2 = K3dMesh::new(cube_geom);
    cube2.set_position(0.0, 0.0, 0.0);
    cube2.set_scale(1.2);

    let mut cube3 = K3dMesh::new(cube_geom);
    cube3.set_position(3.0, 0.0, 0.0);
    cube3.set_scale(1.2);

    let mut auto_rotate = true;
    let start_time = Instant::now();

    println!("Controls:");
    println!("  1           - Checkerboard texture");
    println!("  2           - Brick texture");
    println!("  3           - Gradient texture");
    println!("  SPACE       - Toggle auto-rotation");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(Rgb565::new(5, 10, 15)).unwrap();
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
                    Keycode::Num1 => {
                        current_texture_id = texture_ids[0];
                        println!("Switched to: {}", texture_names[0]);
                    }
                    Keycode::Num2 => {
                        current_texture_id = texture_ids[1];
                        println!("Switched to: {}", texture_names[1]);
                    }
                    Keycode::Num3 => {
                        current_texture_id = texture_ids[2];
                        println!("Switched to: {}", texture_names[2]);
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

        cube1.set_attitude(time * 0.5, time * 0.7, time * 0.3);
        cube2.set_attitude(time * 0.7 + 1.0, time * 0.5 + 1.0, time * 0.4 + 1.0);
        cube3.set_attitude(time * 0.4 + 2.0, time * 0.6 + 2.0, time * 0.5 + 2.0);

        // Update texture IDs for all cubes
        let mut updated_geom = cube_geom;
        updated_geom.texture_id = Some(current_texture_id);

        cube1.geometry = updated_geom;
        cube2.geometry = updated_geom;
        cube3.geometry = updated_geom;

        // Clear display and Z-buffer
        display.clear(Rgb565::new(5, 10, 15)).unwrap();
        zbuffer.fill(u32::MAX);

        // Render with textures
        let cubes = [&cube1, &cube2, &cube3];
        for cube in &cubes {
            let mesh_pos = cube.get_position();
            let distance = (mesh_pos - engine.camera.position).norm();
            let geometry = cube.select_lod(distance);

            // Render each face with texture mapping
            for face in geometry.faces {
                // Transform vertices to clip space
                if let Some([p1, p2, p3]) = engine.transform_points(
                    face,
                    geometry.vertices,
                    engine.camera.vp_matrix * cube.model_matrix,
                ) {
                    // Get UV coordinates if available
                    if let Some(texture_id) = geometry.texture_id {
                        if !geometry.uvs.is_empty() {
                            let uvs = [
                                geometry.uvs[face[0]],
                                geometry.uvs[face[1]],
                                geometry.uvs[face[2]],
                            ];

                            use embedded_3dgfx::DrawPrimitive;
                            draw_zbuffered_with_textures(
                                DrawPrimitive::TexturedTriangleWithDepth {
                                    points: [p1.xy(), p2.xy(), p3.xy()],
                                    depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                    ws: [1.0, 1.0, 1.0],
                                    uvs,
                                    texture_id,
                                },
                                &mut display,
                                &mut zbuffer,
                                800,
                                &texture_manager,
                                None,
                                None,
                            );
                        }
                    }
                }
            }
        }

        // Display info
        perf.print();
        let current_texture_name = texture_names[texture_ids
            .iter()
            .position(|&id| id == current_texture_id)
            .unwrap_or(0)];
        let info_text = format!(
            "{}\nTexture: {}\nAffine mapping (no perspective correction)\nAuto-rotate: {}",
            perf.get_text(),
            current_texture_name,
            if auto_rotate { "ON" } else { "OFF" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "1/2/3: Change texture | SPACE: Toggle rotation | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
