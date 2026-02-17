//! LVGL Integration Proof of Concept
//!
//! Demonstrates rendering embedded-3dgfx to an LVGL-managed display
//!
//! This POC uses direct framebuffer sharing via the display refresh callback.
//! Since the Rust LVGL bindings don't yet have Canvas widget support, we use
//! the lower-level display registration mechanism to push our 3D framebuffer.
//!
//! NOTE: This example requires LVGL Rust bindings:
//!   cargo add lvgl embedded-graphics-simulator
//!
//! Run with: cargo run --example lvgl_integration_poc --features std

use embedded_3dgfx::{K3dengine, draw::draw};
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use nalgebra::Point3;
use std::time::Duration;

// Uncomment when LVGL bindings are added to Cargo.toml:
// use lvgl::{Display, DrawBuffer, Color, Obj, Label, Style, Part, Align};
// use embedded_graphics_simulator::{SimulatorDisplay, Window, OutputSettingsBuilder, SimulatorEvent};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

fn make_cube() -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let vertices = vec![
        [-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0],
    ];
    let faces = vec![
        [0,1,2], [0,2,3], [5,4,7], [5,7,6], [3,2,6], [3,6,7],
        [4,5,1], [4,1,0], [1,5,6], [1,6,2], [4,0,3], [4,3,7],
    ];
    (vertices, faces)
}

fn main() {
    println!("LVGL Integration POC - embedded-3dgfx + LVGL");
    println!("\nThis example demonstrates how to integrate embedded-3dgfx with LVGL.");
    println!("\n⚠️  NOTE: LVGL Rust bindings need to be added to run this example.");
    println!("Add to Cargo.toml:");
    println!("  lvgl = \"0.6.2\"");
    println!("  embedded-graphics-simulator = \"0.8.0\"\n");

    // NOTE: LVGL dependencies not currently added to Cargo.toml
    // This runs a standalone demo showing what the 3D rendering looks like
    println!("ℹ️  Running standalone demo. LVGL integration code is commented out above.\n");
    demo_standalone_rendering();

    // === LVGL Integration Code (uncomment when dependencies added) ===
    /*
    // Initialize embedded-3dgfx engine
    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    engine.camera.set_position(Point3::new(0.0, 2.0, 6.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Create 3D meshes
    let (vertices, faces) = make_cube();
    let geometry = Geometry {
        vertices: &vertices,
        faces: &faces,
        colors: &[], lines: &[], normals: &[], uvs: &[], texture_id: None,
    };
    let mut cube = K3dMesh::new(geometry);
    cube.set_render_mode(RenderMode::Lines);
    cube.set_color(Rgb565::CSS_CYAN);

    // Create shared framebuffer for 3D rendering
    let mut framebuffer = vec![Rgb565::BLACK; (WIDTH * HEIGHT) as usize];

    // Initialize LVGL
    let mut sim_display: SimulatorDisplay<Rgb565> =
        SimulatorDisplay::new(Size::new(WIDTH, HEIGHT));

    // Register display with LVGL - this is the key integration point
    let buffer = DrawBuffer::<{ (WIDTH * HEIGHT) as usize }>::default();
    let display = Display::register(buffer, WIDTH, HEIGHT, |refresh| {
        // LVGL calls this when it needs to refresh the display
        // We can composite our 3D framebuffer with LVGL's UI elements here

        // For now, just pass through to simulator
        sim_display.draw_iter(refresh.as_pixels()).unwrap();
    }).unwrap();

    // Create LVGL UI elements
    let mut screen = display.get_scr_act().unwrap();

    // Add overlay label showing FPS
    let mut label = Label::from("FPS: 0");
    let mut style = Style::default();
    style.set_text_color(Color::from_rgb((0, 255, 0)));
    label.add_style(Part::Main, &mut style);
    label.set_align(Align::TopLeft, 10, 10);

    // Add control button
    let mut button = Button::create(&mut screen).unwrap();
    button.set_size(100, 40);
    button.set_align(Align::BottomRight, -10, -10);

    let mut btn_label = Label::from("Rotate");
    btn_label.set_align(Align::Center, 0, 0);

    // Main loop
    let mut window = Window::new("3D + LVGL", &OutputSettingsBuilder::new().scale(2).build());
    let start = Instant::now();
    let mut frame_count = 0u32;
    let mut rotation = 0.0f32;

    loop {
        // === 1. Render 3D scene to our framebuffer ===
        framebuffer.fill(Rgb565::BLACK);

        cube.set_attitude(rotation, rotation * 0.7, 0.0);
        rotation += 0.02;

        engine.render(std::iter::once(&cube), |primitive| {
            // Draw 3D primitives to our framebuffer
            // (This would need a framebuffer-based draw implementation)
            // For the POC, you'd adapt the existing draw() function
        });

        // === 2. Copy 3D framebuffer to LVGL's display ===
        // In a real implementation, you'd composite the 3D buffer
        // with LVGL's UI layer in the refresh callback

        // For this POC, we could use a custom widget that displays raw pixels
        // (This is where Canvas widget would be ideal)

        // === 3. Update LVGL UI ===
        frame_count += 1;
        if frame_count % 30 == 0 {
            let fps = frame_count as f32 / start.elapsed().as_secs_f32();
            label.set_text(&format!("FPS: {:.1}", fps));
        }

        lvgl::task_handler();

        // === 4. Display and handle events ===
        window.update(&sim_display);

        for event in window.events() {
            if event == SimulatorEvent::Quit {
                return;
            }
        }

        lvgl::tick_inc(Instant::now().duration_since(start));
        std::thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
    */
}

/// Standalone demo showing what the 3D rendering looks like
/// (Used when LVGL dependencies are not available)
fn demo_standalone_rendering() {
    println!("Running standalone 3D rendering demo (without LVGL)...\n");

    use embedded_graphics_simulator::{
        SimulatorDisplay, Window, OutputSettingsBuilder, SimulatorEvent, sdl2::Keycode
    };

    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH, HEIGHT));
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("3D Rendering (LVGL POC)", &output_settings);

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    engine.camera.set_position(Point3::new(0.0, 2.0, 6.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

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

    let mut rotation = 0.0f32;

    println!("Rendering 3D cube. This framebuffer would be composited with LVGL UI.");
    println!("Press ESC to exit\n");

    loop {
        display.clear(Rgb565::BLACK).ok();

        cube.set_attitude(rotation, rotation * 0.7, 0.0);
        rotation += 0.02;

        engine.render(std::iter::once(&cube), |primitive| {
            draw(primitive, &mut display);
        });

        window.update(&display);

        for event in window.events() {
            match event {
                SimulatorEvent::Quit | SimulatorEvent::KeyDown { keycode: Keycode::Escape, .. } => {
                    println!("\nDemo complete!");
                    return;
                }
                _ => {}
            }
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}

// ============================================================================
// Integration Architecture Notes
// ============================================================================
//
// APPROACH 1: Direct Framebuffer Sharing (This POC)
// --------------------------------------------------
// Pros:
//   - Simple conceptually
//   - No LVGL modifications needed
//   - Works immediately
// Cons:
//   - Manual compositing required
//   - Not using LVGL's rendering pipeline
//   - May have performance overhead
//
// APPROACH 2: Custom Canvas Widget (Recommended)
// -----------------------------------------------
// Requires adding Canvas support to lvgl-rs bindings:
//
//   use lvgl::widgets::Canvas;
//
//   let mut canvas = Canvas::create(&mut screen)?;
//   canvas.set_buffer(&framebuffer, WIDTH, HEIGHT, ColorFormat::Rgb565);
//   canvas.set_align(Align::Center, 0, 0);
//
//   // In render loop:
//   engine.render(meshes, |prim| draw_to_framebuffer(prim, &mut fb));
//   canvas.invalidate(); // Trigger LVGL redraw
//
// This would require FFI work in lvgl-rs to expose lv_canvas C API.
//
// APPROACH 3: Custom Draw Unit (Advanced)
// ----------------------------------------
// Integrate deeply into LVGL's rendering pipeline:
//   - Register embedded-3dgfx as an LVGL draw unit
//   - Handle LV_DRAW_TASK_TYPE_3D tasks
//   - Full hardware acceleration potential
//
// This requires significant C FFI work and LVGL expertise.
//
// ============================================================================
// Next Steps for Real Integration
// ============================================================================
//
// 1. Add Canvas widget to lvgl-rs:
//    - Expose lv_canvas_create, lv_canvas_set_buffer via FFI
//    - Create safe Rust wrapper in lvgl crate
//    - Submit PR to lvgl/lv_binding_rust
//
// 2. Optimize framebuffer sharing:
//    - Use zero-copy techniques where possible
//    - Consider DMA for embedded targets
//    - Profile memory usage (framebuffer + Z-buffer + LVGL buffers)
//
// 3. Handle input events:
//    - LVGL touch/click events → 3D scene interaction
//    - Example: Click button to change camera angle
//    - Example: Slider to control physics gravity
//
// 4. Test on embedded hardware:
//    - STM32 with TFT display
//    - ESP32 with LVGL support
//    - Measure actual FPS and memory usage
//
