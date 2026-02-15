//! Bouncing Balls Physics Demo
//!
//! Demonstrates different restitution (bounciness) coefficients.
//! Each ball has a different restitution value, from very bouncy
//! to completely non-bouncy (inelastic).
//!
//! Watch how balls with high restitution bounce many times while
//! balls with low restitution quickly come to rest.
//!
//! Controls:
//! - SPACE: Drop all balls again
//! - R: Reset ball positions
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
use nalgebra::{Point3, Vector3};
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

const NUM_BALLS: usize = 5;
const BALL_RADIUS: f32 = 0.5;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Bouncing Balls - SPACE=drop R=reset ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 3.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 3.0, 0.0));

    let (vertices, faces, normals) = make_sphere(16);

    let mut physics = PhysicsWorld::<16, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 8;

    // Different restitution coefficients
    let restitutions = [0.95, 0.75, 0.50, 0.25, 0.05];
    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_BLUE,
    ];
    let labels = ["0.95", "0.75", "0.50", "0.25", "0.05"];

    let mut ball_ids: Vec<BodyId> = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    let mut initial_positions: Vec<Vector3<f32>> = Vec::new();

    // Create balls with different restitution
    let spacing = 2.5;
    let start_x = -(NUM_BALLS as f32 - 1.0) * spacing * 0.5;

    for i in 0..NUM_BALLS {
        let x = start_x + i as f32 * spacing;
        let pos = Vector3::new(x, 8.0, 0.0);
        initial_positions.push(pos);

        let ball = RigidBody::new(1.0)
            .with_position(pos)
            .with_collider(Collider::Sphere { radius: BALL_RADIUS })
            .with_restitution(restitutions[i])
            .with_friction(0.3)
            .with_damping(0.01)
            .with_inertia_sphere(BALL_RADIUS)
            .with_angular_damping(0.02);

        let ball_id = physics.add_body(ball).unwrap();
        ball_ids.push(ball_id);

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
        mesh.set_position(pos.x, pos.y, pos.z);
        meshes.push(mesh);
    }

    // Static floor
    let _floor_id = physics.add_body(
        RigidBody::new_static()
            .with_position(Vector3::new(0.0, -0.1, 0.0))
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(15.0, 0.1, 10.0),
            })
            .with_restitution(1.0) // Floor is perfectly elastic
            .with_friction(0.5)
    ).unwrap();

    // Floor visual mesh
    let floor_verts: Vec<[f32; 3]> = vec![
        [-12.0, 0.0, 5.0], [12.0, 0.0, 5.0],
        [12.0, 0.0, -5.0], [-12.0, 0.0, -5.0],
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

    println!("Bouncing Balls Demo");
    println!("Restitution values: {}", labels.join(", "));
    println!("SPACE = drop balls");
    println!("R = reset positions");
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
                        for (i, &id) in ball_ids.iter().enumerate() {
                            let body = physics.body_mut(id).unwrap();
                            body.position = initial_positions[i];
                            body.velocity = Vector3::zeros();
                        }
                    }
                    Keycode::R => {
                        for (i, &id) in ball_ids.iter().enumerate() {
                            let body = physics.body_mut(id).unwrap();
                            body.position = initial_positions[i];
                            body.velocity = Vector3::zeros();
                            body.angular_velocity = Vector3::zeros();
                        }
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        physics.step_fixed::<16>(dt, 4);

        for (i, &id) in ball_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            sync_body_to_mesh(body, &mut meshes[i]);
        }

        display.clear(Rgb565::BLACK).unwrap();

        let all_meshes: Vec<&K3dMesh> = meshes.iter().chain(std::iter::once(&floor_mesh)).collect();
        engine.render(all_meshes.into_iter(), |prim| {
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

        Text::new("Restitution coefficients:", Point::new(10, 30), text_style)
            .draw(&mut display)
            .unwrap();
        for i in 0..NUM_BALLS {
            let label_text = format!("Ball {}: {}", i + 1, labels[i]);
            Text::new(&label_text, Point::new(10, 45 + i as i32 * 12), text_style)
                .draw(&mut display)
                .unwrap();
        }

        Text::new("SPACE=drop R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
