//! Billboard Demonstration
//!
//! Shows billboards that always face the camera - perfect for particles and sprites.
//! This demo displays floating billboards representing particles or markers.
//!
//! Controls:
//! - W/S: Move camera forward/backward
//! - A/D: Turn camera left/right
//! - Q/E: Move camera up/down
//! - SPACE: Spawn more particles
//! - ESC: Exit

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::billboard::Billboard;
use embedded_3dgfx::draw::draw_zbuffered;
use embedded_3dgfx::mesh::Geometry;
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
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

struct Particle {
    billboard: Billboard,
    velocity: Vector3<f32>,
    lifetime: f32,
}

impl Particle {
    fn new(position: Point3<f32>, velocity: Vector3<f32>, color: Rgb565) -> Self {
        Self {
            billboard: Billboard::new(position, 0.5, color),
            velocity,
            lifetime: 5.0, // 5 seconds
        }
    }

    fn update(&mut self, dt: f32) {
        // Update position
        self.billboard.position += self.velocity * dt;

        // Apply gravity
        self.velocity.y -= 2.0 * dt;

        // Reduce lifetime
        self.lifetime -= dt;
    }

    fn is_alive(&self) -> bool {
        self.lifetime > 0.0
    }
}

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(800, 600));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Billboard Demo - Particles", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(800, 600);
    apply_default_caps(&mut engine);
    engine.camera.set_position(Point3::new(0.0, 2.0, -10.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut perf = PerformanceCounter::new();
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    // Z-buffer
    let mut zbuffer = vec![u32::MAX; 800 * 600];

    // Particles
    let mut particles: Vec<Particle> = Vec::new();

    // Spawn initial particles
    for i in 0..20 {
        let angle = (i as f32 / 20.0) * 2.0 * PI;
        let radius = 5.0;
        let position = Point3::new(angle.cos() * radius, 1.0, angle.sin() * radius);
        let velocity = Vector3::new((angle + PI).cos() * 0.5, 2.0, (angle + PI).sin() * 0.5);
        let color = match i % 5 {
            0 => Rgb565::CSS_RED,
            1 => Rgb565::CSS_GREEN,
            2 => Rgb565::CSS_BLUE,
            3 => Rgb565::CSS_YELLOW,
            _ => Rgb565::CSS_CYAN,
        };
        particles.push(Particle::new(position, velocity, color));
    }

    // Camera state
    let mut camera_yaw = 0.0f32;
    let mut last_frame = std::time::Instant::now();

    println!("Controls:");
    println!("  W/S         - Move forward/backward");
    println!("  A/D         - Turn left/right");
    println!("  Q/E         - Move up/down");
    println!("  SPACE       - Spawn more particles");
    println!("  ESC         - Exit");
    println!("\nStarting render loop...");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        perf.start_of_frame();

        let now = std::time::Instant::now();
        let dt = (now - last_frame).as_secs_f32().min(0.1); // Cap at 100ms
        last_frame = now;

        // Movement parameters
        let move_speed = 5.0;
        let turn_speed = 2.0;

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

                    // Spawn particles
                    Keycode::Space => {
                        let spawn_time = now.elapsed().as_secs_f32();
                        for i in 0..10 {
                            let angle = (i as f32 / 10.0 + spawn_time) * 2.0 * PI;
                            let speed = 2.0 + (i as f32 / 10.0);
                            let position = engine.camera.position;
                            let velocity = Vector3::new(
                                angle.cos() * speed,
                                1.5 + (i as f32 / 5.0),
                                angle.sin() * speed,
                            );
                            let colors = [
                                Rgb565::CSS_RED,
                                Rgb565::CSS_GREEN,
                                Rgb565::CSS_BLUE,
                                Rgb565::CSS_YELLOW,
                                Rgb565::CSS_CYAN,
                            ];
                            let color = colors[i % colors.len()];
                            particles.push(Particle::new(position, velocity, color));
                        }
                        println!("Spawned 10 particles! Total: {}", particles.len());
                    }

                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Update particles
        for particle in &mut particles {
            particle.update(dt);
        }

        // Remove dead particles
        particles.retain(|p| p.is_alive());

        // Clear display and Z-buffer
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        // Render billboards as triangles
        let camera_up = Vector3::new(0.0, 1.0, 0.0);
        for particle in &particles {
            // Generate quad vertices
            let quad = particle
                .billboard
                .generate_quad(engine.camera.position, camera_up);
            let triangles = Billboard::get_triangles();

            // Create temporary geometry for rendering
            let geometry = Geometry {
                vertices: &quad,
                faces: &triangles,
                colors: &[],
                lines: &[],
                normals: &[],
                vertex_normals: &[],
                uvs: &[],
                texture_id: None,
            };

            // Transform and render each triangle
            for face in geometry.faces {
                if let Some([p1, p2, p3]) =
                    engine.transform_points(face, geometry.vertices, engine.camera.vp_matrix)
                {
                    use embedded_3dgfx::DrawPrimitive;
                    draw_zbuffered(
                        DrawPrimitive::ColoredTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                            color: particle.billboard.color,
                        },
                        &mut display,
                        &mut zbuffer,
                        800,
                    );
                }
            }
        }

        // Display info
        perf.print();
        let info_text = format!(
            "{}\\nParticles: {}\\nCamera: [{:.1}, {:.1}, {:.1}]",
            perf.get_text(),
            particles.len(),
            engine.camera.position.x,
            engine.camera.position.y,
            engine.camera.position.z
        );
        Text::new(&info_text, Point::new(10, 20), text_style)
            .draw(&mut display)
            .unwrap();

        // Help text at bottom
        let help_text = "W/S: Move | A/D: Turn | Q/E: Up/Down | SPACE: Spawn | ESC: Exit";
        Text::new(help_text, Point::new(10, 580), text_style)
            .draw(&mut display)
            .unwrap();

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("\nExiting...");
}
