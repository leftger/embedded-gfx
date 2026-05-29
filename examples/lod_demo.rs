//! Level of Detail (LOD) Demonstration
//!
//! Shows how the LOD system automatically switches between mesh detail levels
//! based on distance from the camera to improve performance.
//!
//! This demo displays multiple spheres at varying distances, each with 3 LOD levels:
//! - High detail: Close to camera
//! - Medium detail: Mid-range
//! - Low detail: Far from camera
//!
//! Controls:
//! - W/S: Move camera forward/backward
//! - A/D: Turn camera left/right
//! - Q/E: Move camera up/down
//! - 1/2/3: Adjust LOD distance thresholds
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, LODLevels, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::telemetry::{ExecuteTelemetry, RecordTelemetry};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor, WebColors};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};
use std::f32::consts::PI;
use std::thread;
use std::time::Duration;
#[path = "shared/perf_hud.rs"]
mod perf_hud;

/// Generate a sphere mesh with specified detail level
fn generate_sphere(segments: usize, rings: usize) -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Generate vertices
    for ring in 0..=rings {
        let phi = (ring as f32 / rings as f32) * PI;
        for segment in 0..=segments {
            let theta = (segment as f32 / segments as f32) * 2.0 * PI;

            let x = phi.sin() * theta.cos();
            let y = phi.cos();
            let z = phi.sin() * theta.sin();

            vertices.push([x, y, z]);
        }
    }

    // Generate faces
    for ring in 0..rings {
        for segment in 0..segments {
            let current = ring * (segments + 1) + segment;
            let next = current + segments + 1;

            // First triangle
            faces.push([current, next, current + 1]);
            // Second triangle
            faces.push([current + 1, next, next + 1]);
        }
    }

    (vertices, faces)
}

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("LOD Demo - Level of Detail", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 5.0, -20.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));
    engine.camera.set_far(200.0); // Need far clipping for distant spheres

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<16384>::new();
    let mut record_telemetry = RecordTelemetry::default();
    let mut execute_telemetry = ExecuteTelemetry::default();

    // Create sphere geometries with different LOD levels
    println!("Generating sphere geometries...");
    let (high_verts, high_faces) = generate_sphere(20, 20); // 400 vertices
    let (med_verts, med_faces) = generate_sphere(10, 10); // 100 vertices
    let (low_verts, low_faces) = generate_sphere(5, 5); // 25 vertices

    println!(
        "High detail: {} vertices, {} faces",
        high_verts.len(),
        high_faces.len()
    );
    println!(
        "Medium detail: {} vertices, {} faces",
        med_verts.len(),
        med_faces.len()
    );
    println!(
        "Low detail: {} vertices, {} faces",
        low_verts.len(),
        low_faces.len()
    );

    let high_geom = Geometry {
        vertices: &high_verts,
        faces: &high_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let med_geom = Geometry {
        vertices: &med_verts,
        faces: &med_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let low_geom = Geometry {
        vertices: &low_verts,
        faces: &low_faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    // Create multiple spheres at different distances
    let mut spheres: Vec<K3dMesh> = Vec::new();

    for i in 0..15 {
        let angle = (i as f32 / 15.0) * 2.0 * PI;
        let distance = 10.0 + i as f32 * 5.0;

        let mut sphere = K3dMesh::new(high_geom);
        sphere.set_position(angle.cos() * distance, 0.0, angle.sin() * distance);
        sphere.set_scale(2.0);

        // Assign color based on distance
        let color = match i {
            0..=4 => Rgb565::CSS_RED,   // Close - will use high detail
            5..=9 => Rgb565::CSS_GREEN, // Mid - will use medium detail
            _ => Rgb565::CSS_BLUE,      // Far - will use low detail
        };
        sphere.set_color(color);
        sphere.set_render_mode(RenderMode::Solid);

        // Set LOD geometries
        let mut lod_levels = LODLevels::default();
        lod_levels.high_distance = 30.0;
        lod_levels.medium_distance = 60.0;
        sphere.set_lod(Some(med_geom), Some(low_geom), lod_levels);

        spheres.push(sphere);
    }

    // Camera state
    let mut camera_yaw = 0.0f32;
    let mut last_frame = std::time::Instant::now();
    let mut lod_high_threshold = 30.0f32;
    let mut lod_medium_threshold = 60.0f32;

    println!("\nControls:");
    println!("  W/S         - Move forward/backward");
    println!("  A/D         - Turn left/right");
    println!("  Q/E         - Move up/down");
    println!("  1/2         - Adjust high detail distance");
    println!("  3/4         - Adjust medium detail distance");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        perf.start_of_frame();

        let now = std::time::Instant::now();
        let dt = (now - last_frame).as_secs_f32().min(0.1);
        last_frame = now;

        // Movement parameters
        let move_speed = 10.0;
        let turn_speed = 1.5;

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,

                    // Camera movement
                    Keycode::W => {
                        let target_offset = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                        engine.camera.position += target_offset * move_speed * dt;
                        engine
                            .camera
                            .set_target(engine.camera.position + target_offset);
                    }
                    Keycode::S => {
                        let target_offset = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                        engine.camera.position -= target_offset * move_speed * dt;
                        engine
                            .camera
                            .set_target(engine.camera.position + target_offset);
                    }
                    Keycode::A => {
                        camera_yaw -= turn_speed * dt;
                        let target_offset = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                        engine
                            .camera
                            .set_target(engine.camera.position + target_offset);
                    }
                    Keycode::D => {
                        camera_yaw += turn_speed * dt;
                        let target_offset = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                        engine
                            .camera
                            .set_target(engine.camera.position + target_offset);
                    }
                    Keycode::Q => {
                        engine.camera.position.y += move_speed * dt;
                        let target_offset = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                        engine
                            .camera
                            .set_target(engine.camera.position + target_offset);
                    }
                    Keycode::E => {
                        engine.camera.position.y -= move_speed * dt;
                        let target_offset = Vector3::new(camera_yaw.cos(), 0.0, camera_yaw.sin());
                        engine
                            .camera
                            .set_target(engine.camera.position + target_offset);
                    }

                    // LOD threshold adjustment
                    Keycode::Num1 => {
                        lod_high_threshold = (lod_high_threshold - 5.0).max(10.0);
                        println!("High detail threshold: {:.0}", lod_high_threshold);
                    }
                    Keycode::Num2 => {
                        lod_high_threshold =
                            (lod_high_threshold + 5.0).min(lod_medium_threshold - 5.0);
                        println!("High detail threshold: {:.0}", lod_high_threshold);
                    }
                    Keycode::Num3 => {
                        lod_medium_threshold =
                            (lod_medium_threshold - 5.0).max(lod_high_threshold + 5.0);
                        println!("Medium detail threshold: {:.0}", lod_medium_threshold);
                    }
                    Keycode::Num4 => {
                        lod_medium_threshold = (lod_medium_threshold + 5.0).min(150.0);
                        println!("Medium detail threshold: {:.0}", lod_medium_threshold);
                    }

                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Update LOD thresholds for all spheres
        for sphere in &mut spheres {
            let mut levels = LODLevels::default();
            levels.high_distance = lod_high_threshold;
            levels.medium_distance = lod_medium_threshold;
            sphere.set_lod(Some(med_geom), Some(low_geom), levels);
        }

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        // Count LOD levels being used
        let mut high_count = 0;
        let mut medium_count = 0;
        let mut low_count = 0;
        let mut total_triangles = 0;
        let mut fallback_candidate_count = 0;

        // Render all spheres with Z-buffering
        for sphere in &spheres {
            let distance = (sphere.get_position() - engine.camera.position).norm();
            let geometry = sphere.select_lod(distance);

            if distance < lod_high_threshold {
                high_count += 1;
            } else if distance < lod_medium_threshold {
                medium_count += 1;
            } else {
                low_count += 1;
            }
            if distance < lod_medium_threshold {
                fallback_candidate_count += 1;
            }

            total_triangles += geometry.faces.len();
        }

        engine
            .record_render_commands_with_budget_selector_fallback(
                spheres.iter(),
                |_, sphere| {
                    let distance = (sphere.get_position() - engine.camera.position).norm();
                    distance < lod_medium_threshold
                },
                &mut commands,
                Some(&mut record_telemetry),
            )
            .unwrap();

        engine
            .execute_recorded_frame_with_telemetry::<_, 16384>(
                &mut display,
                &mut zbuffer,
                WIDTH,
                HEIGHT,
                &commands,
                Some(&mut execute_telemetry),
            )
            .unwrap();

        // Display info
        perf.print();
        let telemetry_text = perf_hud::telemetry_summary(&record_telemetry, &execute_telemetry);
        let info_text = format!(
            "{}\\nLOD Count: High={} Med={} Low={}\\nTotal Triangles: {}\\n{}\\nFallback: {} (near+mid {})\\nLOD Thresholds: High={:.0} Med={:.0}\\nCamera: [{:.1}, {:.1}, {:.1}]",
            perf.get_text(),
            high_count,
            medium_count,
            low_count,
            total_triangles,
            telemetry_text,
            if record_telemetry.fallback_used {
                "ON"
            } else {
                "OFF"
            },
            fallback_candidate_count,
            lod_high_threshold,
            lod_medium_threshold,
            engine.camera.position.x,
            engine.camera.position.y,
            engine.camera.position.z
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "W/S/A/D/Q/E: Move | 1/2: High LOD | 3/4: Med LOD | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
