//! BSP builder + renderer showcase.
//!
//! Builds a small multi-room BSP world at startup using `bsp::builder`,
//! then renders it with `record_bsp` + `execute_bsp_textured`.
//!
//! Controls:
//! - `W/S`: move forward/back
//! - `A/D`: turn left/right
//! - `1/2/3`: Doom/PSX/Modern style presets
//! - `4`: toggle checkerboard stipple
//! - `ESC`: exit

use std::f32::consts::PI;
use std::thread;
use std::time::Duration;

use embedded_3dgfx::bsp::BspTelemetry;
use embedded_3dgfx::bsp::builder::{RoomSpec, build_room_strip};
use embedded_3dgfx::bsp::scratch::BspScratch;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::texture::{Texture, TextureManager};
use embedded_3dgfx::{K3dengine, RetroStyle, StippleMode};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;

static CHECKER: [Rgb565; 64] = {
    let mut data = [Rgb565::new(0, 0, 0); 64];
    let mut i = 0usize;
    while i < 64 {
        let x = i % 8;
        let y = i / 8;
        data[i] = if (x + y).is_multiple_of(2) {
            Rgb565::new(4, 10, 4)
        } else {
            Rgb565::new(12, 24, 10)
        };
        i += 1;
    }
    data
};

static CEIL: [Rgb565; 64] = {
    let mut data = [Rgb565::new(0, 0, 0); 64];
    let mut i = 0usize;
    while i < 64 {
        let x = i % 8;
        let y = i / 8;
        data[i] = if (x + y).is_multiple_of(2) {
            Rgb565::new(8, 8, 16)
        } else {
            Rgb565::new(3, 4, 10)
        };
        i += 1;
    }
    data
};

fn apply_style(engine: &mut K3dengine, idx: usize) -> &'static str {
    match idx {
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
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let mut window = Window::new("BSP Builder Demo", &output_settings);

    let rooms = [
        RoomSpec {
            mins: [-9.0, -2.0, -3.0],
            maxs: [-3.0, 2.0, 3.0],
            floor_texture_id: 0,
            ceiling_texture_id: 1,
            lightmap_id: 0xFFFF,
        },
        RoomSpec {
            mins: [-3.0, -2.0, -3.0],
            maxs: [3.0, 2.0, 3.0],
            floor_texture_id: 0,
            ceiling_texture_id: 1,
            lightmap_id: 0xFFFF,
        },
        RoomSpec {
            mins: [3.0, -2.0, -3.0],
            maxs: [9.0, 2.0, 3.0],
            floor_texture_id: 0,
            ceiling_texture_id: 1,
            lightmap_id: 0xFFFF,
        },
    ];
    let owned = build_room_strip(&rooms).expect("failed to build BSP room strip");
    let world = owned.as_world();

    let mut visframe = vec![0u32; world.faces.len()];
    let mut scratch = BspScratch::new(&mut visframe);
    let mut commands = CommandBuffer::<8192>::new();
    let mut zbuffer = vec![u32::MAX; WIDTH * HEIGHT];

    let mut textures = TextureManager::<4>::new();
    textures.add_texture(Texture::new(&CHECKER, 8, 8)).unwrap();
    textures.add_texture(Texture::new(&CEIL, 8, 8)).unwrap();

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.camera.set_near_far(0.1, 40.0);

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let mut style_idx = 0usize;
    let mut style_name = apply_style(&mut engine, style_idx);
    let mut use_stipple = false;

    let mut cam = Point3::new(-7.5, 0.0, 0.0);
    let mut yaw = PI * 0.5;
    let move_speed = 0.24f32;
    let turn_speed = 0.075f32;

    println!("Controls:");
    println!("  W/S - Move forward/back");
    println!("  A/D - Turn left/right");
    println!("  1/2/3 - Doom / PSX / Modern preset");
    println!("  4 - Toggle checkerboard stipple");
    println!("  ESC - Exit");

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::W => {
                        cam.x += yaw.sin() * move_speed;
                        cam.z -= yaw.cos() * move_speed;
                    }
                    Keycode::S => {
                        cam.x -= yaw.sin() * move_speed;
                        cam.z += yaw.cos() * move_speed;
                    }
                    Keycode::A => yaw -= turn_speed,
                    Keycode::D => yaw += turn_speed,
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
                    Keycode::Num4 => {
                        use_stipple = !use_stipple;
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        cam.x = cam.x.clamp(-8.5, 8.5);
        cam.z = cam.z.clamp(-2.2, 2.2);

        engine.camera.set_position(cam);
        let look = Vector3::new(yaw.sin(), 0.0, -yaw.cos());
        engine.camera.set_target(cam + look);
        engine.set_stipple_mode(if use_stipple {
            StippleMode::Checkerboard
        } else {
            StippleMode::Off
        });

        display.clear(Rgb565::new(1, 3, 6)).unwrap();
        zbuffer.fill(u32::MAX);

        let mut tel = BspTelemetry::default();
        engine
            .record_bsp(&world, &mut scratch, &mut commands, Some(&mut tel))
            .unwrap();

        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute_bsp_textured::<_, 8192, 4>(
                &mut display,
                &mut frame,
                &commands,
                &textures,
                None,
            )
            .unwrap();

        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(8, 14), text_style)
                .draw(&mut display)
                .unwrap();
        }
        Text::new(
            &format!(
                "BSP Builder Demo | Preset: {style_name} | Faces {} | Tris {}",
                tel.faces_visible, tel.triangles_emitted
            ),
            Point::new(8, 28),
            text_style,
        )
        .draw(&mut display)
        .unwrap();
        Text::new(
            &format!(
                "pos x:{:.1} z:{:.1} | 1/2/3 style | 4 stipple: {} | ESC exit",
                cam.x,
                cam.z,
                if use_stipple { "ON" } else { "OFF" }
            ),
            Point::new(8, 42),
            text_style,
        )
        .draw(&mut display)
        .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
