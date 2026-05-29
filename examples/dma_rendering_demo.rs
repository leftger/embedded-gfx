//! DMA Rendering Demonstration
//!
//! Shows double-buffered rendering using the swap chain abstraction.
//! Demonstrates how DMA transfers allow the CPU and display to work in parallel,
//! eliminating tearing and improving performance.
//!
//! This demo displays:
//! - Complex scene with multiple objects
//! - FPS counter with timing breakdown (render vs total frame time)
//! - Toggle between single and double buffering
//! - Performance comparison
//!
//! Controls:
//! - D: Toggle double buffering on/off
//! - SPACE: Toggle auto-rotation
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::display_backend::SimulatorBackend;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::swapchain::StandardSwapChain;
use embedded_3dgfx::telemetry::{ExecuteTelemetry, RecordTelemetry};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor, WebColors};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};
use std::thread;
use std::time::{Duration, Instant};
#[path = "shared/perf_hud.rs"]
mod perf_hud;

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    // For single-buffer mode, we'll use a SimulatorDisplay directly
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    // For double-buffer mode, we'll use a swap chain with two framebuffers
    // Leak vec to get 'static lifetime (acceptable for demo)
    let fb0_data: &'static mut [Rgb565] = vec![Rgb565::BLACK; 800 * 600].leak();
    let fb1_data: &'static mut [Rgb565] = vec![Rgb565::BLACK; 800 * 600].leak();

    let mut swap_chain = StandardSwapChain::<800, 600, _>::from_static_slices(
        fb0_data,
        fb1_data,
        false, // little endian
        SimulatorBackend::new(),
    );

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("DMA Rendering Demo - Double Buffering", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 5.0, 15.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(false);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<16384>::new();
    let mut record_telemetry = RecordTelemetry::default();
    let mut execute_telemetry = ExecuteTelemetry::default();

    // Create a cube geometry
    let cube_vertices = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    let cube_faces = [
        [0, 1, 2],
        [0, 2, 3], // Front
        [1, 5, 6],
        [1, 6, 2], // Right
        [5, 4, 7],
        [5, 7, 6], // Back
        [4, 0, 3],
        [4, 3, 7], // Left
        [3, 2, 6],
        [3, 6, 7], // Top
        [4, 5, 1],
        [4, 1, 0], // Bottom
    ];

    let cube_normals = [
        [0.0, 0.0, -1.0], // Front
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0], // Right
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0], // Back
        [0.0, 0.0, 1.0],
        [-1.0, 0.0, 0.0], // Left
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0], // Top
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0], // Bottom
        [0.0, -1.0, 0.0],
    ];

    let cube_geom = Geometry {
        vertices: &cube_vertices,
        faces: &cube_faces,
        colors: &[],
        lines: &[],
        normals: &cube_normals,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    // Create many cubes to stress test rendering
    let mut cubes: Vec<K3dMesh> = Vec::new();
    for i in 0..20 {
        let mut cube = K3dMesh::new(cube_geom);
        let angle = (i as f32 / 20.0) * std::f32::consts::PI * 2.0;
        let radius = 5.0;
        let x = angle.cos() * radius;
        let z = angle.sin() * radius;
        let y = (i as f32 - 10.0) * 0.5;

        cube.set_position(x, y, z);
        cube.set_scale(0.5);
        cube.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.5, -1.0, 0.3)));

        // Alternate colors
        if i % 3 == 0 {
            cube.color = Rgb565::CSS_RED;
        } else if i % 3 == 1 {
            cube.color = Rgb565::CSS_GREEN;
        } else {
            cube.color = Rgb565::CSS_BLUE;
        }

        cubes.push(cube);
    }

    let mut use_double_buffer = true;
    let mut auto_rotate = true;
    let start_time = Instant::now();
    let mut render_times: Vec<f32> = Vec::new();
    let mut frame_times: Vec<f32> = Vec::new();

    println!("Controls:");
    println!("  D           - Toggle double buffering");
    println!("  SPACE       - Toggle auto-rotation");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");
    println!("Double buffering: ENABLED (DMA simulation)");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        let frame_start = Instant::now();
        perf.start_of_frame();

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => {
                        auto_rotate = !auto_rotate;
                        println!("Auto-rotate: {}", if auto_rotate { "ON" } else { "OFF" });
                    }
                    Keycode::D => {
                        use_double_buffer = !use_double_buffer;
                        println!(
                            "Double buffering: {}",
                            if use_double_buffer {
                                "ENABLED"
                            } else {
                                "DISABLED"
                            }
                        );
                        render_times.clear();
                        frame_times.clear();
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Animate objects
        let time = if auto_rotate {
            start_time.elapsed().as_secs_f32()
        } else {
            0.0
        };

        for (i, cube) in cubes.iter_mut().enumerate() {
            let offset = i as f32 * 0.2;
            cube.set_attitude(
                time * 0.5 + offset,
                time * 0.7 + offset,
                time * 0.3 + offset,
            );
        }

        // Fail-soft subset: keep alternating meshes as a cheap degradation policy.
        let fallback_stride = 2usize;
        let fallback_candidate_count = cubes.len().div_ceil(fallback_stride);

        let render_start = Instant::now();

        if use_double_buffer {
            // Double-buffered path: render to back buffer
            {
                let back_buffer = swap_chain.get_back_buffer();
                back_buffer.clear(Rgb565::BLACK).unwrap();
                zbuffer.fill(u32::MAX);
                engine
                    .record_render_commands_with_budget_decimation_fallback(
                        cubes.iter(),
                        fallback_stride,
                        &mut commands,
                        Some(&mut record_telemetry),
                    )
                    .unwrap();
                engine
                    .execute_recorded_frame_with_telemetry::<_, 16384>(
                        back_buffer,
                        &mut zbuffer,
                        WIDTH,
                        HEIGHT,
                        &commands,
                        Some(&mut execute_telemetry),
                    )
                    .unwrap();

                // Copy back buffer to display for visualization BEFORE presenting
                // (In real hardware, this wouldn't be needed - DMA does it)
                // FrameBuf implements IntoIterator, so we can iterate over pixels
                for pixel in back_buffer.into_iter() {
                    display.draw_iter([pixel]).unwrap();
                }
            }

            let render_time = render_start.elapsed().as_secs_f32() * 1000.0;
            render_times.push(render_time);
            if render_times.len() > 60 {
                render_times.remove(0);
            }

            // Present: swap buffers and "transfer" via DMA (simulated)
            swap_chain.present().unwrap();
        } else {
            // Single-buffered path: render directly to display
            display.clear(Rgb565::BLACK).unwrap();
            zbuffer.fill(u32::MAX);
            engine
                .record_render_commands_with_budget_decimation_fallback(
                    cubes.iter(),
                    fallback_stride,
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

            let render_time = render_start.elapsed().as_secs_f32() * 1000.0;
            render_times.push(render_time);
            if render_times.len() > 60 {
                render_times.remove(0);
            }
        }

        let frame_time = frame_start.elapsed().as_secs_f32() * 1000.0;
        frame_times.push(frame_time);
        if frame_times.len() > 60 {
            frame_times.remove(0);
        }

        // Calculate averages
        let avg_render = if !render_times.is_empty() {
            render_times.iter().sum::<f32>() / render_times.len() as f32
        } else {
            0.0
        };

        let avg_frame = if !frame_times.is_empty() {
            frame_times.iter().sum::<f32>() / frame_times.len() as f32
        } else {
            0.0
        };

        // Display info
        perf.print();
        let telemetry_text = perf_hud::telemetry_summary(&record_telemetry, &execute_telemetry);
        let info_text = format!(
            "{}\nMode: {}\nObjects: {}\nRender time: {:.1}ms\nFrame time: {:.1}ms\n{}\nFallback: {} (subset {})\nSwap chain frames: {}\nAuto-rotate: {}",
            perf.get_text(),
            if use_double_buffer {
                "DOUBLE-BUFFER (DMA)"
            } else {
                "SINGLE-BUFFER"
            },
            cubes.len(),
            avg_render,
            avg_frame,
            telemetry_text,
            if record_telemetry.fallback_used {
                "ON"
            } else {
                "OFF"
            },
            fallback_candidate_count,
            swap_chain.frame_count(),
            if auto_rotate { "ON" } else { "OFF" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "D: Toggle buffering | SPACE: Toggle rotation | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Performance explanation
        if use_double_buffer {
            let benefit_text = "Double buffering: CPU renders while GPU transfers (parallel work)";
            Text::new(benefit_text, Point::new(10, 560), text_style)
                .draw(&mut display)
                .unwrap();
        } else {
            let penalty_text = "Single buffer: Display blocks CPU during transfer (serial work)";
            Text::new(penalty_text, Point::new(10, 560), text_style)
                .draw(&mut display)
                .unwrap();
        }

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
    println!("Final stats:");
    println!(
        "  Mode: {}",
        if use_double_buffer {
            "Double-buffer"
        } else {
            "Single-buffer"
        }
    );
    println!("  Total swap chain frames: {}", swap_chain.frame_count());
}
