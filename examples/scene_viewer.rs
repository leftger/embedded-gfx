//! Scene viewer with multiple objects
//!
//! Demonstrates rendering multiple meshes with different transformations.
//! Use arrow keys to rotate the scene and +/- to zoom.

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::engine::K3dengine;
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
use nalgebra::Point3;
use std::thread;
use std::time::{Duration, Instant};

fn make_pyramid() -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let vertices = vec![
        [0.0, 1.0, 0.0],    // Top
        [-1.0, -1.0, -1.0], // Base
        [1.0, -1.0, -1.0],
        [1.0, -1.0, 1.0],
        [-1.0, -1.0, 1.0],
    ];

    let faces = vec![
        [0, 1, 2], // Front
        [0, 2, 3], // Right
        [0, 3, 4], // Back
        [0, 4, 1], // Left
        [1, 3, 2], // Base
        [1, 4, 3],
    ];

    (vertices, faces)
}

fn make_cube() -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
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
        [0, 2, 3],
        [5, 4, 7],
        [5, 7, 6],
        [3, 2, 6],
        [3, 6, 7],
        [4, 5, 1],
        [4, 1, 0],
        [1, 5, 6],
        [1, 6, 2],
        [4, 0, 3],
        [4, 3, 7],
    ];

    (vertices, faces)
}

fn make_grid() -> Vec<[f32; 3]> {
    let mut vertices = Vec::new();
    let size = 10;
    let spacing = 2.0;

    for i in 0..size {
        for j in 0..size {
            let x = (i as f32 - size as f32 / 2.0) * spacing;
            let z = (j as f32 - size as f32 / 2.0) * spacing;
            vertices.push([x, 0.0, z]);
        }
    }

    vertices
}

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<8192>::new();

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new(
        "3D Scene Viewer - Arrow keys to rotate, +/- to zoom",
        &output_settings,
    );

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);

    // Create objects
    let grid_vertices = make_grid();
    let grid_geometry = Geometry {
        vertices: &grid_vertices,
        faces: &[],
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut grid = K3dMesh::new(grid_geometry);
    grid.set_render_mode(RenderMode::Points);
    grid.set_color(Rgb565::new(0, 10, 0));

    let (cube_verts, cube_faces) = make_cube();
    let cube_geometry1 = Geometry {
        vertices: &cube_verts,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut cube1 = K3dMesh::new(cube_geometry1);
    cube1.set_render_mode(RenderMode::Lines);
    cube1.set_color(Rgb565::CSS_CYAN);
    cube1.set_position(-3.0, 1.0, 0.0);

    let cube_geometry2 = Geometry {
        vertices: &cube_verts,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut cube2 = K3dMesh::new(cube_geometry2);
    cube2.set_render_mode(RenderMode::Solid);
    cube2.set_color(Rgb565::CSS_MAGENTA);
    cube2.set_position(3.0, 1.0, 0.0);
    cube2.set_scale(0.8);

    let (pyramid_verts, pyramid_faces) = make_pyramid();
    let pyramid_geometry = Geometry {
        vertices: &pyramid_verts,
        faces: &pyramid_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut pyramid = K3dMesh::new(pyramid_geometry);
    pyramid.set_render_mode(RenderMode::Lines);
    pyramid.set_color(Rgb565::CSS_YELLOW);
    pyramid.set_position(0.0, 0.5, 5.0);

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let mut camera_angle = 0.0f32;
    let mut camera_distance = 15.0f32;
    let mut camera_height = 5.0f32;

    let start_time = Instant::now();

    println!("Controls:");
    println!("  Arrow Keys - Rotate camera");
    println!("  +/- - Zoom in/out");
    println!("  ESC - Exit");

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
                    Keycode::Left => camera_angle -= 0.05,
                    Keycode::Right => camera_angle += 0.05,
                    Keycode::Up => camera_height += 0.5,
                    Keycode::Down => camera_height -= 0.5,
                    Keycode::Equals | Keycode::Plus => camera_distance -= 1.0,
                    Keycode::Minus => camera_distance += 1.0,
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Update camera position
        let cam_x = camera_distance * camera_angle.cos();
        let cam_z = camera_distance * camera_angle.sin();
        engine
            .camera
            .set_position(Point3::new(cam_x, camera_height, cam_z));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        // Animate objects
        let time = start_time.elapsed().as_secs_f32();
        cube1.set_attitude(time * 0.5, time, 0.0);
        cube2.set_attitude(0.0, -time * 0.7, time * 0.3);
        pyramid.set_attitude(0.0, time * 0.8, 0.0);

        // Clear display
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        engine
            .record(
                [&grid, &cube1, &cube2, &pyramid].iter().copied(),
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
            "{}\nCamera: angle={:.1}, dist={:.1}, height={:.1}",
            perf.get_text(),
            camera_angle,
            camera_distance,
            camera_height
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
}
