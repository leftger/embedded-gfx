//! Capsule Collider Physics Demo
//!
//! Demonstrates capsule colliders interacting with various shapes.
//! Capsules are excellent for characters and dynamic objects.
//!
//! Features:
//! - Capsule-sphere collisions
//! - Capsule-AABB collisions
//! - Capsule-capsule collisions
//! - Realistic tumbling and rolling behavior
//!
//! Controls:
//! - SPACE: Spawn new capsule
//! - R: Reset scene
//! - C: Add cube (AABB)
//! - S: Add sphere
//! - ESC: Exit

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::physics::{Collider, PhysicsWorld, RigidBody};
use embedded_3dgfx::renderer::FrameCtx;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};
use std::f32::consts::PI;
use std::thread;
use std::time::Duration;

fn create_capsule_mesh(
    height: f32,
    radius: f32,
    segments: usize,
) -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Create cylinder body
    let sides = 8;
    for seg in 0..=segments {
        let y = -height / 2.0 + (seg as f32 / segments as f32) * height;
        for side in 0..sides {
            let angle = (side as f32 / sides as f32) * 2.0 * PI;
            vertices.push([angle.cos() * radius, y, angle.sin() * radius]);
        }
    }

    // Add top hemisphere
    let top_center_idx = vertices.len();
    vertices.push([0.0, height / 2.0 + radius, 0.0]);

    // Add bottom hemisphere
    let bottom_center_idx = vertices.len();
    vertices.push([0.0, -height / 2.0 - radius, 0.0]);

    // Create cylinder faces
    for seg in 0..segments {
        for side in 0..sides {
            let curr = seg * sides + side;
            let next_side = seg * sides + ((side + 1) % sides);
            let next_seg = (seg + 1) * sides + side;
            let next_both = (seg + 1) * sides + ((side + 1) % sides);

            faces.push([curr, next_side, next_both]);
            faces.push([curr, next_both, next_seg]);
        }
    }

    // Add top cap faces
    for side in 0..sides {
        let curr = segments * sides + side;
        let next = segments * sides + ((side + 1) % sides);
        faces.push([curr, next, top_center_idx]);
    }

    // Add bottom cap faces
    for side in 0..sides {
        let curr = side;
        let next = (side + 1) % sides;
        faces.push([curr, bottom_center_idx, next]);
    }

    (vertices, faces)
}

fn main() {
    const WIDTH: usize = 320;
    const HEIGHT: usize = 240;
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(320, 240));
    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<4096>::new();
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Capsule Physics Demo", &output_settings);

    let mut engine = K3dengine::new(320, 240);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 3.0, 12.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let mut world = PhysicsWorld::<32, 0>::new();
    world.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    // Create floor
    world.add_body(
        RigidBody::new_static()
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(10.0, 0.5, 10.0),
            })
            .with_position(Vector3::new(0.0, -0.5, 0.0)),
    );

    // Add initial capsule
    world.add_body(
        RigidBody::new(1.0)
            .with_collider(Collider::Capsule {
                height: 2.0,
                radius: 0.4,
            })
            .with_position(Vector3::new(0.0, 5.0, 0.0))
            .with_restitution(0.3),
    );

    // Create meshes
    let (capsule_verts, capsule_faces) = create_capsule_mesh(2.0, 0.4, 6);
    let floor_verts = vec![
        [-10.0, -0.5, -10.0],
        [10.0, -0.5, -10.0],
        [10.0, -0.5, 10.0],
        [-10.0, -0.5, 10.0],
        [-10.0, 0.5, -10.0],
        [10.0, 0.5, -10.0],
        [10.0, 0.5, 10.0],
        [-10.0, 0.5, 10.0],
    ];
    let floor_faces = vec![
        [0, 1, 2],
        [0, 2, 3], // Bottom
        [4, 6, 5],
        [4, 7, 6], // Top
    ];

    println!("Capsule Physics Demo");
    println!("SPACE: Spawn capsule | C: Add cube | S: Add sphere | R: Reset");

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
                        // Spawn capsule (use simple counter for position variation)
                        let count = world.bodies().count();
                        let capsule = RigidBody::new(1.0)
                            .with_collider(Collider::Capsule {
                                height: 2.0,
                                radius: 0.4,
                            })
                            .with_position(Vector3::new(((count % 3) as f32 - 1.0) * 1.5, 8.0, 0.0))
                            .with_restitution(0.3);
                        let _ = world.add_body(capsule);
                    }
                    Keycode::C => {
                        // Add cube
                        let count = world.bodies().count();
                        let cube = RigidBody::new(2.0)
                            .with_collider(Collider::Aabb {
                                half_extents: Vector3::new(0.5, 0.5, 0.5),
                            })
                            .with_position(Vector3::new(((count % 4) as f32 - 1.5) * 1.0, 6.0, 0.0))
                            .with_restitution(0.4);
                        let _ = world.add_body(cube);
                    }
                    Keycode::S => {
                        // Add sphere
                        let count = world.bodies().count();
                        let sphere = RigidBody::new(1.0)
                            .with_collider(Collider::Sphere { radius: 0.5 })
                            .with_position(Vector3::new(((count % 5) as f32 - 2.0) * 0.8, 7.0, 0.0))
                            .with_restitution(0.6);
                        let _ = world.add_body(sphere);
                    }
                    Keycode::R => {
                        // Reset
                        world = PhysicsWorld::<32, 0>::new();
                        world.set_gravity(Vector3::new(0.0, -9.81, 0.0));
                        world.add_body(
                            RigidBody::new_static()
                                .with_collider(Collider::Aabb {
                                    half_extents: Vector3::new(10.0, 0.5, 10.0),
                                })
                                .with_position(Vector3::new(0.0, -0.5, 0.0)),
                        );
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        world.step::<32>(0.016);

        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);
        let mut frame_meshes: Vec<K3dMesh> = Vec::new();

        // Render all bodies
        for (_body_id, body) in world.bodies() {
            if !body.active {
                continue;
            }

            let (vertices, faces, color) = match &body.collider {
                Some(Collider::Capsule { .. }) => {
                    (&capsule_verts[..], &capsule_faces[..], Rgb565::CSS_LIME)
                }
                Some(Collider::Aabb { .. }) => {
                    if body.body_type == embedded_3dgfx::physics::BodyType::Static {
                        (&floor_verts[..], &floor_faces[..], Rgb565::CSS_GRAY)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

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

            let mut mesh = K3dMesh::new(geometry);
            mesh.set_position(body.position.x, body.position.y, body.position.z);
            mesh.set_render_mode(RenderMode::Lines);
            mesh.set_color(color);

            // Apply rotation
            let (roll, pitch, yaw) = body.orientation.euler_angles();
            mesh.set_attitude(roll, pitch, yaw);

            frame_meshes.push(mesh);
        }
        engine
            .record(frame_meshes.iter(), &mut commands, None)
            .unwrap();
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute::<_, 4096>(&mut display, &mut frame, &commands, None)
            .unwrap();

        Text::new(
            &format!("Bodies: {}", world.bodies().count()),
            Point::new(10, 10),
            text_style,
        )
        .draw(&mut display)
        .unwrap();

        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 230), text_style)
                .draw(&mut display)
                .unwrap();
        }

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
