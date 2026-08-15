//! Screen-Space Water Reflection (SSR) & Animated Palette Demo
//!
//! Demonstrates:
//! 1. 3D rotating geometry above a reflective horizon plane.
//! 2. `WaterReflectConfig` and `WaterReflectShader` for temporal screen-space water reflections with ripple waves.
//! 3. `AnimatedPalette` cycling colors for animated beacons.

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::retro::AnimatedPalette;
use embedded_3dgfx::shader::{
    FlatColorShader, FragmentShader, WaterReflectConfig, WaterReflectShader,
};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};
use std::time::Instant;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const WATERLINE_Y: i32 = 140;

fn calculate_face_normal(v0: &[f32; 3], v1: &[f32; 3], v2: &[f32; 3]) -> [f32; 3] {
    let edge1 = Vector3::new(v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]);
    let edge2 = Vector3::new(v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]);
    let normal = edge1.cross(&edge2).normalize();
    [normal.x, normal.y, normal.z]
}

fn make_gem_mesh() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let vertices = vec![
        [0.0, 1.4, 0.0],   // 0: Top apex
        [-0.9, 0.0, 0.9],  // 1: Mid front-left
        [0.9, 0.0, 0.9],   // 2: Mid front-right
        [0.9, 0.0, -0.9],  // 3: Mid back-right
        [-0.9, 0.0, -0.9], // 4: Mid back-left
        [0.0, -1.0, 0.0],  // 5: Bottom apex
    ];

    let faces = vec![
        // Top pyramid
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 1],
        // Bottom pyramid
        [5, 2, 1],
        [5, 3, 2],
        [5, 4, 3],
        [5, 1, 4],
    ];

    let mut normals = Vec::with_capacity(faces.len());
    for face in &faces {
        let v0 = &vertices[face[0]];
        let v1 = &vertices[face[1]];
        let v2 = &vertices[face[2]];
        normals.push(calculate_face_normal(v0, v1, v2));
    }

    (vertices, faces, normals)
}

fn draw_sky_backdrop<D: DrawTarget<Color = Rgb565>>(target: &mut D) {
    for y in 0..WATERLINE_Y {
        let t = y as f32 / WATERLINE_Y as f32;
        // Deep blue to warm orange horizon
        let r = (2.0 + t * 26.0) as u8;
        let g = (8.0 + t * 24.0) as u8;
        let b = (28.0 - t * 16.0) as u8;
        let color = Rgb565::new(r, g, b);
        let _ = target.draw_iter((0..WIDTH).map(|x| Pixel(Point::new(x as i32, y), color)));
    }
}

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<2048>::new();
    let mut color_buffer = vec![Rgb565::BLACK; WIDTH * HEIGHT];

    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new(
        "3D SSR Water Reflection & Palette Cycling Demo",
        &output_settings,
    );

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 1.2, 4.5));
    engine.camera.set_target(Point3::new(0.0, 0.4, 0.0));

    let (vertices, faces, normals) = make_gem_mesh();
    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut gem = K3dMesh::new(geometry);
    gem.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    gem.set_position(0.0, 0.5, 0.0);

    // Animated beacon palette (cycling glowing neon shades)
    let beacon_colors = [
        Rgb565::new(31, 0, 0),
        Rgb565::new(31, 16, 0),
        Rgb565::new(31, 31, 0),
        Rgb565::new(0, 48, 10),
        Rgb565::new(0, 32, 31),
        Rgb565::new(16, 0, 31),
    ];
    let mut beacon_palette = AnimatedPalette::new(beacon_colors);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let title_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_CYAN);

    let start_time = Instant::now();
    let mut frame_count = 0usize;

    'running: loop {
        let elapsed = start_time.elapsed().as_secs_f32();
        frame_count += 1;

        // Step animated palette
        if frame_count % 8 == 0 {
            beacon_palette.step(1);
        }
        let current_beacon = beacon_palette.get_color(0);
        gem.set_color(current_beacon);

        // Clear display & zbuffer, draw sky
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);
        draw_sky_backdrop(&mut display);

        // Animate 3D gem rotation and position
        let rot_y = elapsed * 1.2;
        let rot_x = (elapsed * 0.8).sin() * 0.3;
        gem.set_attitude(rot_x, rot_y, 0.0);
        gem.set_position(0.0, 0.5, 0.0);

        // 1. Render 3D Scene into display
        commands.clear();
        let mut frame_ctx = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine.record([&gem], &mut commands, None).unwrap();
        engine
            .execute(&mut display, &mut frame_ctx, &commands, None)
            .unwrap();

        // 2. Snapshot the rendered upper screen for SSR reflection sampling
        for y in 0..WATERLINE_Y {
            for x in 0..WIDTH as i32 {
                let pixel = display.get_pixel(Point::new(x, y));
                color_buffer[y as usize * WIDTH + x as usize] = pixel;
            }
        }

        // 3. Render animated Water Plane using WaterReflectShader
        let ripple_phase = (elapsed * 24.0) as i32;
        let water_cfg = WaterReflectConfig::new(
            &color_buffer,
            WIDTH,
            HEIGHT,
            WATERLINE_Y,
            Rgb565::new(1, 8, 18),
            40, // 40/255 tint blend
        )
        .with_ripples(3, ripple_phase);

        let water_shader = WaterReflectShader {
            inner: FlatColorShader {
                color: Rgb565::new(1, 6, 16),
            },
            config: &water_cfg,
        };

        for y in WATERLINE_Y..HEIGHT as i32 {
            for x in 0..WIDTH as i32 {
                let water_pixel = water_shader.shade(x, y, 0, ());
                let _ = display.draw_iter([Pixel(Point::new(x, y), water_pixel)]);
            }
        }

        // 4. Draw HUD Overlays
        Text::new("3D SSR WATER REFLECTION", Point::new(6, 12), title_style)
            .draw(&mut display)
            .unwrap();
        Text::new(
            "Temporal Horizon Mirror + Ripples",
            Point::new(6, 24),
            text_style,
        )
        .draw(&mut display)
        .unwrap();

        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::KeyDown {
                    keycode: Keycode::Escape,
                    ..
                } => break 'running,
                _ => {}
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
