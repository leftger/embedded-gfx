//! Tiny no_std binary used to measure flash impact of embedded-3dgfx features.
//!
//! ```bash
//! ./.github/scripts/measure_feature_size.sh
//! ```

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "lighting")]
use embedded_graphics_core::pixelcolor::Rgb565;
use panic_halt as _;

static VERTS: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
static FACES: [[usize; 3]; 1] = [[0, 1, 2]];
static LINES: [[usize; 2]; 1] = [[0, 1]];
#[cfg(feature = "lighting")]
static NORMALS: [[f32; 3]; 1] = [[0.0, 0.0, 1.0]];

#[entry]
fn main() -> ! {
    let mut engine = K3dengine::new(320, 240);
    let geometry = Geometry {
        vertices: &VERTS,
        faces: &FACES,
        colors: &[],
        lines: &LINES,
        #[cfg(feature = "lighting")]
        normals: &NORMALS,
        #[cfg(not(feature = "lighting"))]
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Lines);

    #[cfg(feature = "lighting")]
    {
        mesh.set_render_mode(RenderMode::SolidLightDir(nalgebra::Vector3::new(
            0.0, 1.0, 0.0,
        )));
        let _ = engine.add_point_light(embedded_3dgfx::PointLight::new(
            nalgebra::Point3::new(0.0, 1.0, 0.0),
            Rgb565::new(31, 63, 31),
            1.0,
        ));
    }

    #[cfg(feature = "full")]
    {
        // Keep optional modules reachable so LTO cannot erase their flash cost.
        let _ = core::mem::size_of::<embedded_3dgfx::texture::TextureManager<1>>();
        let _ = embedded_3dgfx::raycast::Raycaster2D::new(32, 24);
        let _ = core::mem::size_of::<embedded_3dgfx::particles::ParticleSystem<8>>();
        let _ = core::mem::size_of::<embedded_3dgfx::physics::PhysicsWorld<2>>();
        let mut buf = [0u8; 5];
        let _ = embedded_3dgfx::hud::format_u16_dec(0u16, &mut buf, 5);
    }

    let mut commands = CommandBuffer::<64>::new();
    let _ = engine.record(core::iter::once(&mesh), &mut commands, None);

    core::hint::black_box((&engine, &commands, &mesh));
    loop {
        cortex_m::asm::nop();
    }
}
