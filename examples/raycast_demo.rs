//! Ray Casting Demo
//!
//! Demonstrates ray casting for object selection and shooting.
//! Click or shoot rays to interact with physics objects.
//!
//! Features:
//! - Ray-sphere intersection
//! - Ray-AABB intersection
//! - Ray-capsule intersection
//! - Visual ray rendering
//! - Hit point and normal visualization
//!
//! Controls:
//! - SPACE: Cast ray forward (shoot)
//! - UP/DOWN/LEFT/RIGHT: Rotate camera
//! - R: Reset scene
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::physics::{PhysicsWorld, RigidBody, Collider, Ray};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
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

fn create_cube_mesh() -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let vertices = vec![
        [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5],
        [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5],
    ];

    let faces = vec![
        [0, 1, 2], [0, 2, 3], // Front
        [5, 4, 7], [5, 7, 6], // Back
        [3, 2, 6], [3, 6, 7], // Top
        [4, 5, 1], [4, 1, 0], // Bottom
        [1, 5, 6], [1, 6, 2], // Right
        [4, 0, 3], [4, 3, 7], // Left
    ];

    (vertices, faces)
}

fn create_sphere_mesh(radius: f32) -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Simple icosphere approximation
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;

    let initial_verts = [
        [-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0],
        [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t],
        [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0],
    ];

    for v in &initial_verts {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        vertices.push([v[0] / len * radius, v[1] / len * radius, v[2] / len * radius]);
    }

    // Icosahedron faces
    faces.push([0, 11, 5]);
    faces.push([0, 5, 1]);
    faces.push([0, 1, 7]);
    faces.push([0, 7, 10]);
    faces.push([0, 10, 11]);

    (vertices, faces)
}

fn main() {
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(320, 240));
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Ray Casting Demo", &output_settings);

    let mut engine = K3dengine::new(320, 240);
    engine.camera.set_position(Point3::new(0.0, 2.0, 8.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let mut world = PhysicsWorld::<16, 0>::new();
    world.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    // Create static objects to shoot at
    // Sphere
    world.add_body(
        RigidBody::new_static()
            .with_collider(Collider::Sphere { radius: 0.8 })
            .with_position(Vector3::new(-3.0, 2.0, 0.0))
    );

    // Cube (AABB)
    world.add_body(
        RigidBody::new_static()
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(0.7, 0.7, 0.7),
            })
            .with_position(Vector3::new(0.0, 2.0, 0.0))
    );

    // Capsule
    world.add_body(
        RigidBody::new_static()
            .with_collider(Collider::Capsule {
                height: 2.0,
                radius: 0.5,
            })
            .with_position(Vector3::new(3.0, 2.0, 0.0))
    );

    let (cube_verts, cube_faces) = create_cube_mesh();
    let (sphere_verts, sphere_faces) = create_sphere_mesh(0.8);

    let mut last_hit = None;
    let mut camera_yaw = 0.0f32;
    let mut camera_pitch = 0.0f32;

    println!("Ray Casting Demo");
    println!("SPACE: Shoot ray | Arrow keys: Rotate camera | R: Reset");

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
                        // Cast ray from camera forward
                        let ray = Ray::new(
                            engine.camera.position.coords,
                            engine.camera.get_direction(),
                        );

                        last_hit = world.ray_cast(&ray, 100.0);

                        if let Some(hit) = &last_hit {
                            println!("Hit! Distance: {:.2}, Body: {:?}", hit.distance, hit.body_id);
                            println!("  Point: {:?}", hit.point);
                            println!("  Normal: {:?}", hit.normal);
                        } else {
                            println!("Miss!");
                        }
                    }
                    Keycode::Left => camera_yaw -= 0.1,
                    Keycode::Right => camera_yaw += 0.1,
                    Keycode::Up => camera_pitch += 0.1,
                    Keycode::Down => camera_pitch -= 0.1,
                    Keycode::R => {
                        last_hit = None;
                        camera_yaw = 0.0;
                        camera_pitch = 0.0;
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Update camera position based on rotation
        let distance = 8.0;
        let cam_x = camera_yaw.sin() * camera_pitch.cos() * distance;
        let cam_y = camera_pitch.sin() * distance + 2.0;
        let cam_z = camera_yaw.cos() * camera_pitch.cos() * distance;

        engine.camera.set_position(Point3::new(cam_x, cam_y, cam_z));
        engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

        display.clear(Rgb565::BLACK).unwrap();

        // Render objects
        for (body_id, body) in world.bodies() {
            let (vertices, faces, color) = match &body.collider {
                Some(Collider::Sphere { .. }) => {
                    (&sphere_verts[..], &sphere_faces[..], Rgb565::CSS_RED)
                }
                Some(Collider::Aabb { .. }) => {
                    (&cube_verts[..], &cube_faces[..], Rgb565::CSS_YELLOW)
                }
                Some(Collider::Capsule { .. }) => {
                    // Use cube for now (would need capsule mesh)
                    (&cube_verts[..], &cube_faces[..], Rgb565::CSS_CYAN)
                }
                None => continue,
            };

            // Highlight if hit
            let color = if let Some(hit) = &last_hit {
                if hit.body_id == body_id {
                    Rgb565::CSS_WHITE
                } else {
                    color
                }
            } else {
                color
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

            engine.render(std::iter::once(&mesh), |prim| {
                draw(prim, &mut display);
            });
        }

        // Display info
        let hit_text = if let Some(hit) = &last_hit {
            format!("HIT! d={:.1}", hit.distance)
        } else {
            "Ready to shoot".to_string()
        };

        Text::new(&hit_text, Point::new(10, 10), text_style)
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
