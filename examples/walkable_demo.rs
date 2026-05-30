//! First-person dungeon demo
//!
//! A walkable stone hall with four pillars, flickering torches (particles +
//! dynamic point lights).  Demonstrates the new `CharacterController` and
//! `InputState` modules alongside the existing rendering pipeline.
//!
//! Controls:
//!   W / S           — walk forward / backward
//!   A / D           — turn left / right
//!   Q / E           — look up / down
//!   Left / Right    — strafe left / right
//!   Space           — jump
//!   Shift           — sprint
//!   ESC             — exit
//!
//! Run with:  cargo run --example walkable_demo --features std

use std::f32::consts::PI;
use std::thread;
use std::time::{Duration, Instant};

use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::lights::PointLight;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::particles::{ParticleSpawn, ParticleSystem};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::{K3dengine, RetroStyle};
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
use embedded_graphics::text::Text;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, Vector3};

// ── Display ───────────────────────────────────────────────────────────────────

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

// ── Level constants ───────────────────────────────────────────────────────────

const R: f32 = 5.0; // hall half-width
const CEIL: f32 = 3.2; // ceiling height
const PIL: f32 = 0.35; // pillar half-size
const EYE_H: f32 = 1.5; // eye height above floor
const PILLAR_XZ: [(f32, f32); 4] = [(-3.0, -3.0), (3.0, -3.0), (-3.0, 3.0), (3.0, 3.0)];

const TORCH_POS: [[f32; 3]; 4] = [
    [0.0, 2.3, -(R - 0.15)],
    [0.0, 2.3, R - 0.15],
    [R - 0.15, 2.3, 0.0],
    [-(R - 0.15), 2.3, 0.0],
];
const FLICKER_PHASE: [f32; 4] = [0.0, 1.9, 3.7, 5.3];

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Emit two triangles + two per-face normals for a planar quad.
/// `n` should be the inward-facing normal (pointing toward the viewer).
fn add_quad(
    verts: &mut Vec<[f32; 3]>,
    faces: &mut Vec<[usize; 3]>,
    norms: &mut Vec<[f32; 3]>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    n: [f32; 3],
) {
    let b = verts.len();
    verts.extend_from_slice(&[p0, p1, p2, p3]);
    faces.push([b, b + 1, b + 2]);
    faces.push([b, b + 2, b + 3]);
    norms.push(n);
    norms.push(n);
}

fn build_level() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let mut verts = Vec::new();
    let mut faces = Vec::new();
    let mut norms = Vec::new();

    // Inward normals: point toward the room interior / toward the player.
    add_quad(
        &mut verts,
        &mut faces,
        &mut norms,
        [-R, 0.0, -R],
        [R, 0.0, -R],
        [R, 0.0, R],
        [-R, 0.0, R],
        [0.0, 1.0, 0.0],
    ); // floor   → up
    add_quad(
        &mut verts,
        &mut faces,
        &mut norms,
        [-R, CEIL, R],
        [R, CEIL, R],
        [R, CEIL, -R],
        [-R, CEIL, -R],
        [0.0, -1.0, 0.0],
    ); // ceiling → down

    add_quad(
        &mut verts,
        &mut faces,
        &mut norms,
        [-R, 0.0, -R],
        [R, 0.0, -R],
        [R, CEIL, -R],
        [-R, CEIL, -R],
        [0.0, 0.0, 1.0],
    ); // north wall → +Z
    add_quad(
        &mut verts,
        &mut faces,
        &mut norms,
        [R, 0.0, R],
        [-R, 0.0, R],
        [-R, CEIL, R],
        [R, CEIL, R],
        [0.0, 0.0, -1.0],
    ); // south wall → –Z
    add_quad(
        &mut verts,
        &mut faces,
        &mut norms,
        [R, 0.0, -R],
        [R, 0.0, R],
        [R, CEIL, R],
        [R, CEIL, -R],
        [-1.0, 0.0, 0.0],
    ); // east wall  → –X
    add_quad(
        &mut verts,
        &mut faces,
        &mut norms,
        [-R, 0.0, R],
        [-R, 0.0, -R],
        [-R, CEIL, -R],
        [-R, CEIL, R],
        [1.0, 0.0, 0.0],
    ); // west wall  → +X

    // Pillars: normals point outward from the pillar surface (= toward the player).
    for &(cx, cz) in &PILLAR_XZ {
        let (x0, x1) = (cx - PIL, cx + PIL);
        let (z0, z1) = (cz - PIL, cz + PIL);
        add_quad(
            &mut verts,
            &mut faces,
            &mut norms,
            [x1, 0.0, z0],
            [x0, 0.0, z0],
            [x0, CEIL, z0],
            [x1, CEIL, z0],
            [0.0, 0.0, -1.0],
        );
        add_quad(
            &mut verts,
            &mut faces,
            &mut norms,
            [x0, 0.0, z1],
            [x1, 0.0, z1],
            [x1, CEIL, z1],
            [x0, CEIL, z1],
            [0.0, 0.0, 1.0],
        );
        add_quad(
            &mut verts,
            &mut faces,
            &mut norms,
            [x0, 0.0, z0],
            [x0, 0.0, z1],
            [x0, CEIL, z1],
            [x0, CEIL, z0],
            [-1.0, 0.0, 0.0],
        );
        add_quad(
            &mut verts,
            &mut faces,
            &mut norms,
            [x1, 0.0, z1],
            [x1, 0.0, z0],
            [x1, CEIL, z0],
            [x1, CEIL, z1],
            [1.0, 0.0, 0.0],
        );
        add_quad(
            &mut verts,
            &mut faces,
            &mut norms,
            [x0, CEIL, z0],
            [x1, CEIL, z0],
            [x1, CEIL, z1],
            [x0, CEIL, z1],
            [0.0, 1.0, 0.0],
        );
    }

    (verts, faces, norms)
}

// ── Particle helpers ──────────────────────────────────────────────────────────

fn lcg(s: &mut u32) -> f32 {
    *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*s >> 1) as f32 / (u32::MAX as f32 * 0.5 + 1.0)
}

fn spawn_flame(sys: &mut ParticleSystem<512>, pos: [f32; 3], rng: &mut u32) {
    let n = 2 + (lcg(rng) * 2.0) as usize;
    for _ in 0..n {
        let ox = (lcg(rng) - 0.5) * 0.12;
        let oz = (lcg(rng) - 0.5) * 0.12;
        sys.spawn(ParticleSpawn {
            position: Point3::new(pos[0] + ox, pos[1], pos[2] + oz),
            velocity: Vector3::new(
                (lcg(rng) - 0.5) * 0.18,
                0.5 + lcg(rng) * 0.9,
                (lcg(rng) - 0.5) * 0.18,
            ),
            acceleration: Vector3::zeros(),
            color_start: Rgb565::new(31, 28, 0),
            color_end: Rgb565::new(20, 4, 0),
            size_start: 0.13,
            size_end: 0.0,
            lifetime: 0.4 + lcg(rng) * 0.55,
        });
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let mut window = Window::new(
        "Dungeon - W/S fwd/back | A/D turn | E/Q look up/dn | Arrows strafe | Space jump | ESC exit",
        &output_settings,
    );

    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.apply_retro_style(RetroStyle::doom_walkable());
    // near must be smaller than the player collision margin (0.3) so walls
    // closer than that never have all vertices behind the clip plane.
    engine.camera.set_near_far(0.1, 20.0);

    let mut zbuffer = vec![u32::MAX; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<8192>::new();

    // ── Level mesh ────────────────────────────────────────────────────────────
    let (lv, lf, ln) = build_level();
    let level_geom = Geometry {
        vertices: &lv,
        faces: &lf,
        colors: &[],
        lines: &[],
        normals: &ln,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };
    let mut level = K3dMesh::new(level_geom);
    level.set_render_mode(RenderMode::Solid);
    level.set_color(Rgb565::new(12, 22, 13));

    // ── Player state ──────────────────────────────────────────────────────────
    let mut player_pos = Point3::new(0.0_f32, EYE_H, 0.0_f32);
    let mut player_yaw = 0.3_f32;
    let mut player_pitch = 0.0_f32;
    let mut v_vel = 0.0_f32; // vertical velocity
    let mut on_ground = true;

    let walk_speed = 0.22_f32; // metres per key-repeat event
    let sprint_speed = 0.40_f32;
    let turn_speed = 0.07_f32; // radians per event
    let mut sprinting = false;

    // ── Particles ─────────────────────────────────────────────────────────────
    let mut particles: ParticleSystem<512> = ParticleSystem::new();
    let mut rng: u32 = 0xDEAD_BEEF;
    let flame_gravity = Vector3::new(0.0, 0.2, 0.0); // flames rise

    for _ in 0..24 {
        for pos in &TORCH_POS {
            spawn_flame(&mut particles, *pos, &mut rng);
        }
        particles.update(1.0 / 30.0, flame_gravity);
    }

    // ── Perf counter / timing ─────────────────────────────────────────────────
    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let mut time = 0.0_f32;
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);

    println!("Controls:");
    println!("  W / S       forward / backward");
    println!("  A / D       turn left / right");
    println!("  E / Q       look up / down");
    println!("  Left/Right  strafe");
    println!("  Space       jump");
    println!("  Shift       sprint");
    println!("  ESC         quit");

    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    let mut last_frame = Instant::now();

    'running: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        // ── Frame time (always measured from wall clock) ───────────────────────
        let frame_start = Instant::now();
        let dt = frame_start
            .duration_since(last_frame)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        last_frame = frame_start;

        let speed = if sprinting { sprint_speed } else { walk_speed };
        let fwd_x = player_yaw.sin();
        let fwd_z = -player_yaw.cos();

        // ── Events ────────────────────────────────────────────────────────────
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => match keycode {
                    Keycode::Escape => break 'running,

                    Keycode::LShift | Keycode::RShift => sprinting = true,

                    Keycode::W => {
                        player_pos.x += fwd_x * speed;
                        player_pos.z += fwd_z * speed;
                    }
                    Keycode::S => {
                        player_pos.x -= fwd_x * speed;
                        player_pos.z -= fwd_z * speed;
                    }
                    Keycode::A => {
                        player_yaw -= turn_speed;
                    }
                    Keycode::D => {
                        player_yaw += turn_speed;
                    }

                    // Look up (E) / down (Q)
                    Keycode::E | Keycode::Up => {
                        player_pitch = (player_pitch + turn_speed).min(PI / 2.0 - 0.05);
                    }
                    Keycode::Q | Keycode::Down => {
                        player_pitch = (player_pitch - turn_speed).max(-PI / 2.0 + 0.05);
                    }

                    // Strafe
                    Keycode::Left => {
                        player_pos.x -= fwd_z * speed; // right = (fwd_z, -fwd_x) rotated
                        player_pos.z += fwd_x * speed;
                    }
                    Keycode::Right => {
                        player_pos.x += fwd_z * speed;
                        player_pos.z -= fwd_x * speed;
                    }

                    Keycode::Space => {
                        if on_ground {
                            v_vel = 5.5;
                            on_ground = false;
                        }
                    }

                    _ => {}
                },
                SimulatorEvent::KeyUp { keycode, .. } => {
                    if matches!(keycode, Keycode::LShift | Keycode::RShift) {
                        sprinting = false;
                    }
                }
                SimulatorEvent::Quit => break 'running,
                _ => {}
            }
        }

        // ── Physics: gravity + floor snap ─────────────────────────────────────
        v_vel -= 14.0 * dt;
        player_pos.y += v_vel * dt;
        if player_pos.y <= EYE_H {
            player_pos.y = EYE_H;
            v_vel = 0.0;
            on_ground = true;
        }

        // ── Boundary clamp (keep player inside room, outside pillars) ─────────
        let margin = 0.3_f32;
        player_pos.x = player_pos.x.clamp(-(R - margin), R - margin);
        player_pos.z = player_pos.z.clamp(-(R - margin), R - margin);
        for &(cx, cz) in &PILLAR_XZ {
            let dx = player_pos.x - cx;
            let dz = player_pos.z - cz;
            let half = PIL + margin;
            if dx.abs() < half && dz.abs() < half {
                if dx.abs() > dz.abs() {
                    player_pos.x = cx + dx.signum() * half;
                } else {
                    player_pos.z = cz + dz.signum() * half;
                }
            }
        }

        time += dt;

        // ── Point lights (torch flicker) ──────────────────────────────────────
        engine.clear_point_lights();
        for (i, pos) in TORCH_POS.iter().enumerate() {
            let fr = 1.0
                + 0.14 * (time * 11.3 + FLICKER_PHASE[i]).sin()
                + 0.05 * (time * 23.7 + FLICKER_PHASE[i] * 1.7).sin();
            let fi = 1.0
                + 0.22 * (time * 9.1 + FLICKER_PHASE[i] * 0.8).sin()
                + 0.08 * (time * 31.1 + FLICKER_PHASE[i] * 2.3).sin();
            engine.add_point_light(
                PointLight::new(
                    Point3::new(pos[0], pos[1], pos[2]),
                    Rgb565::new(31, 20, 5),
                    9.0 * fr,
                )
                .with_intensity(1.8 * fi),
            );
        }

        // ── Particles ─────────────────────────────────────────────────────────
        for pos in &TORCH_POS {
            spawn_flame(&mut particles, *pos, &mut rng);
        }
        particles.update(dt, flame_gravity);

        // ── Camera ────────────────────────────────────────────────────────────
        let cp = player_pitch.cos();
        let look = Vector3::new(
            cp * player_yaw.sin(),
            player_pitch.sin(),
            -cp * player_yaw.cos(),
        );
        engine.camera.set_position(player_pos);
        engine.camera.set_target(player_pos + look);

        // ── Render ────────────────────────────────────────────────────────────
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(u32::MAX);

        engine
            .record(std::iter::once(&level), &mut commands, None)
            .unwrap();
        particles.record(&engine, &mut commands);

        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute::<_, 8192>(&mut display, &mut frame, &commands, None)
            .unwrap();

        // ── HUD ───────────────────────────────────────────────────────────────
        let mut hud_y = 12i32; // top margin; each FONT_6X10 line is 12 px
        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(perf.get_text(), Point::new(4, hud_y), text_style)
                .draw(&mut display)
                .unwrap();
            hud_y += 12;
        }

        let status = if on_ground { "grnd" } else { "air" };
        let info = format!(
            "pos ({:.1},{:.1},{:.1})  yaw {:.0}  [{}]",
            player_pos.x,
            player_pos.y,
            player_pos.z,
            player_yaw.to_degrees(),
            status
        );
        Text::new(&info, Point::new(4, hud_y), text_style)
            .draw(&mut display)
            .unwrap();

        Text::new(
            "W/S fwd | A/D turn | E/Q look up/dn | Arrows strafe | Space jump | ESC exit",
            Point::new(10, HEIGHT as i32 - 10),
            text_style,
        )
        .draw(&mut display)
        .unwrap();

        window.update(&display);

        // Sleep only for whatever budget remains so a slow render frame
        // doesn't stack an additional 16 ms on top of itself.
        const FRAME_BUDGET: Duration = Duration::from_millis(16);
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_BUDGET {
            thread::sleep(FRAME_BUDGET - elapsed);
        }
    }
}
