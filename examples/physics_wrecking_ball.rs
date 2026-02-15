//! Wrecking Ball Physics Demo
//!
//! A heavy wrecking ball swings on a chain to demolish a wall of boxes.
//! Demonstrates:
//! - Distance constraints for swinging motion
//! - Mass differences (heavy ball vs light boxes)
//! - Collision-based destruction
//! - Angular momentum transfer
//!
//! Controls:
//! - SPACE: Pull back and release the wrecking ball
//! - R: Reset wall and ball
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::physics::{BodyId, Collider, PhysicsWorld, RigidBody, sync_body_to_mesh};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, UnitQuaternion, Vector3};
use std::thread;
use std::time::Duration;

fn make_sphere(segments: usize) -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let mut normals = Vec::new();

    let radius = 1.0;
    vertices.push([0.0, radius, 0.0]);
    vertices.push([0.0, -radius, 0.0]);

    for i in 0..segments {
        let theta = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
        vertices.push([
            radius * theta.cos(),
            0.0,
            radius * theta.sin(),
        ]);
    }

    for i in 0..segments {
        let next = (i + 1) % segments;
        faces.push([0, i + 2, next + 2]);
        normals.push([0.0, 1.0, 0.0]);
    }

    for i in 0..segments {
        let next = (i + 1) % segments;
        faces.push([1, next + 2, i + 2]);
        normals.push([0.0, -1.0, 0.0]);
    }

    (vertices, faces, normals)
}

fn make_box() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let vertices = vec![
        [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5],
        [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5],
    ];

    let faces = vec![
        [0, 1, 2], [0, 2, 3],
        [5, 4, 7], [5, 7, 6],
        [3, 2, 6], [3, 6, 7],
        [4, 5, 1], [4, 1, 0],
        [1, 5, 6], [1, 6, 2],
        [4, 0, 3], [4, 3, 7],
    ];

    let normals = vec![
        [0.0, 0.0, 1.0], [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0], [0.0, 0.0, -1.0],
        [0.0, 1.0, 0.0], [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0], [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0], [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0], [-1.0, 0.0, 0.0],
    ];

    (vertices, faces, normals)
}

const WALL_WIDTH: usize = 4;
const WALL_HEIGHT: usize = 5;
const BOX_SIZE: f32 = 1.0;
const BALL_RADIUS: f32 = 1.0;
const CHAIN_LENGTH: f32 = 8.0;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Wrecking Ball - SPACE=swing R=reset ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(-8.0, 5.0, 20.0));
    engine.camera.set_target(Point3::new(0.0, 3.0, 0.0));

    let (sphere_verts, sphere_faces, sphere_normals) = make_sphere(16);
    let (box_verts, box_faces, box_normals) = make_box();

    let mut physics = PhysicsWorld::<64, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 12;

    let mut box_ids: Vec<BodyId> = Vec::new();
    let mut box_meshes: Vec<K3dMesh> = Vec::new();
    let mut initial_box_positions: Vec<Vector3<f32>> = Vec::new();

    // Build wall
    for row in 0..WALL_HEIGHT {
        for col in 0..WALL_WIDTH {
            let x = 5.0 + col as f32 * BOX_SIZE;
            let y = 0.5 + row as f32 * BOX_SIZE;
            let z = 0.0;
            let pos = Vector3::new(x, y, z);
            initial_box_positions.push(pos);

            let box_body = RigidBody::new(0.5)
                .with_position(pos)
                .with_collider(Collider::Aabb { half_extents: Vector3::new(0.5, 0.5, 0.5) })
                .with_restitution(0.3)
                .with_friction(0.6)
                .with_damping(0.02)
                .with_inertia_box(Vector3::new(0.5, 0.5, 0.5))
                .with_angular_damping(0.02);

            let box_id = physics.add_body(box_body).unwrap();
            box_ids.push(box_id);

            let geometry = Geometry {
                vertices: &box_verts,
                faces: &box_faces,
                colors: &[],
                lines: &[],
                normals: &box_normals,
                uvs: &[],
                texture_id: None,
            };
            let mut mesh = K3dMesh::new(geometry);
            mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
            mesh.set_color(if (row + col) % 2 == 0 {
                Rgb565::CSS_ORANGE
            } else {
                Rgb565::CSS_YELLOW
            });
            mesh.set_position(pos.x, pos.y, pos.z);
            box_meshes.push(mesh);
        }
    }

    // Wrecking ball anchor
    let anchor_pos = Vector3::new(-3.0, 10.0, 0.0);
    let anchor_id = physics.add_body(
        RigidBody::new_static()
            .with_position(anchor_pos)
    ).unwrap();

    // Wrecking ball
    let ball_pos = Vector3::new(-3.0, 10.0 - CHAIN_LENGTH, 0.0);
    let initial_ball_pos = ball_pos;

    let ball = RigidBody::new(10.0) // Heavy ball
        .with_position(ball_pos)
        .with_collider(Collider::Sphere { radius: BALL_RADIUS })
        .with_restitution(0.4)
        .with_friction(0.2)
        .with_damping(0.005)
        .with_inertia_sphere(BALL_RADIUS)
        .with_angular_damping(0.01);

    let ball_id = physics.add_body(ball).unwrap();

    // Chain constraint
    physics.add_distance_constraint(
        anchor_id,
        Vector3::zeros(),
        ball_id,
        Vector3::zeros(),
        0.0,
    ).unwrap();

    // Ball mesh
    let ball_geometry = Geometry {
        vertices: &sphere_verts,
        faces: &sphere_faces,
        colors: &[],
        lines: &[],
        normals: &sphere_normals,
        uvs: &[],
        texture_id: None,
    };
    let mut ball_mesh = K3dMesh::new(ball_geometry);
    ball_mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    ball_mesh.set_color(Rgb565::new(15, 15, 15)); // Gray
    ball_mesh.set_position(ball_pos.x, ball_pos.y, ball_pos.z);

    // Floor
    let _floor_id = physics.add_body(
        RigidBody::new_static()
            .with_position(Vector3::new(0.0, -0.1, 0.0))
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(20.0, 0.1, 10.0),
            })
            .with_restitution(0.3)
            .with_friction(0.7)
    ).unwrap();

    let floor_verts: Vec<[f32; 3]> = vec![
        [-15.0, 0.0, 8.0], [15.0, 0.0, 8.0],
        [15.0, 0.0, -8.0], [-15.0, 0.0, -8.0],
    ];
    let floor_faces: Vec<[usize; 3]> = vec![[0, 1, 2], [0, 2, 3]];
    let floor_normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];

    let floor_geometry = Geometry {
        vertices: &floor_verts,
        faces: &floor_faces,
        colors: &[],
        lines: &[],
        normals: &floor_normals,
        uvs: &[],
        texture_id: None,
    };
    let mut floor_mesh = K3dMesh::new(floor_geometry);
    floor_mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    floor_mesh.set_color(Rgb565::new(8, 16, 8));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let dt = 1.0 / 60.0;

    println!("Wrecking Ball Demo");
    println!("SPACE = pull back and swing");
    println!("R = reset");
    println!("ESC = exit");

    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => {
                        // Pull back the ball
                        let ball_body = physics.body_mut(ball_id).unwrap();
                        ball_body.position = Vector3::new(-8.0, 7.0, 0.0);
                        ball_body.velocity = Vector3::zeros();
                    }
                    Keycode::R => {
                        // Reset ball
                        let ball_body = physics.body_mut(ball_id).unwrap();
                        ball_body.position = initial_ball_pos;
                        ball_body.velocity = Vector3::zeros();
                        ball_body.orientation = UnitQuaternion::identity();
                        ball_body.angular_velocity = Vector3::zeros();

                        // Reset wall
                        for (i, &id) in box_ids.iter().enumerate() {
                            physics.set_active(id, true);
                            let body = physics.body_mut(id).unwrap();
                            body.position = initial_box_positions[i];
                            body.velocity = Vector3::zeros();
                            body.orientation = UnitQuaternion::identity();
                            body.angular_velocity = Vector3::zeros();
                        }
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        physics.step_fixed::<128>(dt, 8);

        // Update ball mesh
        let ball_body = physics.body(ball_id).unwrap();
        sync_body_to_mesh(ball_body, &mut ball_mesh);

        // Update box meshes
        for (i, &id) in box_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            if body.active {
                sync_body_to_mesh(body, &mut box_meshes[i]);
            } else {
                // Hide inactive boxes
                box_meshes[i].set_position(0.0, -100.0, 0.0);
            }
        }

        display.clear(Rgb565::BLACK).unwrap();

        let mut all_meshes: Vec<&K3dMesh> = box_meshes.iter().collect();
        all_meshes.push(&ball_mesh);
        all_meshes.push(&floor_mesh);

        engine.render(all_meshes.into_iter(), |prim| {
            draw(prim, &mut display);
        });

        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 15), text_style)
                .draw(&mut display)
                .unwrap();
        }

        Text::new("SPACE=swing R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
