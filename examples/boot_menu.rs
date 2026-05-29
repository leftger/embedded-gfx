//! Boot splash and menu transition demo (96×64 logical resolution).
//!
//! **What you should see**
//! 1. **Boot (~1.4 s)** — textured logo bitmap rises and spins (billboard).
//! 2. **Fade** — brief darken between phases.
//! 3. **Transition** — five menu markers slide in; logo parks top-left.
//! 4. **Menu** — UP/DOWN (5 items); bottom-left label; fades on phase changes.
//!
//! Controls: SPACE skip | UP/DOWN menu | R restart | ESC quit
//!
//! Run: `cargo run --example boot_menu --features std`

use embedded_3dgfx::DrawPrimitive;
use embedded_3dgfx::K3dengine;
use embedded_3dgfx::billboard::Billboard;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::draw::draw_zbuffered_with_textures as draw_tex;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::texture::{Texture, TextureManager};
use embedded_3dgfx::transform_anim::{AnimationPlayer, TransformKeyframe, TransformTrack};
use embedded_3dgfx::tween::{Easing, Tween, scale_rgb565};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};

const W: u32 = 96;
const H: u32 = 64;
const MENU_COUNT: usize = 5;
const FADE_DURATION: f32 = 0.28;

/// Logo intro: rise into frame and spin (centered in view).
const LOGO_INTRO: &[TransformKeyframe] = &[
    TransformKeyframe::new(0.0, [0.0, -1.8, 0.0], 0.0, 0.0, 0.0, 0.9),
    TransformKeyframe::new(0.5, [0.0, -0.4, 0.0], 0.0, 0.0, 0.0, 1.0),
    TransformKeyframe::new(1.4, [0.0, 0.0, 0.0], 0.0, 0.0, 6.28, 1.1),
];

const MENU_X: f32 = 0.0;
const MENU_Y: [f32; MENU_COUNT] = [0.85, 0.42, 0.0, -0.42, -0.85];
const MENU_ITEM_SCALE: f32 = 0.65;
const MENU_SLIDE_FROM: f32 = -1.8;
const LOGO_MENU_X: f32 = -0.55;
const LOGO_MENU_Y: f32 = 1.2;
const LOGO_MENU_SIZE: f32 = 0.55;

const MENU_LABELS: [&str; MENU_COUNT] = ["PLAY", "OPTS", "SND ", "HUD ", "QUIT"];
const MENU_COLORS: [Rgb565; MENU_COUNT] = [
    Rgb565::CSS_GREEN,
    Rgb565::CSS_YELLOW,
    Rgb565::CSS_CYAN,
    Rgb565::CSS_MAGENTA,
    Rgb565::CSS_RED,
];

/// 16×16 logo bitmap (cyan ring + cross on black).
static LOGO_TEX_DATA: [Rgb565; 256] = make_logo_tex_data();

const fn make_logo_tex_data() -> [Rgb565; 256] {
    let mut d = [Rgb565::BLACK; 256];
    let mut y = 0;
    while y < 16 {
        let mut x = 0;
        while x < 16 {
            let dx = x as i32 - 7;
            let dy = y as i32 - 7;
            let dist_sq = dx * dx + dy * dy;
            let idx = y * 16 + x;
            // Outer ring
            if dist_sq >= 16 && dist_sq <= 36 {
                d[idx] = Rgb565::new(0, 31, 31);
            }
            // Inner cross
            if (dx.abs() <= 1 && dy.abs() <= 5) || (dy.abs() <= 1 && dx.abs() <= 5) {
                d[idx] = Rgb565::new(4, 40, 48);
            }
            // Core dot
            if dist_sq <= 4 {
                d[idx] = Rgb565::new(8, 60, 62);
            }
            x += 1;
        }
        y += 1;
    }
    d
}

static DIAMOND_VERTS: [[f32; 3]; 5] = [
    [0.0, 0.22, 0.0],
    [-0.2, 0.0, 0.0],
    [0.2, 0.0, 0.0],
    [0.0, 0.0, 0.2],
    [0.0, 0.0, -0.2],
];

static DIAMOND_FACES: [[usize; 3]; 6] = [
    [0, 1, 3],
    [0, 3, 2],
    [0, 2, 4],
    [0, 4, 1],
    [1, 4, 3],
    [2, 3, 4],
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Boot,
    Transition,
    Menu,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FadeMode {
    None,
    Out { next: Screen },
    In,
}

fn make_menu_item(y: f32, color: Rgb565) -> K3dMesh<'static> {
    let geometry = Geometry {
        vertices: &DIAMOND_VERTS,
        faces: &DIAMOND_FACES,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut mesh = K3dMesh::new(geometry);
    mesh.set_render_mode(RenderMode::Lines);
    mesh.set_color(color);
    mesh.set_position(MENU_SLIDE_FROM, y, 0.0);
    mesh.set_scale(MENU_ITEM_SCALE);
    mesh
}

fn make_menu_items() -> [K3dMesh<'static>; MENU_COUNT] {
    [
        make_menu_item(MENU_Y[0], MENU_COLORS[0]),
        make_menu_item(MENU_Y[1], MENU_COLORS[1]),
        make_menu_item(MENU_Y[2], MENU_COLORS[2]),
        make_menu_item(MENU_Y[3], MENU_COLORS[3]),
        make_menu_item(MENU_Y[4], MENU_COLORS[4]),
    ]
}

fn place_menu_items(items: &mut [K3dMesh<'static>], x: f32) {
    for (i, item) in items.iter_mut().enumerate() {
        item.set_position(x, MENU_Y[i], 0.0);
    }
}

fn apply_billboard_from_sample(
    billboard: &mut Billboard,
    sample: &embedded_3dgfx::SampledTransform,
    size_mul: f32,
) {
    billboard.position = Point3::new(sample.position[0], sample.position[1], sample.position[2]);
    billboard.size = sample.scale * size_mul;
    // Yaw spins the textured quad in screen space (billboards ignore pitch/roll).
    billboard.rotation = sample.yaw;
}

fn park_logo_billboard(billboard: &mut Billboard) {
    billboard.position = Point3::new(LOGO_MENU_X, LOGO_MENU_Y, 0.0);
    billboard.size = LOGO_MENU_SIZE;
    billboard.rotation = 0.0;
}

fn apply_brightness(display: &mut SimulatorDisplay<Rgb565>, factor: f32) {
    let factor = factor.clamp(0.0, 1.0);
    for y in 0..H {
        for x in 0..W {
            let p = Point::new(x as i32, y as i32);
            let c = scale_rgb565(display.get_pixel(p), factor);
            Pixel(p, c).draw(display).unwrap();
        }
    }
}

fn render_textured_billboard(
    engine: &K3dengine,
    billboard: &Billboard,
    texture_id: u32,
    texture_manager: &TextureManager<4>,
    display: &mut SimulatorDisplay<Rgb565>,
    zbuffer: &mut [u32],
) {
    let camera_up = Vector3::y();
    let quad = billboard.generate_quad(engine.camera.position, camera_up);
    let uvs: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let faces = Billboard::get_triangles();

    for face in faces {
        if let Some([p1, p2, p3]) = engine.transform_points(&face, &quad, engine.camera.vp_matrix) {
            draw_tex(
                DrawPrimitive::TexturedTriangleWithDepth {
                    points: [p1.xy(), p2.xy(), p3.xy()],
                    depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                    uvs: [uvs[face[0]], uvs[face[1]], uvs[face[2]]],
                    texture_id,
                },
                display,
                zbuffer,
                W as usize,
                texture_manager,
                None,
                None,
            );
        }
    }
}

fn render_scene(
    engine: &K3dengine,
    screen: Screen,
    logo_billboard: &Billboard,
    logo_tex: u32,
    texture_manager: &TextureManager<4>,
    menu_items: &[K3dMesh<'static>],
    commands: &mut CommandBuffer<1024>,
    display: &mut SimulatorDisplay<Rgb565>,
    zbuffer: &mut [u32],
) {
    match screen {
        Screen::Boot => {
            render_textured_billboard(
                engine,
                logo_billboard,
                logo_tex,
                texture_manager,
                display,
                zbuffer,
            );
        }
        Screen::Transition | Screen::Menu => {
            engine
                .record_render_commands(menu_items.iter(), commands)
                .unwrap();
            engine
                .execute_recorded_frame::<_, 1024>(
                    display, zbuffer, W as usize, H as usize, &commands,
                )
                .unwrap();
            render_textured_billboard(
                engine,
                logo_billboard,
                logo_tex,
                texture_manager,
                display,
                zbuffer,
            );
        }
    }
}

fn start_fade_out(fade_mode: &mut FadeMode, fade_tween: &mut Tween, next: Screen) {
    *fade_mode = FadeMode::Out { next };
    *fade_tween = Tween::new(1.0, 0.0, FADE_DURATION, Easing::EaseInCubic);
}

fn start_fade_in(fade_mode: &mut FadeMode, fade_tween: &mut Tween) {
    *fade_mode = FadeMode::In;
    *fade_tween = Tween::new(0.0, 1.0, FADE_DURATION, Easing::EaseOutCubic);
}

fn reset_demo(
    menu_items: &mut [K3dMesh<'static>; MENU_COUNT],
    logo_player: &mut AnimationPlayer<'_>,
    menu_slide: &mut Tween,
    highlight_tween: &mut Tween,
    fade_mode: &mut FadeMode,
    fade_tween: &mut Tween,
    screen: &mut Screen,
    selected: &mut usize,
    logo_billboard: &mut Billboard,
    zbuffer: &mut [u32],
) {
    *menu_items = make_menu_items();
    logo_player.reset();
    logo_player.set_playing(true);
    menu_slide.reset();
    highlight_tween.reset();
    *fade_mode = FadeMode::None;
    fade_tween.reset();
    *screen = Screen::Boot;
    *selected = 0;
    *logo_billboard = Billboard::new(Point3::new(0.0, -1.8, 0.0), 0.9, Rgb565::CSS_CYAN);
    zbuffer.fill(u32::MAX);
}

fn main() {
    let output_settings = OutputSettingsBuilder::new().scale(6).build();
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(W, H));
    let mut window = Window::new("Boot/Menu 96x64 — SPACE UP/DOWN R ESC", &output_settings);

    let mut engine = K3dengine::new(W as u16, H as u16);
    apply_default_caps(&mut engine);
    engine.camera.set_near_far(0.2, 10.0);
    engine.camera.set_fovy(std::f32::consts::FRAC_PI_3);
    engine.camera.set_position(Point3::new(0.0, 0.0, 3.4));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    let mut texture_manager = TextureManager::<4>::new();
    let logo_tex = texture_manager
        .add_texture(Texture::new(&LOGO_TEX_DATA, 16, 16))
        .unwrap();

    let mut logo_billboard = Billboard::new(Point3::new(0.0, -1.8, 0.0), 0.9, Rgb565::CSS_CYAN);
    let mut menu_items = make_menu_items();

    let intro_track = TransformTrack::new(LOGO_INTRO, false);
    let mut logo_player = AnimationPlayer::new(intro_track);
    let mut menu_slide = Tween::new(MENU_SLIDE_FROM, MENU_X, 0.4, Easing::Smoothstep);
    let mut highlight_tween = Tween::new(MENU_ITEM_SCALE, 1.0, 0.12, Easing::EaseOutCubic);
    let mut fade_tween = Tween::new(1.0, 1.0, 0.0, Easing::Linear);
    let mut fade_mode = FadeMode::None;

    let mut screen = Screen::Boot;
    let mut selected: usize = 0;
    let mut zbuffer = [u32::MAX; (W as usize) * (H as usize)];
    let mut commands = CommandBuffer::<1024>::new();
    let mut last_frame = std::time::Instant::now();

    println!("Boot/menu @ {W}x{H} — bitmap logo + 5 items + fades");
    println!("SPACE: skip | UP/DOWN: select | R: restart | ESC: quit");

    display.clear(Rgb565::BLACK).unwrap();
    apply_billboard_from_sample(&mut logo_billboard, &logo_player.sample(), 1.0);
    render_scene(
        &engine,
        Screen::Boot,
        &logo_billboard,
        logo_tex,
        &texture_manager,
        &menu_items,
        &mut commands,
        &mut display,
        &mut zbuffer,
    );
    window.update(&display);

    'running: loop {
        let dt = last_frame.elapsed().as_secs_f32();
        last_frame = std::time::Instant::now();

        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,
                    Keycode::Space => match screen {
                        Screen::Boot | Screen::Transition => {
                            screen = Screen::Menu;
                            menu_slide.elapsed = menu_slide.duration;
                            place_menu_items(&mut menu_items, MENU_X);
                            park_logo_billboard(&mut logo_billboard);
                            logo_player.set_playing(false);
                            start_fade_in(&mut fade_mode, &mut fade_tween);
                        }
                        Screen::Menu => {}
                    },
                    Keycode::Up => {
                        if screen == Screen::Menu && selected > 0 {
                            selected -= 1;
                            highlight_tween.reset();
                        }
                    }
                    Keycode::Down => {
                        if screen == Screen::Menu && selected + 1 < MENU_COUNT {
                            selected += 1;
                            highlight_tween.reset();
                        }
                    }
                    Keycode::R => reset_demo(
                        &mut menu_items,
                        &mut logo_player,
                        &mut menu_slide,
                        &mut highlight_tween,
                        &mut fade_mode,
                        &mut fade_tween,
                        &mut screen,
                        &mut selected,
                        &mut logo_billboard,
                        &mut zbuffer,
                    ),
                    _ => {}
                },
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // --- Phase logic (skip while fading except fade tweens) ---
        if fade_mode == FadeMode::None {
            match screen {
                Screen::Boot => {
                    logo_player.advance(dt);
                    apply_billboard_from_sample(&mut logo_billboard, &logo_player.sample(), 1.0);
                    if logo_player.is_done() {
                        start_fade_out(&mut fade_mode, &mut fade_tween, Screen::Transition);
                    }
                }
                Screen::Transition => {
                    menu_slide.advance(dt);
                    place_menu_items(&mut menu_items, menu_slide.value());
                    park_logo_billboard(&mut logo_billboard);
                    if menu_slide.is_done() {
                        start_fade_out(&mut fade_mode, &mut fade_tween, Screen::Menu);
                    }
                }
                Screen::Menu => {
                    park_logo_billboard(&mut logo_billboard);
                    highlight_tween.advance(dt);
                    for (i, item) in menu_items.iter_mut().enumerate() {
                        let scale = if i == selected {
                            if highlight_tween.is_done() {
                                1.0
                            } else {
                                highlight_tween.value()
                            }
                        } else {
                            MENU_ITEM_SCALE
                        };
                        item.set_scale(scale);
                    }
                }
            }
        }

        // --- Fade state machine ---
        match fade_mode {
            FadeMode::Out { next } => {
                fade_tween.advance(dt);
                if fade_tween.is_done() {
                    screen = next;
                    match screen {
                        Screen::Transition => {
                            menu_slide.reset();
                            park_logo_billboard(&mut logo_billboard);
                            logo_player.set_playing(false);
                        }
                        Screen::Menu => {
                            place_menu_items(&mut menu_items, MENU_X);
                            for (i, item) in menu_items.iter_mut().enumerate() {
                                item.set_scale(if i == selected { 1.0 } else { MENU_ITEM_SCALE });
                            }
                        }
                        Screen::Boot => {}
                    }
                    start_fade_in(&mut fade_mode, &mut fade_tween);
                }
            }
            FadeMode::In => {
                fade_tween.advance(dt);
                if fade_tween.is_done() {
                    fade_mode = FadeMode::None;
                }
            }
            FadeMode::None => {}
        }

        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        render_scene(
            &engine,
            screen,
            &logo_billboard,
            logo_tex,
            &texture_manager,
            &menu_items,
            &mut commands,
            &mut display,
            &mut zbuffer,
        );

        let brightness = match fade_mode {
            FadeMode::None => 1.0,
            FadeMode::Out { .. } | FadeMode::In => fade_tween.value(),
        };
        if brightness < 1.0 {
            apply_brightness(&mut display, brightness);
        }

        let (label, color) = match screen {
            Screen::Boot => ("BOOT", Rgb565::CSS_CYAN),
            Screen::Transition => ("LOAD", Rgb565::CSS_DARK_GRAY),
            Screen::Menu => (MENU_LABELS[selected], MENU_COLORS[selected]),
        };
        let hud = MonoTextStyle::new(&FONT_6X10, color);
        Text::new(label, Point::new(4, 54), hud)
            .draw(&mut display)
            .unwrap();

        window.update(&display);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
