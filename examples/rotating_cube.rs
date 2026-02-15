//! Rotating cube example
//!
//! Demonstrates animated 3D transformations with a continuously rotating cube

use embedded_3dgfx::K3dengine;
use embedded_3dgfx::draw::draw;
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
use nalgebra::Point3;
use std::thread;
use std::time::{Duration, Instant};

fn make_cube() -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let vertices = vec![
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
    ];

    let faces = vec![
        [0, 1, 2],
        [0, 2, 3],
        [5, 4, 7],
        [5, 7, 6],
        [3, 2, 6],
        [3, 6, 7],
        [4, 5, 1],
        [4, 1, 0],
        [1, 5, 6],
        [1, 6, 2],
        [4, 0, 3],
        [4, 3, 7],
    ];

    (vertices, faces)
}

fn main() {
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(640, 480));

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("Rotating Cube - Press ESC to exit", &output_settings);

    // Create 3D engine
    let mut engine = K3dengine::new(640, 480);
    engine.camera.set_position(Point3::new(0.0, 2.0, 6.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Create cube
    let (vertices, faces) = make_cube();
    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[],
        lines: &[],
        normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let mut cube = K3dMesh::new(geometry);
    cube.set_render_mode(RenderMode::Lines);
    cube.set_color(Rgb565::CSS_CYAN);

    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    let start_time = Instant::now();

    println!("Rotating cube demo");
    println!("Press ESC to exit");

    // Initial render
    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        // Handle events
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => {
                    if keycode == Keycode::Escape {
                        break 'running;
                    }
                }
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // Calculate rotation based on time
        let elapsed = start_time.elapsed().as_secs_f32();

        // Update cube rotation
        cube.set_attitude(elapsed * 0.5, elapsed, elapsed * 0.3);

        // Clear display
        display.clear(Rgb565::BLACK).unwrap();

        // Render
        engine.render(std::iter::once(&cube), |prim| {
            draw(prim, &mut display);
        });

        // Display FPS
        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(10, 20), text_style)
                .draw(&mut display)
                .unwrap();
        }

        // Update window
        window.update(&display);

        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
}
