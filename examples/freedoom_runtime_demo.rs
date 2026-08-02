//! Freedoom WAD runtime demo.
//!
//! Loads generated BSP data from `examples/freedoom1_bsp.rs` and renders it
//! with animated sector lights.
//!
//! Run with:
//! `cargo run --example freedoom_runtime_demo --features std`

use std::f32::consts::PI;
use std::thread;
use std::time::{Duration, Instant};

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::bsp::BspTelemetry;
use embedded_3dgfx::bsp::scratch::BspScratch;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::texture::{Texture, TextureManager};
use embedded_3dgfx::{FogConfig, K3dengine, RetroStyle, StippleMode};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};

#[path = "freedoom1_bsp.rs"]
mod freedoom1_bsp;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const EYE_HEIGHT: f32 = 0.55;
const PLAYER_RADIUS: f32 = 0.12;

static CHECKER: [Rgb565; 64] = {
    let mut data = [Rgb565::new(0, 0, 0); 64];
    let mut i = 0usize;
    while i < 64 {
        let x = i % 8;
        let y = i / 8;
        data[i] = if (x + y).is_multiple_of(2) {
            Rgb565::new(10, 20, 10)
        } else {
            Rgb565::new(5, 10, 5)
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
            // The default PSX preset fog is tuned for smaller scenes.
            engine.set_fog(FogConfig::new(Rgb565::new(2, 2, 4), 20.0, 120.0));
            "PSX"
        }
        _ => {
            engine.apply_retro_style(RetroStyle::modern());
            "Modern"
        }
    }
}

fn point_in_polygon(p: [f32; 2], poly: &[[f32; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let edge_x = b[0] - a[0];
        let edge_z = b[1] - a[1];
        let to_p_x = p[0] - a[0];
        let to_p_z = p[1] - a[1];
        let cross = edge_x * to_p_z - edge_z * to_p_x;
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign.abs() < 1e-6 {
            sign = cross.signum();
        } else if sign * cross < 0.0 {
            return false;
        }
    }
    true
}

fn segment_intersects(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> bool {
    fn orient(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }
    let o1 = orient(a0, a1, b0);
    let o2 = orient(a0, a1, b1);
    let o3 = orient(b0, b1, a0);
    let o4 = orient(b0, b1, a1);
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

fn point_segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let ab_len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if ab_len2 <= 1e-8 {
        let dx = p[0] - a[0];
        let dz = p[1] - a[1];
        return (dx * dx + dz * dz).sqrt();
    }
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / ab_len2).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    let dx = p[0] - q[0];
    let dz = p[1] - q[1];
    (dx * dx + dz * dz).sqrt()
}

fn collides_with_walls(from: [f32; 2], to: [f32; 2], walls: &[[f32; 4]], radius: f32) -> bool {
    for w in walls {
        let a = [w[0], w[1]];
        let b = [w[2], w[3]];
        if segment_intersects(from, to, a, b) {
            return true;
        }
        if point_segment_distance(to, a, b) < radius {
            return true;
        }
    }
    false
}

fn sample_floor_and_ceiling(
    x: f32,
    z: f32,
    points: &[[f32; 2]],
    regions: &[freedoom1_bsp::NavFloorRegion],
) -> Option<(f32, f32)> {
    let p = [x, z];
    for r in regions {
        let start = r.first_point as usize;
        let end = start + r.point_count as usize;
        if end > points.len() || start >= end {
            continue;
        }
        if point_in_polygon(p, &points[start..end]) {
            return Some((r.floor_y, r.ceil_y));
        }
    }
    None
}

fn main() {
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let mut window = Window::new("Freedoom Runtime Demo", &output_settings);

    let world = freedoom1_bsp::bsp_world();
    let sector_lights = freedoom1_bsp::bsp_sector_lights();
    let nav_walls = freedoom1_bsp::bsp_nav_wall_segments();
    let nav_floor_points = freedoom1_bsp::bsp_nav_floor_points();
    let nav_floor_regions = freedoom1_bsp::bsp_nav_floor_regions();

    let mut visframe = vec![0u32; world.faces.len().max(1)];
    let mut scratch = BspScratch::new(&mut visframe);
    // Keep the large fixed-capacity command buffer off the thread stack.
    let mut commands = Box::new(CommandBuffer::<8192>::new());
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];

    let mut textures = TextureManager::<2>::new();
    textures
        .add_texture(Texture::new(&CHECKER, 8, 8))
        .expect("failed to add checker texture");

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.camera.set_near_far(0.1, 100.0);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let mut style_idx = 0usize;
    let mut style_name = apply_style(&mut engine, style_idx);
    let mut use_stipple = false;

    let mut cam = Point3::new(0.0, 0.4, 0.0);
    let mut yaw = PI * 0.5;
    let move_speed = 0.3f32;
    let turn_speed = 0.08f32;
    let start = Instant::now();

    // embedded-graphics-simulator requires at least one update before polling events.
    display.clear(Rgb565::new(1, 3, 6)).unwrap();
    window.update(&display);

    println!("Controls:");
    println!("  W/S - Move forward/back");
    println!("  A/D - Turn left/right");
    println!("  1/2/3 - Doom / PSX / Modern preset");
    println!("  4 - Toggle checkerboard stipple");
    println!("  ESC - Exit");

    'running: loop {
        let mut desired = [cam.x, cam.z];
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::W => {
                        desired[0] += yaw.sin() * move_speed;
                        desired[1] -= yaw.cos() * move_speed;
                    }
                    Keycode::S => {
                        desired[0] -= yaw.sin() * move_speed;
                        desired[1] += yaw.cos() * move_speed;
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

        let from = [cam.x, cam.z];
        let try_x = [desired[0], cam.z];
        if !collides_with_walls(from, try_x, nav_walls, PLAYER_RADIUS) {
            cam.x = desired[0];
        }
        let from2 = [cam.x, cam.z];
        let try_z = [cam.x, desired[1]];
        if !collides_with_walls(from2, try_z, nav_walls, PLAYER_RADIUS) {
            cam.z = desired[1];
        }

        if let Some((floor, ceil)) =
            sample_floor_and_ceiling(cam.x, cam.z, nav_floor_points, nav_floor_regions)
        {
            let head = floor + EYE_HEIGHT;
            cam.y = head.min(ceil - 0.1);
        }

        engine.camera.set_position(cam);
        let look = Vector3::new(yaw.sin(), 0.0, -yaw.cos());
        engine.camera.set_target(cam + look);
        engine.set_stipple_mode(if use_stipple {
            StippleMode::Checkerboard
        } else {
            StippleMode::Off
        });

        display.clear(Rgb565::new(1, 3, 6)).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        let mut tel = BspTelemetry::default();
        let t = start.elapsed().as_secs_f32();
        engine
            .record_bsp_with_sector_lights(
                &world,
                &mut scratch,
                &mut commands,
                Some(sector_lights),
                t,
                Some(&mut tel),
            )
            .unwrap();

        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute_bsp_textured::<_, 8192, 2>(
                &mut display,
                &mut frame,
                &commands,
                &textures,
                None,
            )
            .unwrap();

        Text::new(
            &format!(
                "Freedoom E1M1 | Preset: {style_name} | Faces {} | Tris {} | SectorLights {}",
                tel.faces_visible,
                tel.triangles_emitted,
                sector_lights.len()
            ),
            Point::new(8, 16),
            text_style,
        )
        .draw(&mut display)
        .unwrap();
        Text::new(
            "W/S move, A/D turn, 1/2/3 style, 4 stipple, ESC exit",
            Point::new(8, 30),
            text_style,
        )
        .draw(&mut display)
        .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
