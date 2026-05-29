//! Screenshot capture utility
//!
//! Renders one frame per scene and saves to assets/ as PNG or GIF.
//! Run with: cargo run --example screenshots --features std

use std::f32::consts::PI;
use std::fs::File;
use std::time::Instant;

use embedded_3dgfx::DrawPrimitive;
use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::draw::{
    DitherConfig, FogConfig, draw_zbuffered, draw_zbuffered_with_effects,
    draw_zbuffered_with_textures,
};
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::physics::{Collider, PhysicsWorld, RigidBody, sync_body_to_mesh};
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::softbody::SoftBody;
use embedded_3dgfx::texture::{Texture, TextureManager};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::prelude::Primitive;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay};
use load_stl::embed_stl;
use nalgebra::{Point3, Vector3};

fn save(display: &SimulatorDisplay<Rgb565>, path: &str) {
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    display
        .to_rgb_output_image(&output_settings)
        .save_png(path)
        .unwrap();
    println!("Saved {path}");
}

fn frame_rgb(display: &SimulatorDisplay<Rgb565>) -> Vec<u8> {
    let settings = OutputSettingsBuilder::new().scale(1).build();
    let oi = display.to_rgb_output_image(&settings);
    oi.as_image_buffer().into_raw().to_vec()
}

fn save_gif(frames: &mut [Vec<u8>], width: u16, height: u16, path: &str, delay_cs: u16) {
    let mut file = File::create(path).unwrap();
    let mut encoder = gif::Encoder::new(&mut file, width, height, &[]).unwrap();
    encoder.set_repeat(gif::Repeat::Infinite).unwrap();
    for data in frames.iter_mut() {
        let mut frame = gif::Frame::from_rgb_speed(width, height, data, 10);
        frame.delay = delay_cs;
        encoder.write_frame(&frame).unwrap();
    }
    println!("Saved {path}");
}

// Draws a two-line stats panel in the top-left corner of the display.
// line1 (white): scene name + triangle count.
// line2 (yellow): timing.
fn draw_hud(display: &mut SimulatorDisplay<Rgb565>, line1: &str, line2: &str) {
    let bg_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::new(0, 0, 0))
        .build();
    Rectangle::new(Point::new(0, 0), Size::new(210, 28))
        .into_styled(bg_style)
        .draw(display)
        .unwrap();
    Text::new(
        line1,
        Point::new(4, 10),
        MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE),
    )
    .draw(display)
    .unwrap();
    Text::new(
        line2,
        Point::new(4, 22),
        MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_YELLOW),
    )
    .draw(display)
    .unwrap();
}

fn main() {
    // PNGs
    capture_wireframe_cube();
    capture_blinn_phong();
    capture_physics();
    capture_cloth();
    capture_gouraud();
    capture_fog_dither();
    capture_newtons_cradle();
    capture_texture();
    // GIFs
    gif_rotating_cube();
    gif_suzanne();
    gif_bouncing_balls();
    gif_cloth();
    gif_newtons_cradle();
    // Perf
    benchmark();
    println!("All assets saved to assets/");
}

// ── Scene 1: wireframe rotating cube ────────────────────────────────────────

fn capture_wireframe_cube() {
    const W: usize = 640;
    const H: usize = 480;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let mut zbuffer = vec![u32::MAX; W * H];
    let mut commands = CommandBuffer::<4096>::new();
    let mut engine = K3dengine::new(640, 480);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 1.2, 3.5));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let vertices: &[[f32; 3]] = &[
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
    ];
    let faces: &[[usize; 3]] = &[
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

    let geometry = Geometry {
        vertices,
        faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut cube = K3dMesh::new(geometry);
    cube.set_render_mode(RenderMode::Lines);
    cube.set_color(Rgb565::CSS_CYAN);
    cube.set_attitude(0.5, 1.0, 0.3);

    display.clear(Rgb565::BLACK).unwrap();
    zbuffer.fill(u32::MAX);
    engine
        .record(std::iter::once(&cube), &mut commands, None)
        .unwrap();
    let mut frame = FrameCtx {
        zbuffer: &mut zbuffer,
        width: W,
        height: H,
    };
    engine
        .execute::<_, 4096>(&mut display, &mut frame, &commands, None)
        .unwrap();

    save(&display, "assets/screenshot_wireframe.png");
}

// ── Scene 2: Blinn-Phong shading on STL models ──────────────────────────────
//
// Camera at -Z looks toward +Z. Models face -Z (toward camera) via attitude(-PI/2, yaw, 0).
// Light_dir convention: adjusted.z = -light_dir.z, so light_dir.z > 0 illuminates -Z-facing normals.

fn capture_blinn_phong() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut zbuffer = vec![u32::MAX; 800 * 600];
    let mut commands = CommandBuffer::<8192>::new();

    let mut engine = K3dengine::new(800, 600);
    engine.clear_caps();
    // Camera at +Z. attitude(-PI/2, yaw, 0) rotates Suzanne's -Y face to +Z, facing this camera.
    engine.camera.set_position(Point3::new(0.0, 0.8, 8.0));
    engine.camera.set_target(Point3::new(0.0, -0.3, 0.0));

    // Light from camera side: light_dir.z < 0 → adjusted.z > 0 → illuminates +Z-facing normals.
    let light_dir = Vector3::new(-0.35, 0.75, -0.85).normalize();

    // Suzanne – the classic computer graphics test model
    let mut suzanne = K3dMesh::new(embed_stl!("examples/3d_models/Suzanne.stl"));
    suzanne.set_color(Rgb565::new(26, 30, 4)); // warm amber — classic Suzanne showcase colour
    suzanne.set_scale(2.8);
    suzanne.set_position(0.0, 0.0, 0.0);
    suzanne.set_attitude(-PI / 2.0, 0.4, 0.0);
    suzanne.set_render_mode(RenderMode::BlinnPhong {
        light_dir,
        specular_intensity: 1.5,
        shininess: 56.0,
    });

    display.clear(Rgb565::BLACK).unwrap();
    zbuffer.fill(u32::MAX);

    engine
        .record(std::iter::once(&suzanne), &mut commands, None)
        .unwrap();
    let mut frame = FrameCtx {
        zbuffer: &mut zbuffer,
        width: 800,
        height: 600,
    };
    engine
        .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
        .unwrap();

    save(&display, "assets/screenshot_blinnphong.png");
}

// ── Scene 3: physics simulation – bouncing balls ─────────────────────────────

/// UV sphere with `lat` latitude bands and `lon` longitude segments.
fn make_uv_sphere(lat: usize, lon: usize) -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let r = 0.5f32;
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut faces: Vec<[usize; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();

    // north pole
    verts.push([0.0, r, 0.0]);

    for i in 1..lat {
        let phi = PI * i as f32 / lat as f32; // 0..PI
        let y = r * phi.cos();
        let ring_r = r * phi.sin();
        for j in 0..lon {
            let theta = 2.0 * PI * j as f32 / lon as f32;
            verts.push([ring_r * theta.cos(), y, ring_r * theta.sin()]);
        }
    }

    // south pole
    verts.push([0.0, -r, 0.0]);

    let south = verts.len() - 1;

    // north cap
    for j in 0..lon {
        let a = 1 + j;
        let b = 1 + (j + 1) % lon;
        let nx = (verts[a][0] + verts[b][0]) / (2.0 * r);
        let ny = (verts[a][1] + verts[b][1]) / (2.0 * r);
        let nz = (verts[a][2] + verts[b][2]) / (2.0 * r);
        faces.push([0, a, b]);
        normals.push([nx, ny, nz]);
    }

    // middle bands
    for i in 0..lat - 2 {
        let row = 1 + i * lon;
        let next_row = 1 + (i + 1) * lon;
        for j in 0..lon {
            let a = row + j;
            let b = row + (j + 1) % lon;
            let c = next_row + j;
            let d = next_row + (j + 1) % lon;
            let cn = |v: [f32; 3]| [v[0] / r, v[1] / r, v[2] / r];
            faces.push([a, b, c]);
            normals.push(cn(verts[a]));
            faces.push([b, d, c]);
            normals.push(cn(verts[b]));
        }
    }

    // south cap
    let last_row = 1 + (lat - 2) * lon;
    for j in 0..lon {
        let a = last_row + j;
        let b = last_row + (j + 1) % lon;
        let nx = (verts[a][0] + verts[b][0]) / (2.0 * r);
        let ny = (verts[a][1] + verts[b][1]) / (2.0 * r);
        let nz = (verts[a][2] + verts[b][2]) / (2.0 * r);
        faces.push([south, b, a]);
        normals.push([nx, ny, nz]);
    }

    (verts, faces, normals)
}

fn capture_physics() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut commands = CommandBuffer::<16384>::new();

    let mut engine = K3dengine::new(640, 480);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 5.0, 7.0));
    engine.camera.set_target(Point3::new(0.0, 2.5, 0.0));

    let (sphere_verts, sphere_faces, sphere_normals) = make_uv_sphere(10, 16);

    const NUM_BALLS: usize = 5;
    const BALL_RADIUS: f32 = 0.7;

    let mut physics = PhysicsWorld::<16, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    let restitutions = [0.95f32, 0.75, 0.50, 0.25, 0.05];
    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_BLUE,
    ];

    let spacing = 2.0f32;
    let start_x = -(NUM_BALLS as f32 - 1.0) * spacing * 0.5;

    let mut ball_ids = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();

    for i in 0..NUM_BALLS {
        let x = start_x + i as f32 * spacing;
        // Stagger drop heights so they're at different bounce heights in the screenshot
        let height = 5.0 + i as f32 * 0.8;

        let ball = RigidBody::new(1.0)
            .with_position(Vector3::new(x, height, 0.0))
            .with_collider(Collider::Sphere {
                radius: BALL_RADIUS,
            })
            .with_restitution(restitutions[i])
            .with_friction(0.3)
            .with_damping(0.01)
            .with_inertia_sphere(BALL_RADIUS)
            .with_angular_damping(0.02);

        let id = physics.add_body(ball).unwrap();
        ball_ids.push(id);

        let geometry = Geometry {
            vertices: &sphere_verts,
            faces: &sphere_faces,
            colors: &[],
            lines: &[],
            normals: &sphere_normals,
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
        mesh.set_color(colors[i]);
        mesh.set_position(x, height, 0.0);
        meshes.push(mesh);
    }

    physics
        .add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, -0.1, 0.0))
                .with_collider(Collider::Aabb {
                    half_extents: Vector3::new(15.0, 0.1, 10.0),
                })
                .with_restitution(1.0)
                .with_friction(0.5),
        )
        .unwrap();

    let floor_verts: &[[f32; 3]] = &[
        [-12.0, 0.0, 5.0],
        [12.0, 0.0, 5.0],
        [12.0, 0.0, -5.0],
        [-12.0, 0.0, -5.0],
    ];
    let floor_faces: &[[usize; 3]] = &[[0, 1, 2], [0, 2, 3]];
    let floor_normals: &[[f32; 3]] = &[[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
    let floor_geom = Geometry {
        vertices: floor_verts,
        faces: floor_faces,
        colors: &[],
        lines: &[],
        normals: floor_normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut floor_mesh = K3dMesh::new(floor_geom);
    floor_mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    floor_mesh.set_color(Rgb565::new(8, 16, 8));

    // Simulate to get balls mid-bounce (varying heights based on restitution)
    let dt = 1.0f32 / 60.0;
    for _ in 0..100 {
        physics.step_fixed::<16>(dt, 4);
        for (i, &id) in ball_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            sync_body_to_mesh(body, &mut meshes[i]);
        }
    }

    display.clear(Rgb565::BLACK).unwrap();
    zbuffer.fill(u32::MAX);

    let all_meshes: Vec<&K3dMesh> = meshes.iter().chain(std::iter::once(&floor_mesh)).collect();
    engine
        .record(all_meshes.iter().copied(), &mut commands, None)
        .unwrap();
    let mut frame = FrameCtx {
        zbuffer: &mut zbuffer,
        width: 640,
        height: 480,
    };
    engine
        .execute::<_, 16384>(&mut display, &mut frame, &commands, None)
        .unwrap();

    save(&display, "assets/screenshot_physics.png");
}

// ── Scene 4: cloth soft-body simulation ──────────────────────────────────────

fn capture_cloth() {
    // 640×480 native — large enough to see the cloth clearly
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let mut engine = K3dengine::new(640, 480);
    engine.clear_caps();
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut commands = CommandBuffer::<8192>::new();
    // Slightly elevated side view so the hanging cloth reads as 3-D
    engine.camera.set_position(Point3::new(1.0, 3.5, 7.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    // SoftBody<N=64, S=256>: 8×8=64 particles, within capacity
    let cloth_w = 8usize;
    let cloth_h = 8usize;

    let mut cloth = SoftBody::<64, 256>::create_cloth(cloth_w, cloth_h, 0.45, 180.0, 1.2).unwrap();

    // Raise the cloth so it hangs into frame after simulation
    for p in cloth.particles.iter_mut() {
        p.position.y += 4.0;
        p.previous_position.y += 4.0;
    }
    cloth.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    // Build triangle faces for the cloth grid
    let mut faces = Vec::new();
    for y in 0..cloth_h - 1 {
        for x in 0..cloth_w - 1 {
            let tl = y * cloth_w + x;
            let tr = y * cloth_w + (x + 1);
            let bl = (y + 1) * cloth_w + x;
            let br = (y + 1) * cloth_w + (x + 1);
            faces.push([tl, tr, bl]);
            faces.push([tr, br, bl]);
        }
    }

    // Simulate ~2 seconds so the cloth hangs and settles
    for _ in 0..120 {
        cloth.step(0.016);
    }

    let mut verts = vec![[0.0f32; 3]; cloth.particles.len()];
    cloth.get_vertex_positions(&mut verts);

    let geometry = Geometry {
        vertices: &verts,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Lines);
    mesh.set_color(Rgb565::CSS_CYAN);

    display.clear(Rgb565::BLACK).unwrap();
    zbuffer.fill(u32::MAX);
    engine
        .record(std::iter::once(&mesh), &mut commands, None)
        .unwrap();
    let mut frame = FrameCtx {
        zbuffer: &mut zbuffer,
        width: 640,
        height: 480,
    };
    engine
        .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
        .unwrap();

    save(&display, "assets/screenshot_cloth.png");
}

// ── Scene 5: Gouraud shading – per-vertex color interpolation ────────────────

fn capture_gouraud() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut zbuffer = vec![u32::MAX; 800 * 600];
    let mut engine = K3dengine::new(800, 600);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 2.5, 9.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let pyramid_vertices: &[[f32; 3]] = &[
        [0.0, 1.5, 0.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, -1.0, -1.0],
        [-1.0, -1.0, -1.0],
    ];
    let pyramid_faces: &[[usize; 3]] = &[
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 1],
        [1, 4, 3],
        [1, 3, 2],
    ];
    let pyramid_colors = [
        Rgb565::CSS_WHITE,
        Rgb565::CSS_RED,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_BLUE,
        Rgb565::CSS_YELLOW,
    ];
    let pyramid_geom = Geometry {
        vertices: pyramid_vertices,
        faces: pyramid_faces,
        colors: &pyramid_colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut pyramid = K3dMesh::new(pyramid_geom);
    pyramid.set_position(-3.0, -0.5, 0.0);
    pyramid.set_scale(1.5);
    pyramid.set_attitude(0.3, 1.2, 0.0);

    let cube_vertices: &[[f32; 3]] = &[
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let cube_faces: &[[usize; 3]] = &[
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
    let cube_colors = [
        Rgb565::new(0, 0, 10),
        Rgb565::new(31, 0, 0),
        Rgb565::new(31, 63, 0),
        Rgb565::new(0, 63, 0),
        Rgb565::new(0, 0, 31),
        Rgb565::new(31, 0, 31),
        Rgb565::CSS_WHITE,
        Rgb565::new(0, 63, 31),
    ];
    let cube_geom = Geometry {
        vertices: cube_vertices,
        faces: cube_faces,
        colors: &cube_colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut cube = K3dMesh::new(cube_geom);
    cube.set_position(3.0, 0.0, 0.0);
    cube.set_attitude(0.5, 0.7, 0.3);

    display.clear(Rgb565::BLACK).unwrap();
    zbuffer.fill(u32::MAX);

    for mesh in [&pyramid as &K3dMesh, &cube] {
        let dist = (mesh.get_position() - engine.camera.position).norm();
        let geom = mesh.select_lod(dist);
        for face in geom.faces {
            if let Some([p1, p2, p3]) = engine.transform_points(
                face,
                geom.vertices,
                engine.camera.vp_matrix * mesh.model_matrix,
            ) {
                let colors = if !geom.colors.is_empty() {
                    [
                        geom.colors[face[0]],
                        geom.colors[face[1]],
                        geom.colors[face[2]],
                    ]
                } else {
                    [mesh.color; 3]
                };
                draw_zbuffered(
                    DrawPrimitive::GouraudTriangleWithDepth {
                        points: [p1.xy(), p2.xy(), p3.xy()],
                        depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                        colors,
                    },
                    &mut display,
                    &mut zbuffer,
                    800,
                );
            }
        }
    }

    save(&display, "assets/screenshot_gouraud.png");
}

// ── Scene 6: Fog + Bayer dithering ──────────────────────────────────────────

fn capture_fog_dither() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut zbuffer = vec![u32::MAX; 800 * 600];
    let mut engine = K3dengine::new(800, 600);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 3.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let cube_vertices: &[[f32; 3]] = &[
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let cube_faces: &[[usize; 3]] = &[
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
    let cube_colors = [
        Rgb565::new(0, 0, 10),
        Rgb565::new(31, 0, 0),
        Rgb565::new(31, 63, 0),
        Rgb565::new(0, 63, 0),
        Rgb565::new(0, 0, 31),
        Rgb565::new(31, 0, 31),
        Rgb565::CSS_WHITE,
        Rgb565::new(0, 63, 31),
    ];
    let base_geom = Geometry {
        vertices: cube_vertices,
        faces: cube_faces,
        colors: &cube_colors,
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let cubes: Vec<K3dMesh> = (0..7)
        .map(|i| {
            let mut c = K3dMesh::new(base_geom.clone());
            let x = (i % 3) as f32 * 3.0 - 3.0;
            let y = (i / 3) as f32 * 3.0 - 1.5;
            let z = -(i as f32) * 4.0;
            c.set_position(x, y, z);
            let off = i as f32 * 0.3;
            c.set_attitude(0.75 + off, 1.05 + off, 0.45 + off);
            c
        })
        .collect();

    let fog_color = Rgb565::new(4, 8, 12);
    let fog = Some(FogConfig::new(fog_color, 5.0, 28.0));
    let dither = Some(DitherConfig::new(40u8));

    display.clear(fog_color).unwrap();
    zbuffer.fill(u32::MAX);

    for mesh in &cubes {
        let dist = (mesh.get_position() - engine.camera.position).norm();
        let geom = mesh.select_lod(dist);
        for face in geom.faces {
            if let Some([p1, p2, p3]) = engine.transform_points(
                face,
                geom.vertices,
                engine.camera.vp_matrix * mesh.model_matrix,
            ) {
                let colors = if !geom.colors.is_empty() {
                    [
                        geom.colors[face[0]],
                        geom.colors[face[1]],
                        geom.colors[face[2]],
                    ]
                } else {
                    [mesh.color; 3]
                };
                draw_zbuffered_with_effects(
                    DrawPrimitive::GouraudTriangleWithDepth {
                        points: [p1.xy(), p2.xy(), p3.xy()],
                        depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                        colors,
                    },
                    &mut display,
                    &mut zbuffer,
                    800,
                    fog.as_ref(),
                    dither.as_ref(),
                );
            }
        }
    }

    // Suppress unused-variable warning: cubes is consumed by the iterator above.
    let _ = &cubes;
    save(&display, "assets/screenshot_fog_dither.png");
}

// ── Scene 7: Newton's cradle ──────────────────────────────────────────────────

fn capture_newtons_cradle() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let mut zbuffer = vec![u32::MAX; 640 * 480];
    let mut engine = K3dengine::new(640, 480);
    engine.clear_caps();

    const NUM: usize = 5;
    const RADIUS: f32 = 0.5;
    const CHAIN: f32 = 5.0;
    const SPACING: f32 = RADIUS * 2.0 + 0.01;

    let start_x = -(NUM as f32 - 1.0) * SPACING * 0.5;
    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_CYAN,
    ];
    let light = Vector3::new(0.5, 1.0, 0.3);

    let (sv, sf, sn) = make_uv_sphere(10, 16);

    // Pre-compute sphere anchor positions for the string mesh
    let sphere_positions: Vec<(f32, f32)> = (0..NUM)
        .map(|i| {
            let x = start_x + i as f32 * SPACING;
            match i {
                0 => {
                    let a = 45.0_f32.to_radians();
                    (x - CHAIN * a.sin(), CHAIN - CHAIN * a.cos())
                }
                4 => {
                    let a = 18.0_f32.to_radians();
                    (x + CHAIN * a.sin(), CHAIN - CHAIN * a.cos())
                }
                _ => (x, 0.0),
            }
        })
        .collect();

    // String + frame mesh: 5 anchor vertices + 5 sphere vertices
    let mut frame_verts: Vec<[f32; 3]> = Vec::new();
    for i in 0..NUM {
        let ax = start_x + i as f32 * SPACING;
        frame_verts.push([ax, CHAIN, 0.0]);
    }
    for &(sx, sy) in &sphere_positions {
        frame_verts.push([sx, sy, 0.0]);
    }
    let frame_lines: Vec<[usize; 2]> = (0..NUM)
        .map(|i| [i, NUM + i])
        .chain(std::iter::once([0, NUM - 1])) // top bar
        .collect();

    let frame_geom = Geometry {
        vertices: &frame_verts,
        faces: &[],
        colors: &[],
        lines: &frame_lines,
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut frame_mesh = K3dMesh::new(frame_geom);
    frame_mesh.set_render_mode(RenderMode::Lines);
    frame_mesh.set_color(Rgb565::new(20, 40, 20));

    // Sphere meshes
    let sphere_meshes: Vec<K3dMesh> = (0..NUM)
        .map(|i| {
            let geom = Geometry {
                vertices: &sv,
                faces: &sf,
                colors: &[],
                lines: &[],
                normals: &sn,
                vertex_normals: &[],
                uvs: &[],
                texture_id: None,
            };
            let mut m = K3dMesh::new(geom);
            m.set_render_mode(RenderMode::SolidLightDir(light));
            m.set_color(colors[i]);
            let (sx, sy) = sphere_positions[i];
            m.set_position(sx, sy, 0.0);
            m
        })
        .collect();

    // Camera centred on the scene: sphere0 at x≈-4.6,y≈1.5; sphere4 at x≈3.6,y≈0.3
    engine.camera.set_position(Point3::new(-0.5, 3.5, 14.0));
    engine.camera.set_target(Point3::new(-0.5, 2.0, 0.0));

    display.clear(Rgb565::BLACK).unwrap();
    zbuffer.fill(u32::MAX);

    let mut commands = CommandBuffer::<16384>::new();
    let mut all_meshes: Vec<&K3dMesh> = Vec::new();
    all_meshes.push(&frame_mesh);
    all_meshes.extend(sphere_meshes.iter());
    engine
        .record(all_meshes.iter().copied(), &mut commands, None)
        .unwrap();
    let mut frame = FrameCtx {
        zbuffer: &mut zbuffer,
        width: 640,
        height: 480,
    };
    engine
        .execute::<_, 16384>(&mut display, &mut frame, &commands, None)
        .unwrap();

    save(&display, "assets/screenshot_newtons_cradle.png");
}

// ── Scene 8: UV texture mapping ───────────────────────────────────────────────

fn capture_texture() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut zbuffer = vec![u32::MAX; 800 * 600];
    let mut engine = K3dengine::new(800, 600);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 2.0, 8.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Textures ─────────────────────────────────────────────────────────────────
    let mut tex_mgr = TextureManager::<4>::new();

    static CB: [Rgb565; 64] = {
        let mut d = [Rgb565::new(0, 0, 0); 64];
        let mut i = 0usize;
        while i < 64 {
            let x = i % 8;
            let y = i / 8;
            d[i] = if (x + y) % 2 == 0 {
                Rgb565::new(0, 0, 0)
            } else {
                Rgb565::new(31, 63, 31)
            };
            i += 1;
        }
        d
    };
    static BRICK: [Rgb565; 256] = {
        let mut d = [Rgb565::new(20, 8, 4); 256];
        let mut i = 0usize;
        while i < 256 {
            let x = i % 16;
            let y = i / 16;
            if y % 4 == 0 || (y % 8 < 4 && x == 7) || (y % 8 >= 4 && x == 15) {
                d[i] = Rgb565::new(12, 12, 12);
            }
            i += 1;
        }
        d
    };
    static GRAD: [Rgb565; 64] = {
        let mut d = [Rgb565::new(0, 0, 0); 64];
        let mut i = 0usize;
        while i < 64 {
            let x = (i % 8) as u8;
            let y = (i / 8) as u8;
            d[i] = Rgb565::new(x * 4, y * 8, (7 - x) * 4);
            i += 1;
        }
        d
    };

    let id_cb = tex_mgr.add_texture(Texture::new(&CB, 8, 8)).unwrap();
    let id_brick = tex_mgr.add_texture(Texture::new(&BRICK, 16, 16)).unwrap();
    let id_grad = tex_mgr.add_texture(Texture::new(&GRAD, 8, 8)).unwrap();

    // Cube geometry ────────────────────────────────────────────────────────────
    let cv: &[[f32; 3]] = &[
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let cf: &[[usize; 3]] = &[
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
    let cu: &[[f32; 2]] = &[
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];

    let make_cube = |tex_id, pos: (f32, f32, f32), att: (f32, f32, f32)| {
        let geom = Geometry {
            vertices: cv,
            faces: cf,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: cu,
            texture_id: Some(tex_id),
        };
        let mut m = K3dMesh::new(geom);
        m.set_position(pos.0, pos.1, pos.2);
        m.set_attitude(att.0, att.1, att.2);
        m.set_scale(1.2);
        m
    };

    let cubes = [
        make_cube(id_cb, (-3.0, 0.0, 0.0), (0.5, 1.0, 0.3)),
        make_cube(id_brick, (0.0, 0.0, 0.0), (0.8, 0.6, 0.2)),
        make_cube(id_grad, (3.0, 0.0, 0.0), (0.3, 0.9, 0.5)),
    ];

    display.clear(Rgb565::new(5, 10, 15)).unwrap();
    zbuffer.fill(u32::MAX);

    for mesh in &cubes {
        let dist = (mesh.get_position() - engine.camera.position).norm();
        let geom = mesh.select_lod(dist);
        for face in geom.faces {
            if let Some([p1, p2, p3]) = engine.transform_points(
                face,
                geom.vertices,
                engine.camera.vp_matrix * mesh.model_matrix,
            ) {
                if let Some(tid) = geom.texture_id {
                    if !geom.uvs.is_empty() {
                        let uvs = [geom.uvs[face[0]], geom.uvs[face[1]], geom.uvs[face[2]]];
                        draw_zbuffered_with_textures(
                            DrawPrimitive::TexturedTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                ws: [1.0, 1.0, 1.0],
                                uvs,
                                texture_id: tid,
                            },
                            &mut display,
                            &mut zbuffer,
                            800,
                            &tex_mgr,
                            None,
                            None,
                        );
                    }
                }
            }
        }
    }

    save(&display, "assets/screenshot_texture.png");
}

// ── GIF 1: rotating wireframe cube ────────────────────────────────────────────

fn gif_rotating_cube() {
    const W: u16 = 320;
    const H: u16 = 240;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
    let mut engine = K3dengine::new(W, H);
    engine.clear_caps();
    let mut zbuffer = vec![u32::MAX; W as usize * H as usize];
    let mut commands = CommandBuffer::<4096>::new();
    engine.camera.set_position(Point3::new(0.0, 1.0, 3.5));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let vertices: &[[f32; 3]] = &[
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
    ];
    let faces: &[[usize; 3]] = &[
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
    let geom = Geometry {
        vertices,
        faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut cube = K3dMesh::new(geom);
    cube.set_render_mode(RenderMode::Lines);
    cube.set_color(Rgb565::CSS_CYAN);

    let total = 72u32;
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
    let mut avg_ms = 0.0f64;
    for i in 0..total {
        let yaw = i as f32 * (2.0 * PI / total as f32);
        cube.set_attitude(0.3, yaw, 0.1);
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);
        let t = Instant::now();
        engine
            .record(std::iter::once(&cube), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: W as usize,
            height: H as usize,
        };
        engine
            .execute::<_, 4096>(&mut display, &mut frame, &commands, None)
            .unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        avg_ms = if i == 0 { ms } else { 0.2 * ms + 0.8 * avg_ms };
        draw_hud(
            &mut display,
            "wireframe cube | 12 tris",
            &format!("render {avg_ms:.2}ms"),
        );
        frames.push(frame_rgb(&display));
    }
    save_gif(&mut frames, W, H, "assets/gif_cube.gif", 4);
}

// ── GIF 2: Blinn-Phong Suzanne rotating ──────────────────────────────────────

fn gif_suzanne() {
    const W: u16 = 320;
    const H: u16 = 240;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
    let mut zbuffer = vec![u32::MAX; W as usize * H as usize];
    let mut commands = CommandBuffer::<8192>::new();
    let mut engine = K3dengine::new(W, H);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 0.8, 8.0));
    engine.camera.set_target(Point3::new(0.0, -0.3, 0.0));
    let light_dir = Vector3::new(-0.35, 0.75, -0.85).normalize();

    let mut suzanne = K3dMesh::new(embed_stl!("examples/3d_models/Suzanne.stl"));
    suzanne.set_color(Rgb565::new(26, 30, 4));
    suzanne.set_scale(2.8);
    suzanne.set_position(0.0, 0.0, 0.0);
    suzanne.set_render_mode(RenderMode::BlinnPhong {
        light_dir,
        specular_intensity: 1.5,
        shininess: 56.0,
    });
    let suzanne_tris = embed_stl!("examples/3d_models/Suzanne.stl").faces.len();

    let total = 60u32;
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
    let mut avg_ms = 0.0f64;
    for i in 0..total {
        let yaw = i as f32 * (2.0 * PI / total as f32);
        suzanne.set_attitude(-PI / 2.0, yaw, 0.0);

        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);
        let t = Instant::now();
        engine
            .record(std::iter::once(&suzanne), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: W as usize,
            height: H as usize,
        };
        engine
            .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
            .unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        avg_ms = if i == 0 { ms } else { 0.2 * ms + 0.8 * avg_ms };
        draw_hud(
            &mut display,
            &format!("Blinn-Phong | {suzanne_tris} tris"),
            &format!("render {avg_ms:.2}ms"),
        );
        frames.push(frame_rgb(&display));
    }
    save_gif(&mut frames, W, H, "assets/gif_suzanne.gif", 5);
}

// ── GIF 3: bouncing balls physics ────────────────────────────────────────────

fn gif_bouncing_balls() {
    const W: u16 = 320;
    const H: u16 = 240;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
    let mut zbuffer = vec![u32::MAX; W as usize * H as usize];
    let mut commands = CommandBuffer::<16384>::new();
    let mut engine = K3dengine::new(W, H);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 5.0, 7.0));
    engine.camera.set_target(Point3::new(0.0, 2.5, 0.0));

    let (sv, sf, sn) = make_uv_sphere(10, 16);

    const NUM_BALLS: usize = 5;
    const BALL_RADIUS: f32 = 0.7;
    let mut physics = PhysicsWorld::<16, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    let restitutions = [0.95f32, 0.75, 0.50, 0.25, 0.05];
    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_BLUE,
    ];
    let spacing = 2.0f32;
    let start_x = -(NUM_BALLS as f32 - 1.0) * spacing * 0.5;

    let mut ball_ids = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    for i in 0..NUM_BALLS {
        let x = start_x + i as f32 * spacing;
        let height = 5.0 + i as f32 * 0.8;
        let ball = RigidBody::new(1.0)
            .with_position(Vector3::new(x, height, 0.0))
            .with_collider(Collider::Sphere {
                radius: BALL_RADIUS,
            })
            .with_restitution(restitutions[i])
            .with_friction(0.3)
            .with_damping(0.01)
            .with_inertia_sphere(BALL_RADIUS)
            .with_angular_damping(0.02);
        ball_ids.push(physics.add_body(ball).unwrap());
        let geom = Geometry {
            vertices: &sv,
            faces: &sf,
            colors: &[],
            lines: &[],
            normals: &sn,
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut m = K3dMesh::new(geom);
        m.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
        m.set_color(colors[i]);
        m.set_position(x, height, 0.0);
        meshes.push(m);
    }
    physics
        .add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, -0.1, 0.0))
                .with_collider(Collider::Aabb {
                    half_extents: Vector3::new(15.0, 0.1, 10.0),
                })
                .with_restitution(1.0)
                .with_friction(0.5),
        )
        .unwrap();

    let fv: &[[f32; 3]] = &[
        [-12.0, 0.0, 5.0],
        [12.0, 0.0, 5.0],
        [12.0, 0.0, -5.0],
        [-12.0, 0.0, -5.0],
    ];
    let ff: &[[usize; 3]] = &[[0, 1, 2], [0, 2, 3]];
    let fn_: &[[f32; 3]] = &[[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
    let fgeom = Geometry {
        vertices: fv,
        faces: ff,
        colors: &[],
        lines: &[],
        normals: fn_,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut floor = K3dMesh::new(fgeom);
    floor.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    floor.set_color(Rgb565::new(8, 16, 8));

    let dt = 1.0f32 / 60.0;
    let sphere_tris = sf.len();
    let total_tris = NUM_BALLS * sphere_tris + 2; // +2 for floor quad
    let total = 120u32;
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
    let (mut avg_sim, mut avg_render) = (0.0f64, 0.0f64);
    for i in 0..total {
        let ts = Instant::now();
        physics.step_fixed::<16>(dt, 4);
        for (j, &id) in ball_ids.iter().enumerate() {
            sync_body_to_mesh(physics.body(id).unwrap(), &mut meshes[j]);
        }
        let sim_ms = ts.elapsed().as_secs_f64() * 1000.0;

        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);
        let all: Vec<&K3dMesh> = meshes.iter().chain(std::iter::once(&floor)).collect();
        let tr = Instant::now();
        engine
            .record(all.iter().copied(), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: W as usize,
            height: H as usize,
        };
        engine
            .execute::<_, 16384>(&mut display, &mut frame, &commands, None)
            .unwrap();
        let render_ms = tr.elapsed().as_secs_f64() * 1000.0;

        avg_sim = if i == 0 {
            sim_ms
        } else {
            0.2 * sim_ms + 0.8 * avg_sim
        };
        avg_render = if i == 0 {
            render_ms
        } else {
            0.2 * render_ms + 0.8 * avg_render
        };
        draw_hud(
            &mut display,
            &format!("rigid body | {NUM_BALLS} bodies {total_tris} tris"),
            &format!("sim {avg_sim:.2}ms  render {avg_render:.2}ms"),
        );
        frames.push(frame_rgb(&display));
    }
    save_gif(&mut frames, W, H, "assets/gif_physics.gif", 4);
}

// ── GIF 4: Newton's cradle ────────────────────────────────────────────────────

fn gif_newtons_cradle() {
    const W: u16 = 320;
    const H: u16 = 240;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
    let mut zbuffer = vec![u32::MAX; W as usize * H as usize];
    let mut commands = CommandBuffer::<16384>::new();
    let mut engine = K3dengine::new(W, H);
    engine.clear_caps();
    engine.camera.set_position(Point3::new(0.0, 3.5, 11.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    const NUM: usize = 5;
    const RADIUS: f32 = 0.5;
    const CHAIN: f32 = 5.0;
    const SPACING: f32 = RADIUS * 2.0 + 0.01;
    let start_x = -(NUM as f32 - 1.0) * SPACING * 0.5;

    let (sv, sf, sn) = make_uv_sphere(10, 16);
    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_CYAN,
    ];
    let light = Vector3::new(0.5, 1.0, 0.3);

    let mut physics = PhysicsWorld::<16, 16>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 20;

    let mut sphere_ids = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();

    for i in 0..NUM {
        let x = start_x + i as f32 * SPACING;

        let anchor_id = physics
            .add_body(RigidBody::new_static().with_position(Vector3::new(x, CHAIN, 0.0)))
            .unwrap();

        let sphere_id = physics
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(x, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: RADIUS })
                    .with_restitution(0.99)
                    .with_friction(0.0)
                    .with_damping(0.0)
                    .with_inertia_sphere(RADIUS)
                    .with_angular_damping(0.0),
            )
            .unwrap();
        sphere_ids.push(sphere_id);

        physics
            .add_distance_constraint(
                anchor_id,
                Vector3::zeros(),
                sphere_id,
                Vector3::zeros(),
                0.0,
            )
            .unwrap();

        let geom = Geometry {
            vertices: &sv,
            faces: &sf,
            colors: &[],
            lines: &[],
            normals: &sn,
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut m = K3dMesh::new(geom);
        m.set_render_mode(RenderMode::SolidLightDir(light));
        m.set_color(colors[i]);
        m.set_position(x, 0.0, 0.0);
        meshes.push(m);
    }

    // Pull sphere 0 back to ~40°
    {
        let body = physics.body_mut(sphere_ids[0]).unwrap();
        body.position.x -= 3.0;
        body.position.y += 2.0;
        body.velocity = Vector3::zeros();
    }

    // String + bar geometry (vertices updated per frame)
    let mut fverts: Vec<[f32; 3]> = vec![[0.0; 3]; NUM * 2];
    for i in 0..NUM {
        fverts[i] = [start_x + i as f32 * SPACING, CHAIN, 0.0];
    }
    let flines: Vec<[usize; 2]> = (0..NUM)
        .map(|i| [i, NUM + i])
        .chain(std::iter::once([0, NUM - 1]))
        .collect();

    let sphere_tris = sf.len();
    let dt = 1.0f32 / 60.0;
    let total = 150u32;
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
    let (mut avg_sim, mut avg_render) = (0.0f64, 0.0f64);

    for i in 0..total {
        let ts = Instant::now();
        physics.step_fixed::<32>(dt, 8);
        for (j, &id) in sphere_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            sync_body_to_mesh(body, &mut meshes[j]);
            fverts[NUM + j] = [body.position.x, body.position.y, body.position.z];
        }
        let sim_ms = ts.elapsed().as_secs_f64() * 1000.0;

        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        let fgeom = Geometry {
            vertices: &fverts,
            faces: &[],
            colors: &[],
            lines: &flines,
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut fmesh = K3dMesh::new(fgeom);
        fmesh.set_render_mode(RenderMode::Lines);
        fmesh.set_color(Rgb565::new(20, 40, 20));
        let mut all_meshes: Vec<&K3dMesh> = Vec::new();
        all_meshes.push(&fmesh);
        all_meshes.extend(meshes.iter());
        let tr = Instant::now();
        engine
            .record(all_meshes.iter().copied(), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: W as usize,
            height: H as usize,
        };
        engine
            .execute::<_, 16384>(&mut display, &mut frame, &commands, None)
            .unwrap();
        let render_ms = tr.elapsed().as_secs_f64() * 1000.0;

        avg_sim = if i == 0 {
            sim_ms
        } else {
            0.2 * sim_ms + 0.8 * avg_sim
        };
        avg_render = if i == 0 {
            render_ms
        } else {
            0.2 * render_ms + 0.8 * avg_render
        };
        draw_hud(
            &mut display,
            &format!(
                "constraints | {NUM} chains {tris} tris",
                tris = NUM * sphere_tris
            ),
            &format!("sim {avg_sim:.2}ms  render {avg_render:.2}ms"),
        );
        frames.push(frame_rgb(&display));
    }

    save_gif(&mut frames, W, H, "assets/gif_newtons_cradle.gif", 4);
}

// ── GIF 5: cloth simulation ───────────────────────────────────────────────────

fn gif_cloth() {
    const W: u16 = 320;
    const H: u16 = 240;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
    let mut engine = K3dengine::new(W, H);
    engine.clear_caps();
    let mut zbuffer = vec![u32::MAX; W as usize * H as usize];
    let mut commands = CommandBuffer::<8192>::new();
    engine.camera.set_position(Point3::new(1.0, 3.5, 7.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    let cloth_w = 8usize;
    let cloth_h = 8usize;
    let mut cloth = SoftBody::<64, 256>::create_cloth(cloth_w, cloth_h, 0.45, 180.0, 1.2).unwrap();
    for p in cloth.particles.iter_mut() {
        p.position.y += 4.0;
        p.previous_position.y += 4.0;
    }
    cloth.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    let mut faces: Vec<[usize; 3]> = Vec::new();
    for y in 0..cloth_h - 1 {
        for x in 0..cloth_w - 1 {
            let tl = y * cloth_w + x;
            let tr = y * cloth_w + (x + 1);
            let bl = (y + 1) * cloth_w + x;
            let br = (y + 1) * cloth_w + (x + 1);
            faces.push([tl, tr, bl]);
            faces.push([tr, br, bl]);
        }
    }

    let particles = cloth.particles.len();
    let cloth_tris = faces.len();
    let total = 90u32;
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
    let mut verts = vec![[0.0f32; 3]; particles];
    let (mut avg_sim, mut avg_render) = (0.0f64, 0.0f64);
    for i in 0..total {
        let ts = Instant::now();
        cloth.step(0.016);
        cloth.get_vertex_positions(&mut verts);
        let sim_ms = ts.elapsed().as_secs_f64() * 1000.0;

        let geom = Geometry {
            vertices: &verts,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut mesh = K3dMesh::new(geom);
        mesh.set_render_mode(RenderMode::Lines);
        mesh.set_color(Rgb565::CSS_CYAN);
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);
        let tr = Instant::now();
        engine
            .record(std::iter::once(&mesh), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: W as usize,
            height: H as usize,
        };
        engine
            .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
            .unwrap();
        let render_ms = tr.elapsed().as_secs_f64() * 1000.0;

        avg_sim = if i == 0 {
            sim_ms
        } else {
            0.2 * sim_ms + 0.8 * avg_sim
        };
        avg_render = if i == 0 {
            render_ms
        } else {
            0.2 * render_ms + 0.8 * avg_render
        };
        draw_hud(
            &mut display,
            &format!("soft body | {particles} particles {cloth_tris} tris"),
            &format!("sim {avg_sim:.2}ms  render {avg_render:.2}ms"),
        );
        frames.push(frame_rgb(&display));
    }
    save_gif(&mut frames, W, H, "assets/gif_cloth.gif", 4);
}

// ── Benchmark: measure frame times for key scenarios ─────────────────────────

fn bench_scene<F: FnMut()>(label: &str, iters: u32, mut f: F) -> (f64, f64, f64) {
    // warm up
    for _ in 0..5 {
        f();
    }
    let mut times = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t = Instant::now();
        f();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = times[0];
    let max = *times.last().unwrap();
    let avg = times.iter().sum::<f64>() / times.len() as f64;
    let p50 = times[times.len() / 2];
    println!("  {label:<40} min={min:.3}ms  p50={p50:.3}ms  avg={avg:.3}ms  max={max:.3}ms");
    (min, avg, max)
}

fn benchmark() {
    const W: usize = 320;
    const H: usize = 240;
    const ITERS: u32 = 200;

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "\n── Performance benchmark ({}×{}, {} iters, profile={}) ──",
        W, H, ITERS, profile
    );

    // --- Scene A: wireframe cube ---
    {
        let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
        let mut zbuffer = vec![u32::MAX; W * H];
        let mut commands = CommandBuffer::<4096>::new();
        let mut engine = K3dengine::new(W as u16, H as u16);
        engine.clear_caps();
        engine.camera.set_position(Point3::new(0.0, 1.0, 3.5));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let vertices: &[[f32; 3]] = &[
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
        ];
        let faces: &[[usize; 3]] = &[
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
        let geom = Geometry {
            vertices,
            faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut cube = K3dMesh::new(geom);
        cube.set_render_mode(RenderMode::Lines);
        cube.set_color(Rgb565::CSS_CYAN);

        let mut yaw = 0.0f32;
        bench_scene("wireframe cube (record+execute)", ITERS, || {
            yaw += 0.05;
            cube.set_attitude(0.3, yaw, 0.1);
            display.clear(Rgb565::BLACK).unwrap();
            zbuffer.fill(u32::MAX);
            engine
                .record(std::iter::once(&cube), &mut commands, None)
                .unwrap();
            let mut frame = FrameCtx {
                zbuffer: &mut zbuffer,
                width: W,
                height: H,
            };
            engine
                .execute::<_, 4096>(&mut display, &mut frame, &commands, None)
                .unwrap();
        });
    }

    // --- Scene B: Blinn-Phong Suzanne ---
    {
        let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
        let mut zbuffer = vec![u32::MAX; W * H];
        let mut commands = CommandBuffer::<8192>::new();
        let mut engine = K3dengine::new(W as u16, H as u16);
        engine.clear_caps();
        engine.camera.set_position(Point3::new(0.0, 0.8, 8.0));
        engine.camera.set_target(Point3::new(0.0, -0.3, 0.0));
        let light_dir = Vector3::new(-0.35, 0.75, -0.85).normalize();

        let mut yaw = 0.0f32;
        bench_scene("Blinn-Phong Suzanne (record+execute)", ITERS, || {
            yaw += 0.05;
            let mut suzanne = K3dMesh::new(embed_stl!("examples/3d_models/Suzanne.stl"));
            suzanne.set_color(Rgb565::new(26, 30, 4));
            suzanne.set_scale(2.8);
            suzanne.set_position(0.0, 0.0, 0.0);
            suzanne.set_attitude(-PI / 2.0, yaw, 0.0);
            suzanne.set_render_mode(RenderMode::BlinnPhong {
                light_dir,
                specular_intensity: 1.5,
                shininess: 56.0,
            });
            display.clear(Rgb565::BLACK).unwrap();
            zbuffer.fill(u32::MAX);
            engine
                .record(std::iter::once(&suzanne), &mut commands, None)
                .unwrap();
            let mut frame = FrameCtx {
                zbuffer: &mut zbuffer,
                width: W,
                height: H,
            };
            engine
                .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
                .unwrap();
        });
    }

    // --- Scene C: physics + 5 balls ---
    {
        let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W as u32, H as u32));
        let mut zbuffer = vec![u32::MAX; W * H];
        let mut commands = CommandBuffer::<16384>::new();
        let mut engine = K3dengine::new(W as u16, H as u16);
        engine.clear_caps();
        engine.camera.set_position(Point3::new(0.0, 5.0, 7.0));
        engine.camera.set_target(Point3::new(0.0, 2.5, 0.0));

        let (sv, sf, sn) = make_uv_sphere(10, 16);
        const NUM: usize = 5;
        const R: f32 = 0.7;
        let mut physics = PhysicsWorld::<16, 8>::new();
        physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
        let spacing = 2.0f32;
        let start_x = -(NUM as f32 - 1.0) * spacing * 0.5;
        let colors = [
            Rgb565::CSS_RED,
            Rgb565::CSS_ORANGE,
            Rgb565::CSS_YELLOW,
            Rgb565::CSS_GREEN,
            Rgb565::CSS_BLUE,
        ];
        let mut ball_ids = Vec::new();
        let mut meshes: Vec<K3dMesh> = Vec::new();
        for i in 0..NUM {
            let x = start_x + i as f32 * spacing;
            let ball = RigidBody::new(1.0)
                .with_position(Vector3::new(x, 5.0 + i as f32 * 0.8, 0.0))
                .with_collider(Collider::Sphere { radius: R })
                .with_restitution(0.8)
                .with_friction(0.3)
                .with_damping(0.01)
                .with_inertia_sphere(R)
                .with_angular_damping(0.02);
            ball_ids.push(physics.add_body(ball).unwrap());
            let geom = Geometry {
                vertices: &sv,
                faces: &sf,
                colors: &[],
                lines: &[],
                normals: &sn,
                vertex_normals: &[],
                uvs: &[],
                texture_id: None,
            };
            let mut m = K3dMesh::new(geom);
            m.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
            m.set_color(colors[i]);
            meshes.push(m);
        }
        physics
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, -0.1, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(15.0, 0.1, 10.0),
                    })
                    .with_restitution(1.0),
            )
            .unwrap();

        let fv: &[[f32; 3]] = &[
            [-12.0, 0.0, 5.0],
            [12.0, 0.0, 5.0],
            [12.0, 0.0, -5.0],
            [-12.0, 0.0, -5.0],
        ];
        let ff: &[[usize; 3]] = &[[0, 1, 2], [0, 2, 3]];
        let fn_: &[[f32; 3]] = &[[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let fgeom = Geometry {
            vertices: fv,
            faces: ff,
            colors: &[],
            lines: &[],
            normals: fn_,
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut floor = K3dMesh::new(fgeom);
        floor.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
        floor.set_color(Rgb565::new(8, 16, 8));

        let dt = 1.0f32 / 60.0;
        bench_scene("physics 5 balls + render (record+execute)", ITERS, || {
            physics.step_fixed::<16>(dt, 4);
            for (i, &id) in ball_ids.iter().enumerate() {
                sync_body_to_mesh(physics.body(id).unwrap(), &mut meshes[i]);
            }
            display.clear(Rgb565::BLACK).unwrap();
            zbuffer.fill(u32::MAX);
            let all: Vec<&K3dMesh> = meshes.iter().chain(std::iter::once(&floor)).collect();
            engine
                .record(all.iter().copied(), &mut commands, None)
                .unwrap();
            let mut frame = FrameCtx {
                zbuffer: &mut zbuffer,
                width: W,
                height: H,
            };
            engine
                .execute::<_, 16384>(&mut display, &mut frame, &commands, None)
                .unwrap();
        });
    }

    println!("── end benchmark ──\n");
}
