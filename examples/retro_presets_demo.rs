//! Retro visual preset showcase.
//!
//! Demonstrates Doom/PSX/modern style switches using `RetroStyle`.
//!
//! Controls:
//! - `1`: Doom-like preset
//! - `2`: PSX-like preset
//! - `3`: Modern preset
//! - `SPACE`: Toggle auto-rotation
//! - `ESC`: Exit

use std::thread;
use std::time::{Duration, Instant};

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::{engine::K3dengine, retro::RetroStyle};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::Point3;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;

const CUBE_VERTICES: [[f32; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];

const CUBE_FACES: [[usize; 3]; 12] = [
    [0, 1, 2],
    [0, 2, 3],
    [1, 5, 6],
    [1, 6, 2],
    [5, 4, 7],
    [5, 7, 6],
    [4, 0, 3],
    [4, 3, 7],
    [3, 2, 6],
    [3, 6, 7],
    [4, 5, 1],
    [4, 1, 0],
];

fn apply_style(engine: &mut K3dengine, style_idx: usize) -> &'static str {
    match style_idx {
        0 => {
            engine.apply_retro_style(RetroStyle::doom_walkable());
            "Doom"
        }
        1 => {
            engine.apply_retro_style(RetroStyle::psx());
            "PSX"
        }
        _ => {
            engine.apply_retro_style(RetroStyle::modern());
            "Modern"
        }
    }
}

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new("Retro Presets Demo", &output_settings);

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 2.0, 10.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<8192>::new();

    let geometry = Geometry {
        vertices: &CUBE_VERTICES,
        faces: &CUBE_FACES,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh_left = K3dMesh::new(geometry);
    mesh_left.set_position(-3.0, 0.0, 0.0);
    mesh_left.set_scale(1.5);
    mesh_left.set_render_mode(RenderMode::Solid);
    mesh_left.set_color(Rgb565::CSS_RED);

    let mut mesh_center = K3dMesh::new(geometry);
    mesh_center.set_position(0.0, 0.0, 0.0);
    mesh_center.set_scale(1.6);
    mesh_center.set_render_mode(RenderMode::Solid);
    mesh_center.set_color(Rgb565::CSS_GREEN);

    let mut mesh_right = K3dMesh::new(geometry);
    mesh_right.set_position(3.0, 0.0, 0.0);
    mesh_right.set_scale(1.5);
    mesh_right.set_render_mode(RenderMode::Solid);
    mesh_right.set_color(Rgb565::CSS_BLUE);

    let mut style_idx = 0usize;
    let mut style_name = apply_style(&mut engine, style_idx);
    let mut auto_rotate = true;
    let start = Instant::now();

    println!("Controls:");
    println!("  1 - Doom preset");
    println!("  2 - PSX preset");
    println!("  3 - Modern preset");
    println!("  SPACE - Toggle auto-rotation");
    println!("  ESC - Exit");

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => auto_rotate = !auto_rotate,
                    Keycode::Num1 => {
                        style_idx = 0;
                        style_name = apply_style(&mut engine, style_idx);
                    }
                    Keycode::Num2 => {
                        style_idx = 1;
                        style_name = apply_style(&mut engine, style_idx);
                    }
                    Keycode::Num3 => {
                        style_idx = 2;
                        style_name = apply_style(&mut engine, style_idx);
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        let t = if auto_rotate {
            start.elapsed().as_secs_f32()
        } else {
            0.0
        };
        mesh_left.set_attitude(0.5 + t * 0.8, t * 0.6, 0.1);
        mesh_center.set_attitude(0.2 + t * 0.5, 0.6 + t * 0.9, 0.3);
        mesh_right.set_attitude(0.8 + t * 0.3, 1.1 + t * 0.7, 0.2);

        display.clear(Rgb565::new(2, 4, 8)).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        let meshes = [&mesh_left, &mesh_center, &mesh_right];
        engine
            .record(meshes.iter().copied(), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
            .unwrap();

        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 16), text_style)
                .draw(&mut display)
                .unwrap();
        }
        Text::new(
            &format!(
                "Preset: {style_name} | 1 Doom  2 PSX  3 Modern | Auto-rotate: {}",
                if auto_rotate { "ON" } else { "OFF" }
            ),
            Point::new(10, 30),
            text_style,
        )
        .draw(&mut display)
        .unwrap();
        Text::new(
            "SPACE toggle rotation | ESC exit",
            Point::new(10, 46),
            text_style,
        )
        .draw(&mut display)
        .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
