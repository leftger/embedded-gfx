//! Physics demo: falling cubes with angular dynamics and constraints
//!
//! Demonstrates the physics engine:
//! - Gravity and freefall
//! - Sphere and AABB colliders with impulse-based response
//! - Angular dynamics: cubes spin on impact and from torque
//! - Coulomb friction (each cube has a different friction coefficient)
//! - Constraint joints: a chain of cubes connected by distance constraints
//! - Body deactivation via remove_body()
//! - Syncing physics position + rotation to mesh transforms
//!
//! Controls:
//! - SPACE: Apply upward impulse to all active cubes
//! - T: Apply random torque to all active cubes (spin them!)
//! - D: Deactivate/remove the first active cube
//! - R: Reset positions (reactivates all)
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
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

fn make_cube() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let vertices = vec![
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
    ];

    let faces = vec![
        [0, 1, 2], [0, 2, 3], // front
        [5, 4, 7], [5, 7, 6], // back
        [3, 2, 6], [3, 6, 7], // top
        [4, 5, 1], [4, 1, 0], // bottom
        [1, 5, 6], [1, 6, 2], // right
        [4, 0, 3], [4, 3, 7], // left
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

// 3 free-falling cubes + 3 cubes in a chain = 6 total + 1 floor
const NUM_FREE: usize = 3;
const NUM_CHAIN: usize = 3;
const NUM_CUBES: usize = NUM_FREE + NUM_CHAIN;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Physics Demo - SPACE=impulse T=torque D=deact R=reset ESC=exit",
        &output_settings,
    );

    // Create 3D engine
    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 5.0, 18.0));
    engine.camera.set_target(Point3::new(0.0, 3.0, 0.0));

    // Create cube geometry (shared by all cubes)
    let (vertices, faces, normals) = make_cube();

    // Create physics world: 16 bodies, 8 constraints
    let mut physics = PhysicsWorld::<16, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 8;

    let colors = [
        Rgb565::CSS_CYAN,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_LIME,
        Rgb565::CSS_MAGENTA,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_SALMON,
    ];

    // -- Free-falling cubes (left side) --
    let free_positions: Vec<Vector3<f32>> = (0..NUM_FREE)
        .map(|i| Vector3::new(-4.0, 5.0 + i as f32 * 2.5, 0.0))
        .collect();

    let frictions = [0.1, 0.5, 1.0];
    let initial_spins = [
        Vector3::new(1.0, 0.5, 0.0),
        Vector3::new(0.0, 1.0, 0.5),
        Vector3::new(0.5, 0.0, 1.0),
    ];

    let mut body_ids: Vec<BodyId> = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    let mut initial_positions: Vec<Vector3<f32>> = Vec::new();
    let mut initial_ang_vels: Vec<Vector3<f32>> = Vec::new();

    for i in 0..NUM_FREE {
        let half = Vector3::new(0.5, 0.5, 0.5);
        let body = RigidBody::new(1.0)
            .with_position(free_positions[i])
            .with_damping(0.02)
            .with_restitution(0.4)
            .with_friction(frictions[i % frictions.len()])
            .with_collider(Collider::Sphere { radius: 0.5 })
            .with_inertia_box(half)
            .with_angular_velocity(initial_spins[i % initial_spins.len()])
            .with_angular_damping(0.02);
        let id = physics.add_body(body).unwrap();
        body_ids.push(id);
        initial_positions.push(free_positions[i]);
        initial_ang_vels.push(initial_spins[i % initial_spins.len()]);

        let geometry = Geometry {
            vertices: &vertices, faces: &faces, colors: &[], lines: &[],
            normals: &normals, uvs: &[], texture_id: None,
        };
        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
        mesh.set_color(colors[i % colors.len()]);
        mesh.set_position(free_positions[i].x, free_positions[i].y, free_positions[i].z);
        meshes.push(mesh);
    }

    // -- Hanging chain (right side): static anchor + 3 linked cubes --
    let chain_anchor = physics.add_body(
        RigidBody::new_static()
            .with_position(Vector3::new(4.0, 12.0, 0.0)),
    ).unwrap();

    let chain_spacing = 2.0;
    let mut prev_id = chain_anchor;
    for i in 0..NUM_CHAIN {
        let y = 12.0 - (i as f32 + 1.0) * chain_spacing;
        let pos = Vector3::new(4.0, y, 0.0);
        let half = Vector3::new(0.4, 0.4, 0.4);
        let body = RigidBody::new(1.0)
            .with_position(pos)
            .with_damping(0.02)
            .with_restitution(0.3)
            .with_friction(0.5)
            .with_collider(Collider::Sphere { radius: 0.4 })
            .with_inertia_box(half)
            .with_angular_damping(0.05);
        let id = physics.add_body(body).unwrap();

        // Connect to previous link with a distance constraint
        physics.add_distance_constraint(
            prev_id, Vector3::zeros(),
            id, Vector3::zeros(),
            0.0, // compliance = rigid
        ).unwrap();

        body_ids.push(id);
        initial_positions.push(pos);
        initial_ang_vels.push(Vector3::zeros());
        prev_id = id;

        let geometry = Geometry {
            vertices: &vertices, faces: &faces, colors: &[], lines: &[],
            normals: &normals, uvs: &[], texture_id: None,
        };
        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
        mesh.set_color(colors[(NUM_FREE + i) % colors.len()]);
        mesh.set_position(pos.x, pos.y, pos.z);
        meshes.push(mesh);
    }

    // Static floor
    let _floor_id = physics
        .add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, -0.1, 0.0))
                .with_collider(Collider::Aabb {
                    half_extents: Vector3::new(10.0, 0.1, 10.0),
                })
                .with_restitution(0.5)
                .with_friction(0.6),
        )
        .unwrap();

    // Floor visual mesh
    let floor_verts: Vec<[f32; 3]> = vec![
        [-8.0, 0.0, 5.0], [8.0, 0.0, 5.0],
        [8.0, 0.0, -5.0], [-8.0, 0.0, -5.0],
    ];
    let floor_faces: Vec<[usize; 3]> = vec![[0, 1, 2], [0, 2, 3]];
    let floor_normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];

    let floor_geometry = Geometry {
        vertices: &floor_verts, faces: &floor_faces, colors: &[], lines: &[],
        normals: &floor_normals, uvs: &[], texture_id: None,
    };
    let mut floor_mesh = K3dMesh::new(floor_geometry);
    floor_mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    floor_mesh.set_color(Rgb565::new(8, 16, 8));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let dt = 1.0 / 60.0;
    let mut torque_counter: u32 = 0;

    println!("Physics Demo (angular dynamics + constraints)");
    println!("Left: free-falling cubes (friction 0.1/0.5/1.0)");
    println!("Right: chain of 3 cubes connected by distance joints");
    println!("SPACE = apply upward impulse");
    println!("T = apply torque (spin cubes)");
    println!("D = deactivate first active cube");
    println!("R = reset positions (reactivates all)");
    println!("ESC = exit");

    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        perf.start_of_frame();

        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => {
                        for &id in &body_ids {
                            let body = physics.body_mut(id).unwrap();
                            if body.active {
                                body.apply_impulse(Vector3::new(0.0, 8.0, 0.0));
                            }
                        }
                    }
                    Keycode::T => {
                        for (i, &id) in body_ids.iter().enumerate() {
                            let body = physics.body_mut(id).unwrap();
                            if body.active {
                                torque_counter = torque_counter.wrapping_add(1);
                                let angle = (torque_counter as f32 + i as f32) * 1.7;
                                body.apply_angular_impulse(Vector3::new(
                                    angle.sin() * 5.0,
                                    angle.cos() * 3.0,
                                    (angle * 0.7).sin() * 4.0,
                                ));
                            }
                        }
                    }
                    Keycode::D => {
                        for &id in &body_ids {
                            if physics.body(id).unwrap().active {
                                physics.remove_body(id);
                                break;
                            }
                        }
                    }
                    Keycode::R => {
                        for (i, &id) in body_ids.iter().enumerate() {
                            physics.set_active(id, true);
                            let body = physics.body_mut(id).unwrap();
                            body.position = initial_positions[i];
                            body.velocity = Vector3::zeros();
                            body.orientation = UnitQuaternion::identity();
                            body.angular_velocity = initial_ang_vels[i];
                        }
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Step physics (4 substeps, up to 16 contacts)
        physics.step_fixed::<16>(dt, 4);

        // Sync physics -> meshes
        for (i, &id) in body_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            if body.active {
                sync_body_to_mesh(body, &mut meshes[i]);
            } else {
                meshes[i].set_position(0.0, -100.0, 0.0);
            }
        }

        // Render
        display.clear(Rgb565::BLACK).unwrap();

        let all_meshes: Vec<&K3dMesh> = meshes.iter().chain(std::iter::once(&floor_mesh)).collect();
        engine.render(all_meshes.into_iter(), |prim| {
            draw(prim, &mut display);
        });

        // HUD
        perf.print();
        Text::new(perf.get_text(), Point::new(10, 15), text_style)
            .draw(&mut display)
            .unwrap();
        Text::new("SPC=impulse T=torque D=deact R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
