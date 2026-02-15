//! Rolling Ball Physics Demo
//!
//! A simple introduction to physics simulation showing a ball
//! rolling down a ramp with realistic friction and gravity.
//!
//! This is a great starting point for understanding:
//! - Basic rigid body dynamics
//! - Gravity simulation
//! - Friction coefficients
//! - Angular velocity from rolling motion
//!
//! Controls:
//! - SPACE: Reset ball to top of ramp
//! - 1-3: Change friction (low/medium/high)
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::physics::{Collider, PhysicsWorld, RigidBody, sync_body_to_mesh};
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

const BALL_RADIUS: f32 = 0.5;
const RAMP_ANGLE: f32 = 0.3; // radians (~17 degrees)

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));
    let output_settings = OutputSettingsBuilder::new().scale(1).build();
    let mut window = Window::new(
        "Rolling Ball - SPACE=reset 1-3=friction ESC=exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(5.0, 5.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    let (vertices, faces, normals) = make_sphere(16);

    let mut physics = PhysicsWorld::<16, 8>::new();
    physics.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    physics.solver_iterations = 8;

    // Ball starting position (top of ramp)
    let initial_pos = Vector3::new(-6.0, 5.0, 0.0);
    let mut current_friction = 0.5;

    let ball = RigidBody::new(1.0)
        .with_position(initial_pos)
        .with_collider(Collider::Sphere { radius: BALL_RADIUS })
        .with_restitution(0.4)
        .with_friction(current_friction)
        .with_damping(0.01)
        .with_inertia_sphere(BALL_RADIUS)
        .with_angular_damping(0.01);

    let ball_id = physics.add_body(ball).unwrap();

    let ball_geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &normals,
        uvs: &[],
        texture_id: None,
    };
    let mut ball_mesh = K3dMesh::new(ball_geometry);
    ball_mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    ball_mesh.set_color(Rgb565::CSS_ORANGE);
    ball_mesh.set_position(initial_pos.x, initial_pos.y, initial_pos.z);

    // Create angled ramp
    let ramp_rotation = UnitQuaternion::from_axis_angle(
        &nalgebra::Unit::new_normalize(Vector3::z_axis().into_inner()),
        RAMP_ANGLE,
    );

    let ramp_pos = Vector3::new(0.0, 2.0, 0.0);
    let ramp_half_extents = Vector3::new(8.0, 0.2, 3.0);

    let mut ramp_body = RigidBody::new_static()
        .with_position(ramp_pos)
        .with_collider(Collider::Aabb { half_extents: ramp_half_extents })
        .with_restitution(0.3)
        .with_friction(0.6);
    ramp_body.orientation = ramp_rotation;

    let _ramp_id = physics.add_body(ramp_body).unwrap();

    // Ramp visual mesh
    let hw = ramp_half_extents.x;
    let hh = ramp_half_extents.y;
    let hd = ramp_half_extents.z;

    let ramp_verts: Vec<[f32; 3]> = vec![
        [-hw, -hh, hd], [hw, -hh, hd], [hw, hh, hd], [-hw, hh, hd],
        [-hw, -hh, -hd], [hw, -hh, -hd], [hw, hh, -hd], [-hw, hh, -hd],
    ];
    let ramp_faces: Vec<[usize; 3]> = vec![
        [0, 1, 2], [0, 2, 3], // front
        [5, 4, 7], [5, 7, 6], // back
        [3, 2, 6], [3, 6, 7], // top
        [4, 5, 1], [4, 1, 0], // bottom
        [1, 5, 6], [1, 6, 2], // right
        [4, 0, 3], [4, 3, 7], // left
    ];
    let ramp_normals: Vec<[f32; 3]> = vec![
        [0.0, 0.0, 1.0], [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0], [0.0, 0.0, -1.0],
        [0.0, 1.0, 0.0], [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0], [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0], [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0], [-1.0, 0.0, 0.0],
    ];

    let ramp_geometry = Geometry {
        vertices: &ramp_verts,
        faces: &ramp_faces,
        colors: &[],
        lines: &[],
        normals: &ramp_normals,
        uvs: &[],
        texture_id: None,
    };
    let mut ramp_mesh = K3dMesh::new(ramp_geometry);
    ramp_mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, 1.0, 0.3)));
    ramp_mesh.set_color(Rgb565::CSS_GRAY);
    ramp_mesh.set_position(ramp_pos.x, ramp_pos.y, ramp_pos.z);
    ramp_mesh.set_rotation(ramp_rotation);

    // Floor at bottom
    let _floor_id = physics.add_body(
        RigidBody::new_static()
            .with_position(Vector3::new(0.0, -0.1, 0.0))
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(15.0, 0.1, 10.0),
            })
            .with_restitution(0.3)
            .with_friction(0.6)
    ).unwrap();

    let floor_verts: Vec<[f32; 3]> = vec![
        [-12.0, 0.0, 8.0], [12.0, 0.0, 8.0],
        [12.0, 0.0, -8.0], [-12.0, 0.0, -8.0],
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

    println!("Rolling Ball Demo");
    println!("Watch the ball roll down the ramp with realistic friction!");
    println!("SPACE = reset position");
    println!("1 = low friction (0.2)");
    println!("2 = medium friction (0.5)");
    println!("3 = high friction (0.9)");
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
                        let body = physics.body_mut(ball_id).unwrap();
                        body.position = initial_pos;
                        body.velocity = Vector3::zeros();
                        body.orientation = UnitQuaternion::identity();
                        body.angular_velocity = Vector3::zeros();
                    }
                    Keycode::Num1 | Keycode::Kp1 => {
                        current_friction = 0.2;
                        let body = physics.body_mut(ball_id).unwrap();
                        body.friction = current_friction;
                        println!("Friction set to LOW (0.2)");
                    }
                    Keycode::Num2 | Keycode::Kp2 => {
                        current_friction = 0.5;
                        let body = physics.body_mut(ball_id).unwrap();
                        body.friction = current_friction;
                        println!("Friction set to MEDIUM (0.5)");
                    }
                    Keycode::Num3 | Keycode::Kp3 => {
                        current_friction = 0.9;
                        let body = physics.body_mut(ball_id).unwrap();
                        body.friction = current_friction;
                        println!("Friction set to HIGH (0.9)");
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        physics.step_fixed::<16>(dt, 4);

        let ball_body = physics.body(ball_id).unwrap();
        sync_body_to_mesh(ball_body, &mut ball_mesh);

        display.clear(Rgb565::BLACK).unwrap();

        let all_meshes = vec![&ball_mesh, &ramp_mesh, &floor_mesh];
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

        let friction_text = format!("Friction: {:.1}", current_friction);
        Text::new(&friction_text, Point::new(10, 30), text_style)
            .draw(&mut display)
            .unwrap();

        Text::new("SPACE=reset 1-3=friction", Point::new(10, 470), text_style)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        thread::sleep(Duration::from_millis(16));
    }
}
