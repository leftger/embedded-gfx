//! Domino Chain Physics Demo
//!
//! Demonstrates chain reaction physics with falling dominoes.
//! Watch as one domino knocks over the next in a cascade!
//!
//! Features:
//! - Thin rectangular boxes arranged in a line
//! - Angular dynamics for realistic tipping
//! - Friction and restitution tuned for domino behavior
//!
//! Controls:
//! - SPACE: Push the first domino to start the chain reaction
//! - R: Reset all dominoes to standing position
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

fn make_domino() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    // Thin rectangular box: width=0.2, height=2.0, depth=1.0
    let hw = 0.1;  // half width
    let hh = 1.0;  // half height
    let hd = 0.5;  // half depth

    let vertices = vec![
        // Front face
        [-hw, -hh, hd], [hw, -hh, hd], [hw, hh, hd], [-hw, hh, hd],
        // Back face
        [-hw, -hh, -hd], [hw, -hh, -hd], [hw, hh, -hd], [-hw, hh, -hd],
    ];

    let faces = vec![
        // Front
        [0, 1, 2], [0, 2, 3],
        // Back
        [5, 4, 7], [5, 7, 6],
        // Top
        [3, 2, 6], [3, 6, 7],
        // Bottom
        [4, 5, 1], [4, 1, 0],
        // Right
        [1, 5, 6], [1, 6, 2],
        // Left
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

const NUM_DOMINOES: usize = 12;
const DOMINO_SPACING: f32 = 1.2;
const DOMINO_HALF_EXTENTS: Vector3<f32> = Vector3::new(0.1, 1.0, 0.5);

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Domino Chain - SPACE=push R=reset ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(-5.0, 3.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 1.0, 0.0));

    let (vertices, faces, normals) = make_domino();

    let mut physics = PhysicsWorld::<32, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 10;

    let mut domino_ids: Vec<BodyId> = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    let mut initial_positions: Vec<Vector3<f32>> = Vec::new();

    // Create domino chain
    let start_z = -(NUM_DOMINOES as f32 - 1.0) * DOMINO_SPACING * 0.5;

    for i in 0..NUM_DOMINOES {
        let z = start_z + i as f32 * DOMINO_SPACING;
        let pos = Vector3::new(0.0, 1.0, z);
        initial_positions.push(pos);

        let domino = RigidBody::new(0.5) // Light mass for easy tipping
            .with_position(pos)
            .with_collider(Collider::Aabb { half_extents: DOMINO_HALF_EXTENTS })
            .with_restitution(0.1) // Low bounce
            .with_friction(0.6) // Good friction to prevent sliding
            .with_damping(0.02)
            .with_inertia_box(DOMINO_HALF_EXTENTS)
            .with_angular_damping(0.02);

        let domino_id = physics.add_body(domino).unwrap();
        domino_ids.push(domino_id);

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

        // Alternate colors
        if i % 2 == 0 {
            mesh.set_color(Rgb565::CSS_ORANGE);
        } else {
            mesh.set_color(Rgb565::CSS_PURPLE);
        }

        mesh.set_position(pos.x, pos.y, pos.z);
        meshes.push(mesh);
    }

    // Static floor
    let _floor_id = physics.add_body(
        RigidBody::new_static()
            .with_position(Vector3::new(0.0, -0.1, 0.0))
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(10.0, 0.1, 15.0),
            })
            .with_restitution(0.3)
            .with_friction(0.7)
    ).unwrap();

    // Floor visual mesh
    let floor_verts: Vec<[f32; 3]> = vec![
        [-8.0, 0.0, 10.0], [8.0, 0.0, 10.0],
        [8.0, 0.0, -10.0], [-8.0, 0.0, -10.0],
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

    println!("Domino Chain Demo");
    println!("Press SPACE to push the first domino");
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
                    Keycode::Space => {
                        // Push the first domino with angular impulse to tip it forward
                        let first_domino = physics.body_mut(domino_ids[0]).unwrap();
                        first_domino.apply_impulse(Vector3::new(0.0, 0.0, 3.0));
                        first_domino.apply_angular_impulse(Vector3::new(5.0, 0.0, 0.0));
                    }
                    Keycode::R => {
                        // Reset all dominoes to standing position
                        for (i, &id) in domino_ids.iter().enumerate() {
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

        physics.step_fixed::<64>(dt, 8);

        for (i, &id) in domino_ids.iter().enumerate() {
            let body = physics.body(id).unwrap();
            sync_body_to_mesh(body, &mut meshes[i]);
        }

        display.clear(Rgb565::BLACK).unwrap();

        let all_meshes: Vec<&K3dMesh> = meshes.iter().chain(std::iter::once(&floor_mesh)).collect();
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

        Text::new("SPACE=push first domino R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
