//! Screenshot capture utility
//!
//! Renders one frame per scene and saves to assets/ as PNG.
//! Run with: cargo run --example screenshots --features std

use std::f32::consts::PI;

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::{draw, draw_zbuffered};
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::physics::{Collider, PhysicsWorld, RigidBody, sync_body_to_mesh};
use embedded_3dgfx::softbody::SoftBody;
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


fn main() {
    capture_wireframe_cube();
    capture_blinn_phong();
    capture_physics();
    capture_cloth();
    println!("All screenshots saved to assets/");
}

// ── Scene 1: wireframe rotating cube ────────────────────────────────────────

fn capture_wireframe_cube() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let mut engine = K3dengine::new(640, 480);
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
    engine.render(std::iter::once(&cube), |prim| draw(prim, &mut display));

    save(&display, "assets/screenshot_wireframe.png");
}

// ── Scene 2: Blinn-Phong shading on STL models ──────────────────────────────
//
// Camera at -Z looks toward +Z. Models face -Z (toward camera) via attitude(-PI/2, yaw, 0).
// Light_dir convention: adjusted.z = -light_dir.z, so light_dir.z > 0 illuminates -Z-facing normals.

fn capture_blinn_phong() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));
    let mut zbuffer = vec![u32::MAX; 800 * 600];

    let mut engine = K3dengine::new(800, 600);
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

    engine.render(std::iter::once(&suzanne), |prim| {
        draw_zbuffered(prim, &mut display, &mut zbuffer, 800);
    });

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

    let mut engine = K3dengine::new(640, 480);
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
            .with_collider(Collider::Sphere { radius: BALL_RADIUS })
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
    engine.render(all_meshes.into_iter(), |prim| {
        draw_zbuffered(prim, &mut display, &mut zbuffer, 640);
    });

    save(&display, "assets/screenshot_physics.png");
}

// ── Scene 4: cloth soft-body simulation ──────────────────────────────────────

fn capture_cloth() {
    // 640×480 native — large enough to see the cloth clearly
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let mut engine = K3dengine::new(640, 480);
    // Slightly elevated side view so the hanging cloth reads as 3-D
    engine.camera.set_position(Point3::new(1.0, 3.5, 7.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    // SoftBody<N=64, S=256>: 8×8=64 particles, within capacity
    let cloth_w = 8usize;
    let cloth_h = 8usize;

    let mut cloth =
        SoftBody::<64, 256>::create_cloth(cloth_w, cloth_h, 0.45, 180.0, 1.2).unwrap();

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
    engine.render(std::iter::once(&mesh), |prim| draw(prim, &mut display));

    save(&display, "assets/screenshot_cloth.png");
}

