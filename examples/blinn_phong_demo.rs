//! Blinn-Phong Shading Demonstration
//!
//! Shows specular highlights using the Blinn-Phong lighting model.
//! This demo displays a rotating sphere with adjustable lighting parameters.
//!
//! Controls:
//! - Arrow Keys: Rotate light direction
//! - Q/E: Adjust shininess (specular exponent)
//! - A/D: Adjust specular intensity
//! - SPACE: Toggle auto-rotation
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use load_stl::embed_stl;
use nalgebra::{Point3, Vector3};
use std::f32::consts::PI;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Blinn-Phong Shading Demo", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(-10.0, 2.0, 0.0));
    engine.camera.set_target(Point3::new(-9.0, 2.0, 0.0));

    // Load models
    println!("Loading models...");

    // Suzanne (monkey head) - great for showing specular highlights
    let mut suzanne = K3dMesh::new(embed_stl!("examples/3d_models/Suzanne.stl"));
    suzanne.set_color(Rgb565::new(20, 40, 31)); // Cyan-ish
    suzanne.set_scale(2.0);
    suzanne.set_position(0.0, 0.7, 10.0);

    // Teapot - classic computer graphics model
    let mut teapot = K3dMesh::new(embed_stl!("examples/3d_models/Teapot_low.stl"));
    teapot.set_color(Rgb565::new(31, 20, 10)); // Gold-ish
    teapot.set_position(-10.0, 0.0, 0.0);

    // Blahaj
    let mut blahaj = K3dMesh::new(embed_stl!("examples/3d_models/blahaj.stl"));
    blahaj.set_color(Rgb565::new(105 >> 3, 150 >> 2, 173 >> 3));
    blahaj.set_position(0.0, 0.0, 0.0);

    println!("Models loaded!");

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<8192>::new();

    // Lighting parameters
    let mut light_angle_h = 0.0f32; // Horizontal angle
    let mut light_angle_v = 0.5f32; // Vertical angle
    let mut shininess = 32.0f32;
    let mut specular_intensity = 0.8f32;
    let mut auto_rotate = true;

    let start_time = Instant::now();

    println!("Controls:");
    println!("  Arrow Keys  - Rotate light direction");
    println!("  Q/E         - Adjust shininess (specular exponent)");
    println!("  A/D         - Adjust specular intensity");
    println!("  SPACE       - Toggle auto-rotation");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
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
                    Keycode::Left => {
                        light_angle_h -= 0.2;
                        println!("Light horizontal: {:.1}°", light_angle_h.to_degrees());
                    }
                    Keycode::Right => {
                        light_angle_h += 0.2;
                        println!("Light horizontal: {:.1}°", light_angle_h.to_degrees());
                    }
                    Keycode::Up => {
                        light_angle_v += 0.1;
                        light_angle_v = light_angle_v.min(PI / 2.0 - 0.1);
                        println!("Light vertical: {:.1}°", light_angle_v.to_degrees());
                    }
                    Keycode::Down => {
                        light_angle_v -= 0.1;
                        light_angle_v = light_angle_v.max(-PI / 2.0 + 0.1);
                        println!("Light vertical: {:.1}°", light_angle_v.to_degrees());
                    }
                    Keycode::Q => {
                        shininess = (shininess - 4.0).max(1.0);
                        println!("Shininess: {:.1}", shininess);
                    }
                    Keycode::E => {
                        shininess = (shininess + 4.0).min(256.0);
                        println!("Shininess: {:.1}", shininess);
                    }
                    Keycode::A => {
                        specular_intensity = (specular_intensity - 0.1).max(0.0);
                        println!("Specular intensity: {:.2}", specular_intensity);
                    }
                    Keycode::D => {
                        specular_intensity = (specular_intensity + 0.1).min(2.0);
                        println!("Specular intensity: {:.2}", specular_intensity);
                    }
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Calculate light direction
        let time = start_time.elapsed().as_secs_f32();

        let (current_h, current_v) = if auto_rotate {
            (time * 0.5, (time * 0.3).sin() * 0.5)
        } else {
            (light_angle_h, light_angle_v)
        };

        let light_dir = Vector3::new(
            current_h.cos() * current_v.cos(),
            current_v.sin(),
            current_h.sin() * current_v.cos(),
        )
        .normalize();

        // Rotate objects
        suzanne.set_attitude(-PI / 2.0, time * 0.5, 0.0);
        teapot.set_attitude(-PI / 2.0, time * 0.8, 0.0);
        blahaj.set_attitude(-PI / 2.0, time * 0.6, 0.0);

        // Set Blinn-Phong shading for all objects
        suzanne.set_render_mode(RenderMode::BlinnPhong {
            light_dir,
            specular_intensity,
            shininess,
        });
        teapot.set_render_mode(RenderMode::BlinnPhong {
            light_dir,
            specular_intensity,
            shininess,
        });
        blahaj.set_render_mode(RenderMode::BlinnPhong {
            light_dir,
            specular_intensity,
            shininess,
        });

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        engine
            .record_render_commands(
                [&suzanne, &teapot, &blahaj].iter().copied(),
                &mut commands,
            )
            .unwrap();
        engine
            .execute_recorded_frame::<_, 8192>(&mut display, &mut zbuffer, WIDTH, HEIGHT, &commands)

            .unwrap();

        // Display info
        perf.print();
        let info_text = format!(
            "{}\\nLight: [{:.2}, {:.2}, {:.2}]\\nShininess: {:.0} | Specular: {:.2}\\nAuto: {}",
            perf.get_text(),
            light_dir.x,
            light_dir.y,
            light_dir.z,
            shininess,
            specular_intensity,
            if auto_rotate { "ON" } else { "OFF" }
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "Arrows: Light | Q/E: Shininess | A/D: Specular | SPACE: Auto | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Light direction indicator (top-right corner)
        let indicator_x = 750;
        let indicator_y = 50;
        let indicator_radius = 30.0;

        // Draw background circle
        for angle in 0..360 {
            let rad = (angle as f32).to_radians();
            let px = (indicator_x as f32 + rad.cos() * indicator_radius) as i32;
            let py = (indicator_y as f32 + rad.sin() * indicator_radius) as i32;
            if px >= 0 && px < 800 && py >= 0 && py < 600 {
                display
                    .draw_iter(std::iter::once(Pixel(
                        Point::new(px, py),
                        Rgb565::new(5, 5, 5),
                    )))
                    .ok();
            }
        }

        // Draw light direction as a cross
        let light_x = indicator_x as f32 + light_dir.x * indicator_radius;
        let light_y = indicator_y as f32 - light_dir.z * indicator_radius;

        for i in -3..=3 {
            let px = (light_x as i32 + i).clamp(0, 799);
            let py = (light_y as i32).clamp(0, 599);
            display
                .draw_iter(std::iter::once(Pixel(
                    Point::new(px, py),
                    Rgb565::CSS_YELLOW,
                )))
                .ok();

            let px = (light_x as i32).clamp(0, 799);
            let py = (light_y as i32 + i).clamp(0, 599);
            display
                .draw_iter(std::iter::once(Pixel(
                    Point::new(px, py),
                    Rgb565::CSS_YELLOW,
                )))
                .ok();
        }

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
