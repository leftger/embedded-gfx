//! Pendulum Physics Demo
//!
//! Demonstrates constraint-based pendulum motion using distance joints.
//! Multiple pendulums with different lengths and masses show how
//! physics constraints can simulate realistic swinging motion.
//!
//! Controls:
//! - SPACE: Apply random impulse to pendulums
//! - 1-3: Pull back specific pendulum
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

const NUM_PENDULUMS: usize = 3;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Pendulum Demo - SPACE=impulse 1-3=pull R=reset ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 5.0, 20.0));
    engine.camera.set_target(Point3::new(0.0, 5.0, 0.0));

    let (vertices, faces, normals) = make_sphere(16);

    let mut physics = PhysicsWorld::<16, 16>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 20;

    let pendulum_configs = [
        (4.0, 1.0, 0.4),  // length, mass, radius
        (6.0, 1.5, 0.5),
        (8.0, 2.0, 0.6),
    ];

    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_CYAN,
        Rgb565::CSS_YELLOW,
    ];

    let mut bob_ids: Vec<BodyId> = Vec::new();
    let mut anchor_ids: Vec<BodyId> = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    let mut initial_positions: Vec<Vector3<f32>> = Vec::new();

    let spacing = 4.0;
    let start_x = -(NUM_PENDULUMS as f32 - 1.0) * spacing * 0.5;

    for i in 0..NUM_PENDULUMS {
        let (length, mass, radius) = pendulum_configs[i];
        let x = start_x + i as f32 * spacing;

        // Static anchor point
        let anchor_pos = Vector3::new(x, 10.0, 0.0);
        let anchor_id = physics.add_body(
            RigidBody::new_static()
                .with_position(anchor_pos)
        ).unwrap();
        anchor_ids.push(anchor_id);

        // Pendulum bob
        let bob_pos = Vector3::new(x, 10.0 - length, 0.0);
        initial_positions.push(bob_pos);

        let bob = RigidBody::new(mass)
            .with_position(bob_pos)
            .with_collider(Collider::Sphere { radius })
            .with_restitution(0.3)
            .with_friction(0.1)
            .with_damping(0.005) // Very light damping for realistic swing
            .with_inertia_sphere(radius)
            .with_angular_damping(0.01);

        let bob_id = physics.add_body(bob).unwrap();
        bob_ids.push(bob_id);

        // Distance constraint (the "string")
        physics.add_distance_constraint(
            anchor_id,
            Vector3::zeros(),
            bob_id,
            Vector3::zeros(),
            0.0, // Rigid constraint
        ).unwrap();

        // Visual mesh
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
        mesh.set_scale(radius * 2.0);
        mesh.set_position(bob_pos.x, bob_pos.y, bob_pos.z);
        meshes.push(mesh);
    }

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let dt = 1.0 / 60.0;

    println!("Pendulum Demo");
    println!("Three pendulums with different lengths and masses");
    println!("SPACE = apply impulse");
    println!("1-3 = pull back pendulum");
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
                        // Apply impulse to all pendulums
                        for (i, &id) in bob_ids.iter().enumerate() {
                            let body = physics.body_mut(id).unwrap();
                            let angle = i as f32 * 1.5;
                            body.apply_impulse(Vector3::new(
                                angle.sin() * 5.0,
                                angle.cos() * 5.0,
                                (angle * 0.7).sin() * 5.0,
                            ));
                        }
                    }
                    Keycode::Num1 | Keycode::Kp1 => {
                        let body = physics.body_mut(bob_ids[0]).unwrap();
                        body.position.x -= 3.0;
                        body.velocity = Vector3::zeros();
                    }
                    Keycode::Num2 | Keycode::Kp2 => {
                        let body = physics.body_mut(bob_ids[1]).unwrap();
                        body.position.x -= 3.0;
                        body.velocity = Vector3::zeros();
                    }
                    Keycode::Num3 | Keycode::Kp3 => {
                        let body = physics.body_mut(bob_ids[2]).unwrap();
                        body.position.x -= 3.0;
                        body.velocity = Vector3::zeros();
                    }
                    Keycode::R => {
                        for (i, &id) in bob_ids.iter().enumerate() {
                            let body = physics.body_mut(id).unwrap();
                            body.position = initial_positions[i];
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

        physics.step_fixed::<32>(dt, 8);

        for (i, &id) in bob_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            sync_body_to_mesh(body, &mut meshes[i]);
        }

        display.clear(Rgb565::BLACK).unwrap();

        engine.render(meshes.iter(), |prim| {
            draw(prim, &mut display);
        });

        // Constraints are working but invisible (no line drawing yet)

        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 15), text_style)
                .draw(&mut display)
                .unwrap();
        }

        Text::new("SPACE=impulse 1-3=pull R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
