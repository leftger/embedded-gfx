//! Mesh-based texture mapping via the high-level `record()`/
//! `execute_with_textures()` API.
//!
//! `examples/texture_mapping_demo.rs` shows the *low-level* path: it builds
//! its own `K3dMesh`es but bypasses `record()`/`execute()` entirely and
//! calls `transform_points`/`draw_zbuffered_with_textures` by hand, because
//! plain `execute()` silently drops textured triangles. This demo shows the
//! alternative added alongside `RenderMode::Textured`: build meshes exactly
//! like any other (`Solid`, `GouraudLightDir`, ...), `record()` them
//! normally, and call `execute_with_textures()` instead of `execute()`
//! when the batch includes a `RenderMode::Textured` mesh -- no manual
//! per-face transform/draw calls needed, and textured + flat-colored
//! meshes can be recorded together in one `record()` call.
//!
//! Controls:
//! - SPACE: toggle auto-rotation
//! - ESC: exit

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::texture::{Texture, TextureManager};
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::Point3;
use std::thread;
use std::time::{Duration, Instant};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 320;
const TEX_SIZE: u32 = 8;

/// 8x8 checkerboard, alternating orange/teal.
fn checkerboard() -> &'static [Rgb565] {
    const N: usize = (TEX_SIZE * TEX_SIZE) as usize;
    static DATA: [Rgb565; N] = {
        let mut data = [Rgb565::BLACK; N];
        let mut y = 0;
        while y < TEX_SIZE {
            let mut x = 0;
            while x < TEX_SIZE {
                let idx = (y * TEX_SIZE + x) as usize;
                data[idx] = if (x + y) % 2 == 0 {
                    Rgb565::new(31, 40, 0)
                } else {
                    Rgb565::new(0, 40, 31)
                };
                x += 1;
            }
            y += 1;
        }
        data
    };
    &DATA
}

/// Unwelded quad: 4 unique vertices so its own UVs aren't shared with any
/// neighboring geometry. Order: bottom-left, bottom-right, top-right,
/// top-left (matching UV `(0,1) (1,1) (1,0) (0,0)`).
const QUAD_VERTICES: [[f32; 3]; 4] = [
    [-1.5, -1.5, 0.0],
    [1.5, -1.5, 0.0],
    [1.5, 1.5, 0.0],
    [-1.5, 1.5, 0.0],
];
const QUAD_FACES: [[usize; 3]; 2] = [[0, 1, 2], [0, 2, 3]];
const QUAD_UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH, HEIGHT));
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new(
        "Mesh texture demo (record + execute_with_textures)",
        &output_settings,
    );

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    engine.camera.set_position(Point3::new(0.0, 0.0, 6.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));
    engine.camera.set_near_far(0.5, 20.0);

    let mut texture_manager = TextureManager::<1>::new();
    let texture_id = texture_manager
        .add_texture(Texture::new(checkerboard(), TEX_SIZE, TEX_SIZE))
        .expect("texture manager has room");

    let geometry = Geometry {
        vertices: &QUAD_VERTICES,
        faces: &QUAD_FACES,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &QUAD_UVS,
        texture_id: Some(texture_id),
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Textured);

    let mut zbuffer = vec![Z_MAX_VALUE; (WIDTH * HEIGHT) as usize];
    let mut yaw = 0.0f32;
    let mut spinning = true;
    let clock = Instant::now();
    let mut last_tick = clock.elapsed();

    'running: loop {
        let now = clock.elapsed();
        let dt = (now - last_tick).as_secs_f32();
        last_tick = now;
        if spinning {
            yaw += dt * 0.8;
        }
        mesh.set_attitude(0.0, yaw, 0.0);

        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);

        let mut commands: CommandBuffer<64> = CommandBuffer::new();
        engine
            .record(core::iter::once(&mesh), &mut commands, None)
            .expect("record");
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH as usize,
            height: HEIGHT as usize,
        };
        engine
            .execute_with_textures(&mut display, &mut frame, &commands, &texture_manager, None)
            .expect("execute_with_textures");

        window.update(&display);
        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::KeyDown {
                    keycode: Keycode::Escape,
                    ..
                } => break 'running,
                SimulatorEvent::KeyDown {
                    keycode: Keycode::Space,
                    ..
                } => spinning = !spinning,
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(16));
    }
}
