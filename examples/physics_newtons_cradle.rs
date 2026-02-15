//! Newton's Cradle Physics Demo
//!
//! Demonstrates conservation of momentum and energy using
//! constraint joints and collision detection.
//!
//! Five spheres hang in a row. Pull back one or more spheres
//! and release them to see momentum transfer in action!
//!
//! Controls:
//! - 1-5: Pull back sphere 1-5
//! - SPACE: Release all pulled spheres
//! - R: Reset to neutral position
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

    // Simple icosphere approximation for visual sphere
    let radius = 0.5;
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

    // Top cap
    for i in 0..segments {
        let next = (i + 1) % segments;
        faces.push([0, i + 2, next + 2]);
        normals.push([0.0, 1.0, 0.0]);
    }

    // Bottom cap
    for i in 0..segments {
        let next = (i + 1) % segments;
        faces.push([1, next + 2, i + 2]);
        normals.push([0.0, -1.0, 0.0]);
    }

    (vertices, faces, normals)
}

const NUM_SPHERES: usize = 5;
const SPHERE_RADIUS: f32 = 0.5;
const CHAIN_LENGTH: f32 = 5.0;
const SPACING: f32 = SPHERE_RADIUS * 2.0 + 0.01;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Newton's Cradle - 1-5=pull SPACE=release R=reset ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 0.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let (vertices, faces, normals) = make_sphere(16);

    let mut physics = PhysicsWorld::<16, 16>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 20; // High iteration count for stable constraints

    let mut sphere_ids: Vec<BodyId> = Vec::new();
    let mut anchor_ids: Vec<BodyId> = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    let mut pulled_back: Vec<bool> = vec![false; NUM_SPHERES];

    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_CYAN,
    ];

    // Create hanging spheres
    let start_x = -(NUM_SPHERES as f32 - 1.0) * SPACING * 0.5;

    for i in 0..NUM_SPHERES {
        let x = start_x + i as f32 * SPACING;

        // Static anchor point above
        let anchor_pos = Vector3::new(x, CHAIN_LENGTH, 0.0);
        let anchor_id = physics.add_body(
            RigidBody::new_static()
                .with_position(anchor_pos)
        ).unwrap();
        anchor_ids.push(anchor_id);

        // Dynamic sphere
        let sphere_pos = Vector3::new(x, 0.0, 0.0);
        let sphere = RigidBody::new(1.0)
            .with_position(sphere_pos)
            .with_collider(Collider::Sphere { radius: SPHERE_RADIUS })
            .with_restitution(0.99) // Nearly perfect elastic collision
            .with_friction(0.0) // Frictionless for clean momentum transfer
            .with_damping(0.0) // No damping for conservation
            .with_inertia_sphere(SPHERE_RADIUS)
            .with_angular_damping(0.0);

        let sphere_id = physics.add_body(sphere).unwrap();
        sphere_ids.push(sphere_id);

        // Distance constraint acting as string
        physics.add_distance_constraint(
            anchor_id,
            Vector3::zeros(),
            sphere_id,
            Vector3::zeros(),
            0.0, // Rigid constraint
        ).unwrap();

        // Create visual mesh
        let geometry = Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &normals,
            uvs: &[],
            texture_id: None,
        };
        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
        mesh.set_color(colors[i]);
        mesh.set_position(sphere_pos.x, sphere_pos.y, sphere_pos.z);
        meshes.push(mesh);
    }

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let dt = 1.0 / 60.0;

    println!("Newton's Cradle Demo");
    println!("Press 1-5 to pull back a sphere");
    println!("Press SPACE to release");
    println!("Press R to reset");
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
                    Keycode::Num1 | Keycode::Kp1 => {
                        if !pulled_back[0] {
                            let body = physics.body_mut(sphere_ids[0]).unwrap();
                            body.position.x -= 3.0;
                            body.position.y += 2.0;
                            body.velocity = Vector3::zeros();
                            pulled_back[0] = true;
                        }
                    }
                    Keycode::Num2 | Keycode::Kp2 => {
                        if !pulled_back[1] {
                            let body = physics.body_mut(sphere_ids[1]).unwrap();
                            body.position.x -= 3.0;
                            body.position.y += 2.0;
                            body.velocity = Vector3::zeros();
                            pulled_back[1] = true;
                        }
                    }
                    Keycode::Num3 | Keycode::Kp3 => {
                        if !pulled_back[2] {
                            let body = physics.body_mut(sphere_ids[2]).unwrap();
                            body.position.x -= 3.0;
                            body.position.y += 2.0;
                            body.velocity = Vector3::zeros();
                            pulled_back[2] = true;
                        }
                    }
                    Keycode::Num4 | Keycode::Kp4 => {
                        if !pulled_back[3] {
                            let body = physics.body_mut(sphere_ids[3]).unwrap();
                            body.position.x += 3.0;
                            body.position.y += 2.0;
                            body.velocity = Vector3::zeros();
                            pulled_back[3] = true;
                        }
                    }
                    Keycode::Num5 | Keycode::Kp5 => {
                        if !pulled_back[4] {
                            let body = physics.body_mut(sphere_ids[4]).unwrap();
                            body.position.x += 3.0;
                            body.position.y += 2.0;
                            body.velocity = Vector3::zeros();
                            pulled_back[4] = true;
                        }
                    }
                    Keycode::Space => {
                        // Release all pulled spheres
                        pulled_back = vec![false; NUM_SPHERES];
                    }
                    Keycode::R => {
                        // Reset all spheres to neutral
                        let start_x = -(NUM_SPHERES as f32 - 1.0) * SPACING * 0.5;
                        for (i, &id) in sphere_ids.iter().enumerate() {
                            let x = start_x + i as f32 * SPACING;
                            let body = physics.body_mut(id).unwrap();
                            body.position = Vector3::new(x, 0.0, 0.0);
                            body.velocity = Vector3::zeros();
                            body.orientation = UnitQuaternion::identity();
                            body.angular_velocity = Vector3::zeros();
                        }
                        pulled_back = vec![false; NUM_SPHERES];
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Step physics
        physics.step_fixed::<32>(dt, 8);

        // Sync physics -> meshes
        for (i, &id) in sphere_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            sync_body_to_mesh(body, &mut meshes[i]);
        }

        // Render
        display.clear(Rgb565::BLACK).unwrap();

        engine.render(meshes.iter(), |prim| {
            draw(prim, &mut display);
        });

        // HUD
        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 15), text_style)
                .draw(&mut display)
                .unwrap();
        }
        Text::new("1-5=pull SPACE=release R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
