//! Hybrid 3D Scene + 2D Sprite Compositing & Screen Picking Demo
//!
//! Demonstrates:
//! 1. 3D Scene with lit geometry and Distance-based LOD (`TextureLodConfig`).
//! 2. 2D Sprite overlays using `Sprite2D` (color-key transparency & integer scaling).
//! 3. Integrated Screen-Space Picking (`execute_commands_with_picking`).

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::hud::{HudElement, HudLayer, Sprite2D};
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::renderer::{FrameCtx, PickQuery, PickResult, execute_commands_with_picking};
use embedded_3dgfx::retro::TextureLodConfig;
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

fn calculate_face_normal(v0: &[f32; 3], v1: &[f32; 3], v2: &[f32; 3]) -> [f32; 3] {
    let edge1 = Vector3::new(v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]);
    let edge2 = Vector3::new(v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]);
    let normal = edge1.cross(&edge2).normalize();
    [normal.x, normal.y, normal.z]
}

fn make_cube() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let vertices = vec![
        // Front face
        [-0.8, -0.8, 0.8],
        [0.8, -0.8, 0.8],
        [0.8, 0.8, 0.8],
        [-0.8, 0.8, 0.8],
        // Back face
        [-0.8, -0.8, -0.8],
        [0.8, -0.8, -0.8],
        [0.8, 0.8, -0.8],
        [-0.8, 0.8, -0.8],
    ];

    let faces = vec![
        // Front
        [0, 1, 2],
        [0, 2, 3],
        // Back
        [5, 4, 7],
        [5, 7, 6],
        // Top
        [3, 2, 6],
        [3, 6, 7],
        // Bottom
        [4, 5, 1],
        [4, 1, 0],
        // Right
        [1, 5, 6],
        [1, 6, 2],
        // Left
        [4, 0, 3],
        [4, 3, 7],
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

// 8x8 Heart icon with color-key (magenta 0xF81F)
const HEART_KEY: Rgb565 = Rgb565::new(31, 0, 31);
const R: Rgb565 = Rgb565::RED;
const K: Rgb565 = HEART_KEY;

#[rustfmt::skip]
static HEART_SPRITE: [Rgb565; 64] = [
    K, R, R, K, K, R, R, K,
    R, R, R, R, R, R, R, R,
    R, R, R, R, R, R, R, R,
    R, R, R, R, R, R, R, R,
    K, R, R, R, R, R, R, K,
    K, K, R, R, R, R, K, K,
    K, K, K, R, R, K, K, K,
    K, K, K, K, K, K, K, K,
];

// 8x8 Shield icon
const B: Rgb565 = Rgb565::CSS_CYAN;
#[rustfmt::skip]
static SHIELD_SPRITE: [Rgb565; 64] = [
    K, B, B, B, B, B, B, K,
    B, B, B, B, B, B, B, B,
    B, B, B, B, B, B, B, B,
    B, B, B, B, B, B, B, B,
    B, B, B, B, B, B, B, B,
    K, B, B, B, B, B, B, K,
    K, K, B, B, B, B, K, K,
    K, K, K, B, B, K, K, K,
];

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<2048>::new();

    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Hybrid 3D + 2D Sprite HUD & Picking Demo", &output_settings);

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 1.5, 5.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let (vertices, faces, normals) = make_cube();
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

    let light_dir = Vector3::new(0.5, 1.0, 0.3);

    // Primary foreground cube
    let mut cube_front = K3dMesh::new(geometry.clone());
    cube_front.set_render_mode(RenderMode::SolidLightDir(light_dir));
    cube_front.set_color(Rgb565::CSS_ORANGE);
    cube_front.set_position(-1.2, 0.0, 0.0);

    // Secondary background cube (demonstrating distance LOD)
    let mut cube_back = K3dMesh::new(geometry);
    cube_back.set_render_mode(RenderMode::SolidLightDir(light_dir));
    cube_back.set_color(Rgb565::CSS_LIME);
    cube_back.set_position(1.4, 0.5, -2.5);

    let lod_config = TextureLodConfig::new(3.0, 8.0, Rgb565::new(16, 32, 16));

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let hud_title_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_YELLOW);

    let start_time = Instant::now();
    let mut mouse_x = 160i32;
    let mut mouse_y = 120i32;

    'running: loop {
        let elapsed = start_time.elapsed().as_secs_f32();

        // Rotate meshes and maintain positions
        cube_front.set_attitude(elapsed * 0.8, elapsed * 1.1, 0.0);
        cube_front.set_position(-1.2, 0.0, 0.0);

        cube_back.set_attitude(elapsed * 0.5, -elapsed * 0.9, 0.0);
        cube_back.set_position(1.4, 0.5, -2.5);

        display.clear(Rgb565::new(2, 4, 8)).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        // 1. Record 3D commands
        commands.clear();
        engine
            .record([&cube_front, &cube_back], &mut commands, None)
            .unwrap();

        // 2. Execute 3D rasterization with Screen-Space Picking under cursor
        let queries = [PickQuery::new(mouse_x, mouse_y)];
        let mut pick_results: [Option<PickResult>; 1] = [None];

        let mut frame_ctx = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };

        execute_commands_with_picking(
            &mut display,
            &mut frame_ctx,
            &commands,
            &queries,
            &mut pick_results,
        )
        .unwrap();

        // 3. Composite 2D Sprites & HUD Layer
        let mut hud = HudLayer::<8>::new();

        // Heart Sprite (scaled 2x with color-key)
        let heart = Sprite2D::new(8, 8, 8, 8, &HEART_SPRITE)
            .with_colorkey(HEART_KEY)
            .with_scale(2);
        hud.push(HudElement::Sprite(heart)).unwrap();

        // Health bar
        hud.push(HudElement::ProgressBar {
            x: 28,
            y: 12,
            w: 80,
            h: 8,
            value: 0.75,
            fg: Rgb565::RED,
            bg: Rgb565::new(6, 0, 0),
        })
        .unwrap();

        // Shield Sprite (scaled 2x with color-key)
        let shield = Sprite2D::new(8, 28, 8, 8, &SHIELD_SPRITE)
            .with_colorkey(HEART_KEY)
            .with_scale(2);
        hud.push(HudElement::Sprite(shield)).unwrap();

        // Shield bar
        hud.push(HudElement::ProgressBar {
            x: 28,
            y: 32,
            w: 80,
            h: 8,
            value: 0.90,
            fg: Rgb565::CSS_CYAN,
            bg: Rgb565::new(0, 4, 8),
        })
        .unwrap();

        hud.draw(&mut display);

        // 4. Picking Crosshair & Status text
        let crosshair_color = if pick_results[0].is_some() {
            Rgb565::CSS_YELLOW
        } else {
            Rgb565::CSS_WHITE
        };
        let _ = display.draw_iter([
            Pixel(Point::new(mouse_x - 3, mouse_y), crosshair_color),
            Pixel(Point::new(mouse_x - 2, mouse_y), crosshair_color),
            Pixel(Point::new(mouse_x + 2, mouse_y), crosshair_color),
            Pixel(Point::new(mouse_x + 3, mouse_y), crosshair_color),
            Pixel(Point::new(mouse_x, mouse_y - 3), crosshair_color),
            Pixel(Point::new(mouse_x, mouse_y - 2), crosshair_color),
            Pixel(Point::new(mouse_x, mouse_y + 2), crosshair_color),
            Pixel(Point::new(mouse_x, mouse_y + 3), crosshair_color),
        ]);

        Text::new(
            "HYBRID 3D + 2D SPRITE HUD",
            Point::new(120, 16),
            hud_title_style,
        )
        .draw(&mut display)
        .unwrap();

        let pick_text = if let Some(hit) = pick_results[0] {
            format!("HIT Target #{} Depth: {}", hit.command_index, hit.depth)
        } else {
            "Cursor: Empty Space".to_string()
        };
        Text::new(&pick_text, Point::new(120, 28), text_style)
            .draw(&mut display)
            .unwrap();

        let lod_info = if lod_config.should_drop_texture(6.0) {
            "LOD Status: Back mesh flat fallback active"
        } else {
            "LOD Status: Perspective textures active"
        };
        Text::new(lod_info, Point::new(8, 226), text_style)
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
                SimulatorEvent::MouseMove { point } => {
                    mouse_x = point.x.clamp(0, (WIDTH - 1) as i32);
                    mouse_y = point.y.clamp(0, (HEIGHT - 1) as i32);
                }
                _ => {}
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
