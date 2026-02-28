//! Skeletal Animation Demo
//!
//! Demonstrates skeletal subspace deformation (skinning) with an animated arm.
//! Shows how bones influence vertex positions through weighted blending.
//!
//! Features:
//! - Hierarchical bone structure (shoulder -> elbow -> hand)
//! - Linear blend skinning (SSD)
//! - Real-time skeletal animation
//! - Smooth vertex deformation
//!
//! Controls:
//! - SPACE: Toggle animation
//! - UP/DOWN: Rotate shoulder
//! - LEFT/RIGHT: Rotate elbow
//! - R: Reset to bind pose
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::skeleton::{Skeleton, Bone, SkinningData, VertexSkinning, apply_skinning};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, UnitQuaternion, Vector3};
use std::f32::consts::PI;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(320, 240));

    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Skeletal Animation Demo", &output_settings);

    let mut engine = K3dengine::new(320, 240);
    engine.camera.set_position(Point3::new(0.0, 2.0, 8.0));
    engine.camera.set_target(Point3::new(0.0, 2.0, 0.0));

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Create skeleton
    let mut skeleton = Skeleton::<8>::new();

    // Root bone (shoulder)
    let shoulder = skeleton.add_bone(
        Bone::new("shoulder").with_position(Vector3::new(0.0, 3.0, 0.0)),
        None
    ).unwrap();

    // Elbow bone (child of shoulder)
    let elbow = skeleton.add_bone(
        Bone::new("elbow").with_position(Vector3::new(0.0, -1.5, 0.0)),
        Some(shoulder)
    ).unwrap();

    // Hand bone (child of elbow)
    let hand = skeleton.add_bone(
        Bone::new("hand").with_position(Vector3::new(0.0, -1.5, 0.0)),
        Some(elbow)
    ).unwrap();

    // Compute bind pose
    skeleton.update_transforms();
    skeleton.compute_inverse_bind_poses();

    // Create arm mesh (cylinder-like shape)
    let mut bind_vertices = Vec::new();
    let mut faces = Vec::new();
    let segments = 8; // Segments along the arm
    let sides = 8; // Sides around the cylinder

    // Create vertices along the arm
    for seg in 0..=segments {
        let y = -3.0 + (seg as f32 / segments as f32) * 3.0; // 0 to -3 (arm length)
        let radius = if seg < segments / 2 { 0.3 } else { 0.25 }; // Thicker upper arm

        for side in 0..sides {
            let angle = (side as f32 / sides as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            bind_vertices.push([x, y + 3.0, z]); // Offset to world space
        }
    }

    // Create faces (triangle strip around cylinder)
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

    // Create skinning data
    let mut skinning_data = SkinningData::new();

    for seg in 0..=segments {
        let t = seg as f32 / segments as f32;

        for _side in 0..sides {
            // Blend between shoulder, elbow, and hand based on position along arm
            let skinning = if t < 0.33 {
                // Upper arm - mostly shoulder
                let weight = 1.0 - (t / 0.33);
                VertexSkinning::two_bones(shoulder.0, weight, elbow.0, 1.0 - weight)
            } else if t < 0.66 {
                // Forearm - mostly elbow
                let local_t = (t - 0.33) / 0.33;
                let weight = 1.0 - local_t;
                VertexSkinning::two_bones(elbow.0, weight, hand.0, 1.0 - weight)
            } else {
                // Hand - mostly hand bone
                VertexSkinning::single_bone(hand.0)
            };

            skinning_data.add_vertex(skinning).unwrap();
        }
    }

    // Storage for deformed vertices
    let mut deformed_vertices = bind_vertices.clone();

    let mut animate = true;
    let mut shoulder_angle = 0.0f32;
    let mut elbow_angle = 0.0f32;
    let start_time = Instant::now();

    println!("Skeletal Animation Demo");
    println!("Controls:");
    println!("  SPACE: Toggle animation");
    println!("  UP/DOWN: Rotate shoulder");
    println!("  LEFT/RIGHT: Rotate elbow");
    println!("  R: Reset pose");
    println!("  ESC: Exit");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => animate = !animate,
                    Keycode::Up => shoulder_angle += 0.1,
                    Keycode::Down => shoulder_angle -= 0.1,
                    Keycode::Left => elbow_angle -= 0.1,
                    Keycode::Right => elbow_angle += 0.1,
                    Keycode::R => {
                        shoulder_angle = 0.0;
                        elbow_angle = 0.0;
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Animate bones
        if animate {
            let elapsed = start_time.elapsed().as_secs_f32();
            shoulder_angle = (elapsed * 0.5).sin() * 0.8;
            elbow_angle = (elapsed * 1.5).sin() * 1.2;
        }

        // Update skeleton pose
        if let Some(shoulder_bone) = skeleton.get_bone_mut(shoulder) {
            shoulder_bone.set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::z_axis(),
                shoulder_angle,
            ));
        }

        if let Some(elbow_bone) = skeleton.get_bone_mut(elbow) {
            elbow_bone.set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::z_axis(),
                elbow_angle,
            ));
        }

        skeleton.update_transforms();

        // Apply skinning to deform mesh
        apply_skinning(
            &skeleton,
            &skinning_data,
            &bind_vertices,
            &mut deformed_vertices,
        );

        // Create mesh with deformed vertices
        let geometry = Geometry {
            vertices: &deformed_vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_render_mode(RenderMode::Lines);
        mesh.set_color(Rgb565::CSS_LIME);

        // Clear and render
        display.clear(Rgb565::BLACK).unwrap();

        engine.render(std::iter::once(&mesh), |prim| {
            draw(prim, &mut display);
        });

        // Display info
        let info_text = if animate { "Animating" } else { "Paused" };
        Text::new(info_text, Point::new(10, 10), text_style)
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
