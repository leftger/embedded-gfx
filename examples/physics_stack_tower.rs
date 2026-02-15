//! Stack Tower Physics Demo
//!
//! Demonstrates stability, friction, and stacking behavior.
//! Build a tower of boxes and watch physics keep it stable
//! (or tumble down if pushed too hard!)
//!
//! Features:
//! - Multiple boxes stacked vertically
//! - Realistic friction preventing sliding
//! - Angular dynamics for natural tumbling
//! - Test stability limits
//!
//! Controls:
//! - SPACE: Apply impulse to random box
//! - LEFT/RIGHT: Apply horizontal force to tower
//! - UP: Add upward impulse to top box
//! - R: Reset tower to original position
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

fn make_box() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let vertices = vec![
        [-0.5, -0.3, 0.5], [0.5, -0.3, 0.5], [0.5, 0.3, 0.5], [-0.5, 0.3, 0.5],
        [-0.5, -0.3, -0.5], [0.5, -0.3, -0.5], [0.5, 0.3, -0.5], [-0.5, 0.3, -0.5],
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

const NUM_BOXES: usize = 8;
const BOX_HALF_EXTENTS: Vector3<f32> = Vector3::new(0.5, 0.3, 0.5);
const BOX_HEIGHT: f32 = 0.6;

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Stack Tower - SPACE=impulse ARROWS=push R=reset ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(5.0, 3.0, 12.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    let (vertices, faces, normals) = make_box();

    let mut physics = PhysicsWorld::<32, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 12; // More iterations for stable stacking

    let mut box_ids: Vec<BodyId> = Vec::new();
    let mut meshes: Vec<K3dMesh> = Vec::new();
    let mut initial_positions: Vec<Vector3<f32>> = Vec::new();

    let colors = [
        Rgb565::CSS_RED,
        Rgb565::CSS_ORANGE,
        Rgb565::CSS_YELLOW,
        Rgb565::CSS_GREEN,
        Rgb565::CSS_CYAN,
        Rgb565::CSS_BLUE,
        Rgb565::CSS_PURPLE,
        Rgb565::CSS_MAGENTA,
    ];

    // Create stack of boxes
    for i in 0..NUM_BOXES {
        let y = 0.3 + i as f32 * BOX_HEIGHT;
        let pos = Vector3::new(0.0, y, 0.0);
        initial_positions.push(pos);

        let box_body = RigidBody::new(1.0)
            .with_position(pos)
            .with_collider(Collider::Aabb { half_extents: BOX_HALF_EXTENTS })
            .with_restitution(0.2) // Low bounce for stable stacking
            .with_friction(0.8) // High friction to prevent sliding
            .with_damping(0.05) // Some damping for stability
            .with_inertia_box(BOX_HALF_EXTENTS)
            .with_angular_damping(0.05);

        let box_id = physics.add_body(box_body).unwrap();
        box_ids.push(box_id);

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
        mesh.set_color(colors[i % colors.len()]);
        mesh.set_position(pos.x, pos.y, pos.z);
        meshes.push(mesh);
    }

    // Static floor
    let _floor_id = physics.add_body(
        RigidBody::new_static()
            .with_position(Vector3::new(0.0, -0.1, 0.0))
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(10.0, 0.1, 10.0),
            })
            .with_restitution(0.3)
            .with_friction(0.9)
    ).unwrap();

    // Floor visual mesh
    let floor_verts: Vec<[f32; 3]> = vec![
        [-8.0, 0.0, 5.0], [8.0, 0.0, 5.0],
        [8.0, 0.0, -5.0], [-8.0, 0.0, -5.0],
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

    println!("Stack Tower Demo");
    println!("SPACE = impulse random box");
    println!("LEFT/RIGHT = push tower horizontally");
    println!("UP = lift top box");
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
                        // Apply impulse to middle box
                        let mid_idx = NUM_BOXES / 2;
                        let body = physics.body_mut(box_ids[mid_idx]).unwrap();
                        body.apply_impulse(Vector3::new(2.0, 1.0, -1.5));
                    }
                    Keycode::Left => {
                        // Push tower left
                        let mid_idx = NUM_BOXES / 2;
                        let body = physics.body_mut(box_ids[mid_idx]).unwrap();
                        body.apply_impulse(Vector3::new(-3.0, 0.0, 0.0));
                    }
                    Keycode::Right => {
                        // Push tower right
                        let mid_idx = NUM_BOXES / 2;
                        let body = physics.body_mut(box_ids[mid_idx]).unwrap();
                        body.apply_impulse(Vector3::new(3.0, 0.0, 0.0));
                    }
                    Keycode::Up => {
                        // Lift top box
                        let top_idx = NUM_BOXES - 1;
                        let body = physics.body_mut(box_ids[top_idx]).unwrap();
                        body.apply_impulse(Vector3::new(0.0, 5.0, 0.0));
                    }
                    Keycode::R => {
                        // Reset all boxes
                        for (i, &id) in box_ids.iter().enumerate() {
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

        for (i, &id) in box_ids.iter().enumerate() {
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

        Text::new("SPC=impulse ARROWS=push R=reset", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
