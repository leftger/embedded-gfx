//! STAR STRIKER 3D - Classic 90s Space Rail Shooter Demo
//!
//! A high-speed 3D retro arcade rail shooter inspired by early 90s polygonal classics,
//! built entirely using `embedded-3dgfx`.
//!
//! Controls:
//!   - Arrow Keys / WASD : Steer Star Striker (Bank left/right, Pitch up/down)
//!   - SPACE             : Fire Twin Plasma Laser Cannons
//!   - Z / X             : Perform a Barrel Roll (Invulnerability spin & shield deflection)
//!   - ESC               : Exit Demo

use std::collections::HashSet;
use std::f32::consts::PI;
use std::thread;
use std::time::{Duration, Instant};

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::command_buffer::CommandBuffer;
use embedded_3dgfx::config::apply_default_caps;
use embedded_3dgfx::lights::PointLight;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
use embedded_3dgfx::particles::{ParticleSpawn, ParticleSystem};
#[cfg(feature = "perfcounter")]
use embedded_3dgfx::perfcounter::PerformanceCounter;
use embedded_3dgfx::renderer::FrameCtx;
use embedded_3dgfx::{engine::K3dengine, retro::RetroStyle};

use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10, ascii::FONT_8X13_BOLD};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window, sdl2::Keycode,
};
use nalgebra::{Point3, UnitQuaternion, Vector3};

// ── Screen Constants ─────────────────────────────────────────────────────────

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const COMMAND_BUF_SIZE: usize = 16384;

// ── Geometry Helpers ─────────────────────────────────────────────────────────

fn calculate_face_normal(v0: &[f32; 3], v1: &[f32; 3], v2: &[f32; 3]) -> [f32; 3] {
    let edge1 = Vector3::new(v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]);
    let edge2 = Vector3::new(v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]);
    let cross = edge1.cross(&edge2);
    let norm = cross.norm();
    if norm > 1e-6 {
        let n = cross / norm;
        [n.x, n.y, n.z]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn compute_normals(verts: &[[f32; 3]], faces: &[[usize; 3]]) -> Vec<[f32; 3]> {
    faces
        .iter()
        .map(|f| calculate_face_normal(&verts[f[0]], &verts[f[1]], &verts[f[2]]))
        .collect()
}

fn make_double_sided(
    verts: Vec<[f32; 3]>,
    mut faces: Vec<[usize; 3]>,
) -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let orig = faces.len();
    for i in 0..orig {
        let f = faces[i];
        faces.push([f[0], f[2], f[1]]);
    }
    let norms = compute_normals(&verts, &faces);
    (verts, faces, norms)
}

fn add_box(verts: &mut Vec<[f32; 3]>, faces: &mut Vec<[usize; 3]>, min: [f32; 3], max: [f32; 3]) {
    let b = verts.len();
    verts.extend_from_slice(&[
        [min[0], min[1], max[2]], // 0: front-bottom-left
        [max[0], min[1], max[2]], // 1: front-bottom-right
        [max[0], max[1], max[2]], // 2: front-top-right
        [min[0], max[1], max[2]], // 3: front-top-left
        [min[0], min[1], min[2]], // 4: back-bottom-left
        [max[0], min[1], min[2]], // 5: back-bottom-right
        [max[0], max[1], min[2]], // 6: back-top-right
        [min[0], max[1], min[2]], // 7: back-top-left
    ]);
    // Front quad
    faces.push([b, b + 1, b + 2]);
    faces.push([b, b + 2, b + 3]);
    // Back quad
    faces.push([b + 5, b + 4, b + 7]);
    faces.push([b + 5, b + 7, b + 6]);
    // Top quad
    faces.push([b + 3, b + 2, b + 6]);
    faces.push([b + 3, b + 6, b + 7]);
    // Bottom quad
    faces.push([b + 4, b + 5, b + 1]);
    faces.push([b + 4, b + 1, b]);
    // Right quad
    faces.push([b + 1, b + 5, b + 6]);
    faces.push([b + 1, b + 6, b + 2]);
    // Left quad
    faces.push([b + 4, b, b + 3]);
    faces.push([b + 4, b + 3, b + 7]);
}

// ── 3D Mesh Constructors ────────────────────────────────────────────────────

/// Star Striker White Fuselage & Forward Swept Wings
fn make_ship_body() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let verts = vec![
        // 0: Nose tip
        [0.0, 0.0, -2.4],
        // 1: Fuselage Top Ridge
        [0.0, 0.35, -0.4],
        // 2: Fuselage Rear Top
        [0.0, 0.25, 1.2],
        // 3: Fuselage Rear Bottom
        [0.0, -0.22, 1.2],
        // 4: Fuselage Bottom Front
        [0.0, -0.18, -1.6],
        // 5: Left Fuselage Mid
        [-0.45, 0.0, 0.3],
        // 6: Right Fuselage Mid
        [0.45, 0.0, 0.3],
        // 7: Left Wing Tip
        [-2.4, 0.08, 0.9],
        // 8: Left Wing Trailing Edge
        [-0.5, 0.02, 1.2],
        // 9: Left Wing Leading Root
        [-0.45, 0.02, -0.3],
        // 10: Right Wing Tip
        [2.4, 0.08, 0.9],
        // 11: Right Wing Trailing Edge
        [0.5, 0.02, 1.2],
        // 12: Right Wing Leading Root
        [0.45, 0.02, -0.3],
        // 13: Tail Fin Top
        [0.0, 0.9, 1.1],
        // 14: Tail Fin Base Front
        [0.0, 0.3, 0.5],
        // 15: Tail Fin Base Rear
        [0.0, 0.2, 1.3],
    ];

    let faces = vec![
        // Nose Top-Left & Top-Right
        [0, 1, 5],
        [0, 6, 1],
        // Nose Bottom-Left & Bottom-Right
        [0, 5, 4],
        [0, 4, 6],
        // Body Mid Top-Left & Top-Right
        [1, 2, 5],
        [1, 6, 2],
        // Body Mid Bottom-Left & Bottom-Right
        [4, 5, 3],
        [4, 3, 6],
        // Engine Rear Cap
        [2, 6, 3],
        [2, 3, 5],
        // Left Wing (Top & Bottom)
        [9, 7, 8],
        [9, 8, 5],
        [9, 8, 7],
        // Right Wing (Top & Bottom)
        [12, 11, 10],
        [12, 6, 11],
        [12, 10, 11],
        // Tail Fin (Left & Right)
        [14, 13, 15],
        [14, 15, 13],
    ];

    make_double_sided(verts, faces)
}

/// Star Striker Cyan Cockpit Glass Canopy
fn make_ship_canopy() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let verts = vec![
        [0.0, 0.35, -0.5], // front
        [-0.2, 0.22, 0.2], // left
        [0.2, 0.22, 0.2],  // right
        [0.0, 0.52, 0.1],  // peak
        [0.0, 0.32, 0.65], // rear
    ];

    let faces = vec![
        // Front facets
        [0, 3, 1],
        [0, 2, 3],
        // Rear facets
        [3, 4, 1],
        [3, 2, 4],
    ];

    make_double_sided(verts, faces)
}

/// Wingtip Blaster Cannons & Plasma Pods
fn make_ship_accents() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let verts = vec![
        // Left Cannon
        [-2.45, 0.08, -0.1],
        [-2.55, 0.16, 0.9],
        [-2.35, 0.16, 0.9],
        [-2.45, 0.00, 0.9],
        // Right Cannon
        [2.45, 0.08, -0.1],
        [2.35, 0.16, 0.9],
        [2.55, 0.16, 0.9],
        [2.45, 0.00, 0.9],
        // Left Pod
        [-1.3, -0.05, 0.4],
        [-1.4, -0.32, 0.8],
        [-1.2, -0.32, 0.8],
        // Right Pod
        [1.3, -0.05, 0.4],
        [1.2, -0.32, 0.8],
        [1.4, -0.32, 0.8],
    ];

    let faces = vec![
        // Left Cannon
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 1],
        // Right Cannon
        [4, 6, 5],
        [4, 7, 6],
        [4, 5, 7],
        // Left Pod
        [8, 9, 10],
        [8, 10, 9],
        // Right Pod
        [11, 12, 13],
        [11, 13, 12],
    ];

    make_double_sided(verts, faces)
}

/// Enemy Invader Delta Fighter (Prominent delta-wing interceptor)
fn make_enemy_fighter() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let verts = vec![
        // 0: Nose (facing player: +Z)
        [0.0, 0.0, 2.2],
        // 1: Top Center Ridge
        [0.0, 0.6, 0.0],
        // 2: Left Wingtip
        [-2.4, 0.15, -1.0],
        // 3: Right Wingtip
        [2.4, 0.15, -1.0],
        // 4: Bottom Keel
        [0.0, -0.45, 0.0],
        // 5: Rear Exhaust Top
        [0.0, 0.35, -1.4],
        // 6: Rear Exhaust Bottom
        [0.0, -0.25, -1.4],
        // 7: Top Rudder Fin
        [0.0, 1.1, -1.0],
    ];

    let faces = vec![
        // Top front left / right
        [0, 1, 2],
        [0, 3, 1],
        // Bottom front left / right
        [0, 2, 4],
        [0, 4, 3],
        // Rear wings
        [1, 5, 2],
        [1, 3, 5],
        [4, 2, 6],
        [4, 6, 3],
        // Rear cap
        [5, 6, 2],
        [5, 3, 6],
        // Top fin
        [1, 7, 5],
        [1, 5, 7],
    ];

    make_double_sided(verts, faces)
}

/// Low-poly 3D Asteroid (Large faceted tumbling space rock)
fn make_asteroid() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let verts = vec![
        [0.0, 2.0, 0.4],
        [-1.8, 0.6, 1.2],
        [1.7, 0.7, 1.1],
        [1.2, 0.5, -1.7],
        [-1.5, 0.3, -1.4],
        [-1.1, -1.7, 0.9],
        [1.4, -1.5, 0.8],
        [0.3, -1.8, -1.2],
    ];

    let faces = vec![
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 1],
        [1, 5, 2],
        [2, 5, 6],
        [2, 6, 3],
        [3, 6, 7],
        [3, 7, 4],
        [4, 7, 5],
        [4, 5, 1],
        [5, 7, 6],
    ];

    make_double_sided(verts, faces)
}

/// Golden Floating Warp / Recovery Ring (Large 10-sided glowing ring)
fn make_space_ring() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let num_segments = 10;
    let r_out = 3.2f32;
    let r_in = 2.4f32;
    let depth = 0.5f32;

    let mut verts = Vec::new();
    let mut faces = Vec::new();

    for i in 0..num_segments {
        let theta = (i as f32 / num_segments as f32) * 2.0 * PI;
        let c = theta.cos();
        let s = theta.sin();

        verts.push([c * r_out, s * r_out, -depth * 0.5]);
        verts.push([c * r_in, s * r_in, -depth * 0.5]);
        verts.push([c * r_out, s * r_out, depth * 0.5]);
        verts.push([c * r_in, s * r_in, depth * 0.5]);
    }

    for i in 0..num_segments {
        let next = (i + 1) % num_segments;
        let b0 = i * 4;
        let b1 = next * 4;

        // Front face quad
        faces.push([b0, b1, b0 + 1]);
        faces.push([b1, b1 + 1, b0 + 1]);

        // Back face quad
        faces.push([b0 + 2, b0 + 3, b1 + 2]);
        faces.push([b1 + 2, b0 + 3, b1 + 3]);

        // Outer rim quad
        faces.push([b0, b0 + 2, b1]);
        faces.push([b1, b0 + 2, b1 + 2]);

        // Inner rim quad
        faces.push([b0 + 1, b1 + 1, b0 + 3]);
        faces.push([b1 + 1, b1 + 3, b0 + 3]);
    }

    make_double_sided(verts, faces)
}

/// Space Gateway Arch / Monolith (Solid 3D columns & overhead bridge)
fn make_space_arch() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let mut verts = Vec::new();
    let mut faces = Vec::new();

    // Left Column Box
    add_box(&mut verts, &mut faces, [-6.2, -4.5, -1.2], [-4.2, 4.2, 1.2]);
    // Right Column Box
    add_box(&mut verts, &mut faces, [4.2, -4.5, -1.2], [6.2, 4.2, 1.2]);
    // Top Lintel Bridge Box
    add_box(&mut verts, &mut faces, [-6.2, 4.2, -1.2], [6.2, 5.8, 1.2]);

    make_double_sided(verts, faces)
}

/// Bold 3D Laser Bolt Mesh (elongated hexagonal plasma cylinder)
fn make_laser_bolt() -> (Vec<[f32; 3]>, Vec<[usize; 3]>, Vec<[f32; 3]>) {
    let hw = 0.22f32;
    let hh = 0.22f32;
    let l_front = -2.2f32;
    let l_back = 2.2f32;

    let verts = vec![
        // 0: Front sharp tip
        [0.0, 0.0, l_front - 0.8],
        // 1..4: Front quad ring
        [-hw, -hh, l_front],
        [hw, -hh, l_front],
        [hw, hh, l_front],
        [-hw, hh, l_front],
        // 5..8: Rear quad ring
        [-hw, -hh, l_back],
        [hw, -hh, l_back],
        [hw, hh, l_back],
        [-hw, hh, l_back],
        // 9: Rear tip
        [0.0, 0.0, l_back + 0.8],
    ];

    let faces = vec![
        // Front nose cap
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 1],
        // 4 side quads
        [1, 5, 6],
        [1, 6, 2],
        [2, 6, 7],
        [2, 7, 3],
        [3, 7, 8],
        [3, 8, 4],
        [4, 8, 5],
        [4, 5, 1],
        // Rear tail cap
        [9, 6, 5],
        [9, 7, 6],
        [9, 8, 7],
        [9, 5, 8],
    ];

    let norms = compute_normals(&verts, &faces);
    (verts, faces, norms)
}

// ── Particle FX Helpers ─────────────────────────────────────────────────────

fn lcg(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    (*state as f32) / (u32::MAX as f32)
}

fn spawn_engine_exhaust(particles: &mut ParticleSystem<1024>, pos: Point3<f32>, rng: &mut u32) {
    let offset_x = (lcg(rng) - 0.5) * 0.2;
    let offset_y = (lcg(rng) - 0.5) * 0.15;
    let p_pos = Point3::new(pos.x + offset_x, pos.y - 0.05 + offset_y, pos.z + 1.3);

    particles.spawn(ParticleSpawn {
        position: p_pos,
        velocity: Vector3::new(
            (lcg(rng) - 0.5) * 0.4,
            (lcg(rng) - 0.5) * 0.4,
            12.0 + lcg(rng) * 6.0,
        ),
        acceleration: Vector3::zeros(),
        color_start: Rgb565::new(0, 58, 31),
        color_end: Rgb565::new(3, 10, 31),
        size_start: 0.35,
        size_end: 0.08,
        lifetime: 0.28,
    });
}

fn spawn_muzzle_flash(particles: &mut ParticleSystem<1024>, pos: Point3<f32>, rng: &mut u32) {
    for _ in 0..4 {
        particles.spawn(ParticleSpawn {
            position: pos,
            velocity: Vector3::new(
                (lcg(rng) - 0.5) * 4.0,
                (lcg(rng) - 0.5) * 4.0,
                -18.0 - lcg(rng) * 12.0,
            ),
            acceleration: Vector3::zeros(),
            color_start: Rgb565::new(31, 63, 15),
            color_end: Rgb565::new(0, 55, 10),
            size_start: 0.3,
            size_end: 0.05,
            lifetime: 0.14,
        });
    }
}

fn spawn_laser_trail(
    particles: &mut ParticleSystem<1024>,
    pos: Point3<f32>,
    is_enemy: bool,
    rng: &mut u32,
) {
    let (c_start, c_end) = if is_enemy {
        (Rgb565::new(31, 28, 0), Rgb565::new(31, 4, 0))
    } else {
        (Rgb565::new(12, 63, 20), Rgb565::new(0, 48, 10))
    };

    let jitter_x = (lcg(rng) - 0.5) * 0.15;
    let jitter_y = (lcg(rng) - 0.5) * 0.15;
    let trail_pos = Point3::new(pos.x + jitter_x, pos.y + jitter_y, pos.z + 0.6);

    particles.spawn(ParticleSpawn {
        position: trail_pos,
        velocity: Vector3::new(0.0, 0.0, if is_enemy { -2.0 } else { 3.0 }),
        acceleration: Vector3::zeros(),
        color_start: c_start,
        color_end: c_end,
        size_start: 0.25,
        size_end: 0.05,
        lifetime: 0.18,
    });
}

fn spawn_explosion(
    particles: &mut ParticleSystem<1024>,
    pos: Point3<f32>,
    rng: &mut u32,
    big: bool,
) {
    let count = if big { 28 } else { 12 };
    let speed_mult = if big { 9.0 } else { 5.0 };

    for _ in 0..count {
        let theta = lcg(rng) * 2.0 * PI;
        let phi = (lcg(rng) - 0.5) * PI;
        let spd = (0.4 + lcg(rng) * 0.8) * speed_mult;

        let vel = Vector3::new(
            phi.cos() * theta.cos() * spd,
            phi.sin() * spd,
            phi.cos() * theta.sin() * spd,
        );

        let color = if lcg(rng) > 0.4 {
            Rgb565::new(31, 55, 0)
        } else {
            Rgb565::new(31, 10, 0)
        };

        particles.spawn(ParticleSpawn {
            position: pos,
            velocity: vel,
            acceleration: Vector3::new(0.0, -1.5, 0.0),
            color_start: color,
            color_end: Rgb565::new(10, 2, 2),
            size_start: if big { 0.45 } else { 0.28 },
            size_end: 0.05,
            lifetime: if big { 0.65 } else { 0.4 },
        });
    }
}

fn spawn_ring_sparkles(particles: &mut ParticleSystem<1024>, pos: Point3<f32>, rng: &mut u32) {
    for _ in 0..8 {
        let theta = lcg(rng) * 2.0 * PI;
        let spd = 2.0 + lcg(rng) * 3.5;
        particles.spawn(ParticleSpawn {
            position: pos,
            velocity: Vector3::new(theta.cos() * spd, theta.sin() * spd, (lcg(rng) - 0.5) * 2.0),
            acceleration: Vector3::zeros(),
            color_start: Rgb565::new(31, 63, 12),
            color_end: Rgb565::new(31, 40, 0),
            size_start: 0.35,
            size_end: 0.05,
            lifetime: 0.5,
        });
    }
}

fn spawn_starfield_dust(particles: &mut ParticleSystem<1024>, rng: &mut u32) {
    let x = (lcg(rng) - 0.5) * 36.0;
    let y = (lcg(rng) - 0.5) * 26.0;
    let z = -75.0 - lcg(rng) * 25.0;

    let colors = [
        Rgb565::new(31, 63, 31),
        Rgb565::new(12, 45, 31),
        Rgb565::new(31, 55, 15),
    ];
    let col = colors[(lcg(rng) * 3.0) as usize % 3];

    particles.spawn(ParticleSpawn {
        position: Point3::new(x, y, z),
        velocity: Vector3::new(0.0, 0.0, 48.0),
        acceleration: Vector3::zeros(),
        color_start: col,
        color_end: Rgb565::new(2, 4, 8),
        size_start: 0.22,
        size_end: 0.12,
        lifetime: 1.6,
    });
}

// ── Game Entity Structures ──────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct LaserBolt {
    pos: Point3<f32>,
    active: bool,
    is_enemy: bool,
}

#[derive(Clone, Copy)]
struct Enemy {
    pos: Point3<f32>,
    initial_x: f32,
    initial_y: f32,
    phase: f32,
    health: i32,
    active: bool,
    shoot_timer: f32,
}

#[derive(Clone, Copy)]
struct AsteroidObj {
    pos: Point3<f32>,
    rot_axis: Vector3<f32>,
    rot_speed: f32,
    angle: f32,
    scale: f32,
    health: i32,
    active: bool,
}

#[derive(Clone, Copy)]
struct RingObj {
    pos: Point3<f32>,
    angle: f32,
    active: bool,
    collected: bool,
}

#[derive(Clone, Copy)]
struct ArchObj {
    pos: Point3<f32>,
    active: bool,
}

// ── Radio Transmission System ───────────────────────────────────────────────

#[derive(Clone, Copy)]
enum RadioSpeaker {
    Leader,
    Ace,
    Sparks,
    Hawk,
    Unit64,
}

struct RadioMessage {
    speaker: RadioSpeaker,
    text: &'static str,
}

fn trigger_radio(
    current: &mut Option<RadioMessage>,
    timer: &mut f32,
    speaker: RadioSpeaker,
    text: &'static str,
    duration: f32,
) {
    *timer = duration;
    *current = Some(RadioMessage { speaker, text });
}

fn spawn_enemy(
    enemies: &mut [Enemy],
    pos: Point3<f32>,
    initial_x: f32,
    initial_y: f32,
    phase: f32,
    health: i32,
    shoot_timer: f32,
) {
    for en in enemies.iter_mut() {
        if !en.active {
            *en = Enemy {
                pos,
                initial_x,
                initial_y,
                phase,
                health,
                active: true,
                shoot_timer,
            };
            return;
        }
    }
}

fn spawn_ring(rings: &mut [RingObj], pos: Point3<f32>) {
    for r in rings.iter_mut() {
        if !r.active {
            *r = RingObj {
                pos,
                angle: 0.0,
                active: true,
                collected: false,
            };
            return;
        }
    }
}

fn spawn_asteroid(
    asteroids: &mut [AsteroidObj],
    pos: Point3<f32>,
    rot_axis: Vector3<f32>,
    rot_speed: f32,
    scale: f32,
    health: i32,
) {
    for a in asteroids.iter_mut() {
        if !a.active {
            *a = AsteroidObj {
                pos,
                rot_axis,
                rot_speed,
                angle: 0.0,
                scale,
                health,
                active: true,
            };
            return;
        }
    }
}

fn spawn_arch(arches: &mut [ArchObj], pos: Point3<f32>) {
    for a in arches.iter_mut() {
        if !a.active {
            *a = ArchObj { pos, active: true };
            return;
        }
    }
}

fn init_level(
    enemies: &mut [Enemy],
    asteroids: &mut [AsteroidObj],
    rings: &mut [RingObj],
    arches: &mut [ArchObj],
) {
    for e in enemies.iter_mut() {
        e.active = false;
    }
    for a in asteroids.iter_mut() {
        a.active = false;
    }
    for r in rings.iter_mut() {
        r.active = false;
    }
    for a in arches.iter_mut() {
        a.active = false;
    }

    // Immediately populate the space corridor so all objects are visible from frame 1
    spawn_arch(arches, Point3::new(0.0, 0.0, -22.0));
    spawn_arch(arches, Point3::new(0.0, 0.0, -50.0));

    spawn_ring(rings, Point3::new(-2.2, 0.6, -14.0));
    spawn_ring(rings, Point3::new(2.2, -0.4, -28.0));
    spawn_ring(rings, Point3::new(0.0, 1.2, -40.0));

    spawn_enemy(
        enemies,
        Point3::new(-2.4, 0.8, -25.0),
        -2.4,
        0.8,
        0.0,
        2,
        1.5,
    );
    spawn_enemy(enemies, Point3::new(0.0, 1.8, -28.0), 0.0, 1.8, 1.5, 2, 1.8);
    spawn_enemy(enemies, Point3::new(2.4, 0.8, -25.0), 2.4, 0.8, 3.0, 2, 2.1);

    spawn_asteroid(
        asteroids,
        Point3::new(-3.6, 1.2, -34.0),
        Vector3::new(0.4, 1.0, 0.2).normalize(),
        1.2,
        1.2,
        2,
    );
    spawn_asteroid(
        asteroids,
        Point3::new(3.6, -1.0, -46.0),
        Vector3::new(0.8, 0.5, 0.3).normalize(),
        1.5,
        1.1,
        2,
    );
}

// ── Main Game Function ──────────────────────────────────────────────────────

fn main() {
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(WIDTH as u32, HEIGHT as u32));
    let mut window = Window::new(
        "STAR STRIKER 3D - Arrow Keys/WASD: Move | SPACE: Lasers | Z/X: Barrel Roll | ESC: Quit",
        &output_settings,
    );

    // ── 3D Engine Setup ─────────────────────────────────────────────────────
    let mut engine = K3dengine::new(WIDTH as u16, HEIGHT as u16);
    apply_default_caps(&mut engine);
    engine.apply_retro_style(RetroStyle::modern());
    engine.camera.set_near_far(0.2, 140.0);

    let mut zbuffer = vec![Z_MAX_VALUE; WIDTH * HEIGHT];
    let mut commands = CommandBuffer::<COMMAND_BUF_SIZE>::new();

    // ── Pre-build Geometries & Base Meshes ───────────────────────────────────
    let (ship_b_v, ship_b_f, ship_b_n) = make_ship_body();
    let ship_body_geom = Geometry {
        vertices: &ship_b_v,
        faces: &ship_b_f,
        colors: &[],
        lines: &[],
        normals: &ship_b_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (ship_c_v, ship_c_f, ship_c_n) = make_ship_canopy();
    let ship_canopy_geom = Geometry {
        vertices: &ship_c_v,
        faces: &ship_c_f,
        colors: &[],
        lines: &[],
        normals: &ship_c_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (ship_a_v, ship_a_f, ship_a_n) = make_ship_accents();
    let ship_accents_geom = Geometry {
        vertices: &ship_a_v,
        faces: &ship_a_f,
        colors: &[],
        lines: &[],
        normals: &ship_a_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (en_v, en_f, en_n) = make_enemy_fighter();
    let enemy_geom = Geometry {
        vertices: &en_v,
        faces: &en_f,
        colors: &[],
        lines: &[],
        normals: &en_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (ast_v, ast_f, ast_n) = make_asteroid();
    let asteroid_geom = Geometry {
        vertices: &ast_v,
        faces: &ast_f,
        colors: &[],
        lines: &[],
        normals: &ast_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (ring_v, ring_f, ring_n) = make_space_ring();
    let ring_geom = Geometry {
        vertices: &ring_v,
        faces: &ring_f,
        colors: &[],
        lines: &[],
        normals: &ring_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (arch_v, arch_f, arch_n) = make_space_arch();
    let arch_geom = Geometry {
        vertices: &arch_v,
        faces: &arch_f,
        colors: &[],
        lines: &[],
        normals: &arch_n,
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    let (lsr_v, lsr_f, _) = make_laser_bolt();
    let laser_geom = Geometry {
        vertices: &lsr_v,
        faces: &lsr_f,
        colors: &[],
        lines: &[],
        normals: &[], // Empty normals: disable backface culling for emissive laser bolts
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    };

    // Instantiate mesh instances for rendering with high-contrast palette
    let mut mesh_ship_body = K3dMesh::new(ship_body_geom);
    mesh_ship_body.set_color(Rgb565::new(31, 63, 31)); // Brilliant White

    let mut mesh_ship_canopy = K3dMesh::new(ship_canopy_geom);
    mesh_ship_canopy.set_color(Rgb565::new(0, 58, 31)); // Electric cyan glass

    let mut mesh_ship_accents = K3dMesh::new(ship_accents_geom);
    mesh_ship_accents.set_color(Rgb565::new(6, 26, 31)); // Deep royal blue blasters

    let mut mesh_enemy = K3dMesh::new(enemy_geom);
    mesh_enemy.set_color(Rgb565::new(31, 8, 8)); // Enemy crimson

    let mut mesh_asteroid = K3dMesh::new(asteroid_geom);
    mesh_asteroid.set_color(Rgb565::new(20, 42, 20)); // Well-lit space rock

    let mut mesh_ring = K3dMesh::new(ring_geom);
    mesh_ring.set_color(Rgb565::new(31, 58, 4)); // Shining gold ring

    let mut mesh_arch = K3dMesh::new(arch_geom);
    mesh_arch.set_color(Rgb565::new(12, 36, 42)); // Monolith teal

    let mut mesh_laser_player = K3dMesh::new(laser_geom);
    mesh_laser_player.set_color(Rgb565::new(0, 63, 10)); // Brilliant neon emerald green

    let mut mesh_laser_enemy = K3dMesh::new(laser_geom);
    mesh_laser_enemy.set_color(Rgb565::new(31, 24, 0)); // Enemy bright plasma red/orange

    // ── Game State ──────────────────────────────────────────────────────────
    let mut player_pos = Point3::new(0.0_f32, 0.0_f32, 0.0_f32);
    let mut player_roll = 0.0_f32;
    let mut player_pitch = 0.0_f32;
    let mut player_shield = 100i32;
    let mut score = 0u32;
    let mut hits_count = 0u32;
    let mut game_over = false;

    // Roll state (Z / X barrel roll)
    let mut barrel_roll_timer = 0.0_f32;
    let mut barrel_roll_dir = 0.0_f32;

    // Laser & weapon state
    let mut fire_cooldown = 0.0_f32;
    let mut lasers: [LaserBolt; 32] = [LaserBolt {
        pos: Point3::origin(),
        active: false,
        is_enemy: false,
    }; 32];

    // World Entities
    let mut enemies: [Enemy; 16] = [Enemy {
        pos: Point3::origin(),
        initial_x: 0.0,
        initial_y: 0.0,
        phase: 0.0,
        health: 0,
        active: false,
        shoot_timer: 0.0,
    }; 16];

    let mut asteroids: [AsteroidObj; 12] = [AsteroidObj {
        pos: Point3::origin(),
        rot_axis: Vector3::y(),
        rot_speed: 1.0,
        angle: 0.0,
        scale: 1.0,
        health: 2,
        active: false,
    }; 12];

    let mut rings: [RingObj; 8] = [RingObj {
        pos: Point3::origin(),
        angle: 0.0,
        active: false,
        collected: false,
    }; 8];

    let mut arches: [ArchObj; 6] = [ArchObj {
        pos: Point3::origin(),
        active: false,
    }; 6];

    init_level(&mut enemies, &mut asteroids, &mut rings, &mut arches);

    // Particle System
    let mut particles: ParticleSystem<1024> = ParticleSystem::new();
    let mut rng: u32 = 0x57A8_F014;

    // Pre-populate space starfield dust
    for _ in 0..40 {
        spawn_starfield_dust(&mut particles, &mut rng);
        particles.update(0.04, Vector3::zeros());
    }

    // Input tracking (smooth multi-key support)
    let mut held_keys: HashSet<Keycode> = HashSet::new();

    // Radio Chatter System
    let mut current_radio: Option<RadioMessage> = None;
    let mut radio_timer = 0.0f32;
    trigger_radio(
        &mut current_radio,
        &mut radio_timer,
        RadioSpeaker::Leader,
        "This is Leader! Approaching Sector Orion orbital defense zone.",
        4.5,
    );
    let mut ring_bonus_popup_timer = 0.0f32;

    // Wave spawning state
    let mut wave_timer = 2.4f32;
    let mut wave_index = 0u32;

    // Performance & Styles
    #[cfg(feature = "perfcounter")]
    let mut perf = PerformanceCounter::new();
    #[cfg(feature = "perfcounter")]
    perf.only_fps(true);

    let hud_font = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_WHITE);
    let hud_bold = MonoTextStyle::new(&FONT_8X13_BOLD, Rgb565::CSS_YELLOW);
    let hud_green = MonoTextStyle::new(&FONT_6X10, Rgb565::new(8, 63, 10));
    let hud_cyan = MonoTextStyle::new(&FONT_6X10, Rgb565::new(0, 55, 31));

    // Dynamic Lighting: Add overhead & fill point lights to brighten the ship and scene
    engine.clear_point_lights();
    engine.add_point_light(
        PointLight::new(Point3::new(0.0, 7.0, 6.0), Rgb565::new(31, 63, 31), 60.0)
            .with_intensity(1.8),
    );
    engine.add_point_light(
        PointLight::new(Point3::new(0.0, 4.0, -15.0), Rgb565::new(24, 48, 28), 80.0)
            .with_intensity(1.2),
    );

    // Directional Light: shining from above and slightly behind camera for maximum illumination
    let light_dir = Vector3::new(0.3, 0.85, 0.4).normalize();
    let light_mode = RenderMode::SolidLightDir(light_dir);

    mesh_ship_body.set_render_mode(light_mode.clone());
    mesh_ship_canopy.set_render_mode(light_mode.clone());
    mesh_ship_accents.set_render_mode(light_mode.clone());
    mesh_enemy.set_render_mode(light_mode.clone());
    mesh_asteroid.set_render_mode(light_mode.clone());
    mesh_ring.set_render_mode(light_mode.clone());
    mesh_arch.set_render_mode(light_mode.clone());
    mesh_laser_player.set_render_mode(RenderMode::Solid);
    mesh_laser_enemy.set_render_mode(RenderMode::Solid);

    display.clear(Rgb565::BLACK).unwrap();
    window.update(&display);

    let mut last_frame = Instant::now();

    'main_loop: loop {
        #[cfg(feature = "perfcounter")]
        perf.start_of_frame();

        let frame_start = Instant::now();
        let dt = frame_start
            .duration_since(last_frame)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        last_frame = frame_start;

        // ── Input Handling ──────────────────────────────────────────────────
        for event in window.events() {
            match event {
                SimulatorEvent::KeyDown { keycode, .. } => {
                    held_keys.insert(keycode);
                    match keycode {
                        Keycode::Escape => break 'main_loop,
                        Keycode::Z => {
                            if barrel_roll_timer <= 0.0 {
                                barrel_roll_timer = 0.55;
                                barrel_roll_dir = -1.0;
                                trigger_radio(
                                    &mut current_radio,
                                    &mut radio_timer,
                                    RadioSpeaker::Ace,
                                    "Do a barrel roll! (Z / X)",
                                    2.5,
                                );
                            }
                        }
                        Keycode::X => {
                            if barrel_roll_timer <= 0.0 {
                                barrel_roll_timer = 0.55;
                                barrel_roll_dir = 1.0;
                            }
                        }
                        Keycode::R if game_over => {
                            // Reset game
                            player_pos = Point3::new(0.0, 0.0, 0.0);
                            player_roll = 0.0;
                            player_pitch = 0.0;
                            player_shield = 100;
                            score = 0;
                            hits_count = 0;
                            game_over = false;
                            for l in &mut lasers {
                                l.active = false;
                            }
                            init_level(&mut enemies, &mut asteroids, &mut rings, &mut arches);
                            wave_index = 0;
                            wave_timer = 2.4;
                            trigger_radio(
                                &mut current_radio,
                                &mut radio_timer,
                                RadioSpeaker::Leader,
                                "Good luck, Star Striker squadron!",
                                3.0,
                            );
                        }
                        _ => {}
                    }
                }
                SimulatorEvent::KeyUp { keycode, .. } => {
                    held_keys.remove(&keycode);
                }
                SimulatorEvent::Quit => break 'main_loop,
                _ => {}
            }
        }

        // ── Player Movement Physics & Banking ───────────────────────────────
        let speed = 9.5 * dt;
        let mut target_roll = 0.0f32;
        let mut target_pitch = 0.0f32;

        if !game_over {
            // Horizontal Steering (Left / Right / A / D)
            if held_keys.contains(&Keycode::Left) || held_keys.contains(&Keycode::A) {
                player_pos.x -= speed;
                target_roll = 0.65; // Bank left
            }
            if held_keys.contains(&Keycode::Right) || held_keys.contains(&Keycode::D) {
                player_pos.x += speed;
                target_roll = -0.65; // Bank right
            }

            // Vertical Steering (Up / Down / W / S)
            if held_keys.contains(&Keycode::Up) || held_keys.contains(&Keycode::W) {
                player_pos.y += speed;
                target_pitch = 0.35; // Pitch up
            }
            if held_keys.contains(&Keycode::Down) || held_keys.contains(&Keycode::S) {
                player_pos.y -= speed;
                target_pitch = -0.35; // Pitch down
            }

            // Clamp player inside flight corridor bounds
            player_pos.x = player_pos.x.clamp(-5.2, 5.2);
            player_pos.y = player_pos.y.clamp(-3.2, 3.5);

            // Handle Barrel Roll Animation
            if barrel_roll_timer > 0.0 {
                barrel_roll_timer -= dt;
                let t = (0.55 - barrel_roll_timer) / 0.55;
                player_roll = barrel_roll_dir * t * 2.0 * PI;

                // Sparkles during barrel roll
                spawn_ring_sparkles(&mut particles, player_pos, &mut rng);
            } else {
                // Smoothly interpolate roll and pitch back to target / neutral
                player_roll += (target_roll - player_roll) * 10.0 * dt;
                player_pitch += (target_pitch - player_pitch) * 10.0 * dt;
            }

            // ── Shooting Lasers (SPACE) ─────────────────────────────────────
            fire_cooldown -= dt;
            if held_keys.contains(&Keycode::Space) && fire_cooldown <= 0.0 {
                fire_cooldown = 0.14; // rapid fire rate

                let rot = UnitQuaternion::from_euler_angles(player_roll, player_pitch, 0.0);
                let left_tip_local = Vector3::new(-2.45, 0.08, -0.2);
                let right_tip_local = Vector3::new(2.45, 0.08, -0.2);
                let left_pos = player_pos + rot * left_tip_local;
                let right_pos = player_pos + rot * right_tip_local;

                spawn_muzzle_flash(&mut particles, left_pos, &mut rng);
                spawn_muzzle_flash(&mut particles, right_pos, &mut rng);

                let mut spawned = 0;
                for bolt in &mut lasers {
                    if !bolt.active {
                        if spawned == 0 {
                            bolt.pos = left_pos;
                            bolt.active = true;
                            bolt.is_enemy = false;
                            spawned += 1;
                        } else if spawned == 1 {
                            bolt.pos = right_pos;
                            bolt.active = true;
                            bolt.is_enemy = false;
                            break;
                        }
                    }
                }
            }

            // Spawn Engine Thruster Particles
            spawn_engine_exhaust(&mut particles, player_pos, &mut rng);
        }

        // ── Starfield Dust & Warp Particles ─────────────────────────────────
        if rng % 2 == 0 {
            spawn_starfield_dust(&mut particles, &mut rng);
        }
        particles.update(dt, Vector3::zeros());

        // ── Wave Spawner & Scripting (Continuous Action) ────────────────────
        wave_timer -= dt;
        if wave_timer <= 0.0 && !game_over {
            wave_index += 1;
            wave_timer = 2.4; // Continuous rapid waves

            match wave_index % 5 {
                1 => {
                    // Wave 1: Enemy Delta Formation (V-Formation of 3)
                    trigger_radio(
                        &mut current_radio,
                        &mut radio_timer,
                        RadioSpeaker::Sparks,
                        "Enemy interceptors incoming at 12 o'clock!",
                        3.5,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(-2.8, 0.8, -55.0),
                        -2.8,
                        0.8,
                        0.0,
                        2,
                        1.4,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(0.0, 1.8, -60.0),
                        0.0,
                        1.8,
                        1.5,
                        2,
                        1.6,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(2.8, 0.8, -55.0),
                        2.8,
                        0.8,
                        3.0,
                        2,
                        1.8,
                    );
                }
                2 => {
                    // Wave 2: Golden Recovery Rings in Slalom Path
                    trigger_radio(
                        &mut current_radio,
                        &mut radio_timer,
                        RadioSpeaker::Unit64,
                        "Supply rings detected. Fly through them to repair shields!",
                        4.0,
                    );
                    spawn_ring(&mut rings, Point3::new(-2.2, -0.4, -48.0));
                    spawn_ring(&mut rings, Point3::new(0.0, 1.4, -58.0));
                    spawn_ring(&mut rings, Point3::new(2.2, 0.2, -68.0));
                }
                3 => {
                    // Wave 3: Asteroid Field
                    trigger_radio(
                        &mut current_radio,
                        &mut radio_timer,
                        RadioSpeaker::Hawk,
                        "Watch it, Leader! Heavy asteroid cluster ahead!",
                        3.5,
                    );
                    spawn_asteroid(
                        &mut asteroids,
                        Point3::new(-3.5, 1.4, -50.0),
                        Vector3::new(0.5, 0.9, 0.2).normalize(),
                        1.4,
                        1.2,
                        2,
                    );
                    spawn_asteroid(
                        &mut asteroids,
                        Point3::new(0.0, -1.8, -56.0),
                        Vector3::new(0.2, 0.7, 0.8).normalize(),
                        1.2,
                        1.4,
                        2,
                    );
                    spawn_asteroid(
                        &mut asteroids,
                        Point3::new(3.5, 1.2, -62.0),
                        Vector3::new(0.8, 0.3, 0.5).normalize(),
                        1.6,
                        1.1,
                        2,
                    );
                }
                4 => {
                    // Wave 4: Monolith Gates & Arches
                    trigger_radio(
                        &mut current_radio,
                        &mut radio_timer,
                        RadioSpeaker::Ace,
                        "Fly straight through the space gates!",
                        3.5,
                    );
                    spawn_arch(&mut arches, Point3::new(0.0, 0.0, -52.0));
                    spawn_ring(&mut rings, Point3::new(0.0, 0.5, -52.0));
                    spawn_arch(&mut arches, Point3::new(0.0, 0.0, -78.0));
                }
                0 => {
                    // Wave 5: Ace Squadron (4 Weaving Fighters)
                    trigger_radio(
                        &mut current_radio,
                        &mut radio_timer,
                        RadioSpeaker::Leader,
                        "Bogey squadron! Don't let them flank us!",
                        3.5,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(-3.2, 1.2, -55.0),
                        -3.2,
                        1.2,
                        0.0,
                        3,
                        1.2,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(-1.0, -1.0, -58.0),
                        -1.0,
                        -1.0,
                        1.5,
                        3,
                        1.4,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(1.0, -1.0, -58.0),
                        1.0,
                        -1.0,
                        3.0,
                        3,
                        1.4,
                    );
                    spawn_enemy(
                        &mut enemies,
                        Point3::new(3.2, 1.2, -55.0),
                        3.2,
                        1.2,
                        4.5,
                        3,
                        1.2,
                    );
                }
                _ => {}
            }
        }

        // ── Entity Simulation & Collisions ──────────────────────────────────
        let scroll_speed = 18.0 * dt;

        // 1. Update Laser Projectiles
        for bolt in &mut lasers {
            if !bolt.active {
                continue;
            }
            // Spawn glowing trail particles behind the laser bolt
            spawn_laser_trail(&mut particles, bolt.pos, bolt.is_enemy, &mut rng);

            if bolt.is_enemy {
                // Enemy laser flying towards player (+Z)
                bolt.pos.z += 28.0 * dt;
                if bolt.pos.z > 8.0 {
                    bolt.active = false;
                } else if !game_over && (bolt.pos - player_pos).norm() < 1.4 {
                    // Hit player!
                    bolt.active = false;
                    if barrel_roll_timer <= 0.0 {
                        player_shield = (player_shield - 15).max(0);
                        spawn_explosion(&mut particles, player_pos, &mut rng, false);
                        if player_shield <= 0 {
                            game_over = true;
                            spawn_explosion(&mut particles, player_pos, &mut rng, true);
                            trigger_radio(
                                &mut current_radio,
                                &mut radio_timer,
                                RadioSpeaker::Sparks,
                                "LEADER DOWN! (Press R to restart)",
                                10.0,
                            );
                        }
                    } else {
                        // Deflected by barrel roll!
                        spawn_ring_sparkles(&mut particles, player_pos, &mut rng);
                    }
                }
            } else {
                // Player laser flying forward (-Z) at calibrated cinematic arcade speed
                bolt.pos.z -= 58.0 * dt;
                if bolt.pos.z < -80.0 {
                    bolt.active = false;
                }

                // Check collision against Enemies
                for en in &mut enemies {
                    if en.active && (bolt.pos - en.pos).norm() < 2.4 {
                        bolt.active = false;
                        en.health -= 1;
                        spawn_explosion(&mut particles, bolt.pos, &mut rng, false);
                        if en.health <= 0 {
                            en.active = false;
                            score += 150;
                            hits_count += 1;
                            spawn_explosion(&mut particles, en.pos, &mut rng, true);
                        }
                        break;
                    }
                }

                // Check collision against Asteroids
                for ast in &mut asteroids {
                    if ast.active && (bolt.pos - ast.pos).norm() < (2.0 * ast.scale) {
                        bolt.active = false;
                        ast.health -= 1;
                        spawn_explosion(&mut particles, bolt.pos, &mut rng, false);
                        if ast.health <= 0 {
                            ast.active = false;
                            score += 80;
                            hits_count += 1;
                            spawn_explosion(&mut particles, ast.pos, &mut rng, true);
                        }
                        break;
                    }
                }
            }
        }

        // 2. Update Enemies
        for en in &mut enemies {
            if !en.active {
                continue;
            }
            en.pos.z += scroll_speed * 1.25;
            en.phase += dt * 2.5;
            en.pos.x = en.initial_x + (en.phase).sin() * 2.2;
            en.pos.y = en.initial_y + (en.phase * 0.7).cos() * 1.2;

            // Enemy shoots plasma at player
            en.shoot_timer -= dt;
            if en.shoot_timer <= 0.0 && en.pos.z < -10.0 && en.pos.z > -50.0 {
                en.shoot_timer = 2.0;
                for bolt in &mut lasers {
                    if !bolt.active {
                        bolt.pos = en.pos;
                        bolt.active = true;
                        bolt.is_enemy = true;
                        break;
                    }
                }
            }

            // Player body collision with enemy
            if !game_over && (en.pos - player_pos).norm() < 2.2 {
                en.active = false;
                spawn_explosion(&mut particles, en.pos, &mut rng, true);
                if barrel_roll_timer <= 0.0 {
                    player_shield = (player_shield - 25).max(0);
                    if player_shield <= 0 {
                        game_over = true;
                        spawn_explosion(&mut particles, player_pos, &mut rng, true);
                    }
                }
            }

            if en.pos.z > 10.0 {
                en.active = false;
            }
        }

        // 3. Update Asteroids
        for ast in &mut asteroids {
            if !ast.active {
                continue;
            }
            ast.pos.z += scroll_speed * 1.1;
            ast.angle += ast.rot_speed * dt;

            // Player collision with asteroid
            if !game_over && (ast.pos - player_pos).norm() < (1.8 * ast.scale) {
                ast.active = false;
                spawn_explosion(&mut particles, ast.pos, &mut rng, true);
                if barrel_roll_timer <= 0.0 {
                    player_shield = (player_shield - 30).max(0);
                    if player_shield <= 0 {
                        game_over = true;
                        spawn_explosion(&mut particles, player_pos, &mut rng, true);
                    }
                }
            }

            if ast.pos.z > 10.0 {
                ast.active = false;
            }
        }

        // 4. Update Golden Rings
        for ring in &mut rings {
            if !ring.active {
                continue;
            }
            ring.pos.z += scroll_speed;
            ring.angle += 1.8 * dt;

            // Spawn radiant sparkles around the ring so it glitters in the darkness of space
            if !ring.collected && ring.pos.z < 2.0 && ring.pos.z > -60.0 && (rng % 2 == 0) {
                let r_angle = lcg(&mut rng) * 2.0 * PI;
                let r_dist = 2.8;
                let spark_pos = Point3::new(
                    ring.pos.x + r_angle.cos() * r_dist,
                    ring.pos.y + r_angle.sin() * r_dist,
                    ring.pos.z,
                );
                particles.spawn(ParticleSpawn {
                    position: spark_pos,
                    velocity: Vector3::new(0.0, 0.0, 3.0),
                    acceleration: Vector3::zeros(),
                    color_start: Rgb565::new(31, 63, 12),
                    color_end: Rgb565::new(31, 44, 0),
                    size_start: 0.28,
                    size_end: 0.04,
                    lifetime: 0.25,
                });
            }

            // Check if player flew through ring!
            if !ring.collected && (ring.pos.z - player_pos.z).abs() < 2.2 {
                let d_xy =
                    Vector3::new(ring.pos.x - player_pos.x, ring.pos.y - player_pos.y, 0.0).norm();
                if d_xy < 2.8 {
                    ring.collected = true;
                    player_shield = (player_shield + 25).min(100);
                    score += 500;
                    ring_bonus_popup_timer = 2.0;
                    spawn_ring_sparkles(&mut particles, ring.pos, &mut rng);
                }
            }

            if ring.pos.z > 10.0 {
                ring.active = false;
            }
        }

        // 5. Update Space Arches
        for arch in &mut arches {
            if !arch.active {
                continue;
            }
            arch.pos.z += scroll_speed;
            if arch.pos.z > 15.0 {
                arch.active = false;
            }
        }

        // Radio Message Timer
        if radio_timer > 0.0 {
            radio_timer -= dt;
            if radio_timer <= 0.0 {
                current_radio = None;
            }
        }
        if ring_bonus_popup_timer > 0.0 {
            ring_bonus_popup_timer -= dt;
        }

        // ── Camera Update (Dynamic Third-Person Chase Cam) ───────────────────
        let cam_pos = Point3::new(player_pos.x * 0.4, player_pos.y * 0.4 + 1.6, 6.2);
        let cam_target = Point3::new(player_pos.x * 0.8, player_pos.y * 0.8 + 0.3, -30.0);
        engine.camera.set_position(cam_pos);
        engine.camera.set_target(cam_target);

        // ── 3D Scene Rendering ──────────────────────────────────────────────
        display.clear(Rgb565::BLACK).unwrap();
        zbuffer.fill(Z_MAX_VALUE);
        commands.clear();

        // 1. Draw Space Arches
        for arch in &arches {
            if arch.active {
                mesh_arch.set_position(arch.pos.x, arch.pos.y, arch.pos.z);
                mesh_arch.set_attitude(0.0, 0.0, 0.0);
                engine
                    .record(std::iter::once(&mesh_arch), &mut commands, None)
                    .ok();
            }
        }

        // 2. Draw Golden Rings
        for ring in &rings {
            if ring.active && !ring.collected {
                mesh_ring.set_position(ring.pos.x, ring.pos.y, ring.pos.z);
                mesh_ring.set_attitude(0.0, 0.0, ring.angle);
                engine
                    .record(std::iter::once(&mesh_ring), &mut commands, None)
                    .ok();
            }
        }

        // 3. Draw Asteroids
        for ast in &asteroids {
            if ast.active {
                mesh_asteroid.set_position(ast.pos.x, ast.pos.y, ast.pos.z);
                let q = UnitQuaternion::from_axis_angle(
                    &nalgebra::Unit::new_normalize(ast.rot_axis),
                    ast.angle,
                );
                mesh_asteroid.set_rotation(q);
                engine
                    .record(std::iter::once(&mesh_asteroid), &mut commands, None)
                    .ok();
            }
        }

        // 4. Draw Enemies
        for en in &enemies {
            if en.active {
                mesh_enemy.set_position(en.pos.x, en.pos.y, en.pos.z);
                // Banking slightly with motion
                let bank = (en.phase).cos() * 0.4;
                mesh_enemy.set_attitude(bank, 0.0, 0.0);
                engine
                    .record(std::iter::once(&mesh_enemy), &mut commands, None)
                    .ok();
            }
        }

        // 5. Draw Lasers
        for bolt in &lasers {
            if bolt.active {
                if bolt.is_enemy {
                    mesh_laser_enemy.set_position(bolt.pos.x, bolt.pos.y, bolt.pos.z);
                    mesh_laser_enemy.set_attitude(0.0, 0.0, 0.0);
                    engine
                        .record(std::iter::once(&mesh_laser_enemy), &mut commands, None)
                        .ok();
                } else {
                    mesh_laser_player.set_position(bolt.pos.x, bolt.pos.y, bolt.pos.z);
                    mesh_laser_player.set_attitude(0.0, 0.0, 0.0);
                    engine
                        .record(std::iter::once(&mesh_laser_player), &mut commands, None)
                        .ok();
                }
            }
        }

        // 6. Draw Player Star Striker (Body + Cockpit + Accents)
        if !game_over {
            let roll_q = UnitQuaternion::from_euler_angles(player_roll, player_pitch, 0.0);

            mesh_ship_body.set_position(player_pos.x, player_pos.y, player_pos.z);
            mesh_ship_body.set_rotation(roll_q);

            mesh_ship_canopy.set_position(player_pos.x, player_pos.y, player_pos.z);
            mesh_ship_canopy.set_rotation(roll_q);

            mesh_ship_accents.set_position(player_pos.x, player_pos.y, player_pos.z);
            mesh_ship_accents.set_rotation(roll_q);

            engine
                .record(
                    [&mesh_ship_body, &mesh_ship_canopy, &mesh_ship_accents]
                        .iter()
                        .copied(),
                    &mut commands,
                    None,
                )
                .ok();
        }

        // 7. Record Particles
        particles.record(&engine, &mut commands);

        // Execute all 3D draw commands
        let mut frame = FrameCtx {
            zbuffer: &mut zbuffer,
            width: WIDTH,
            height: HEIGHT,
        };
        engine
            .execute::<_, COMMAND_BUF_SIZE>(&mut display, &mut frame, &commands, None)
            .unwrap();

        // ── 2D HUD & Retro Arcade UI Overlay ────────────────────────────────

        // 1. Aiming Reticle (Projected in front of ship)
        if !game_over {
            let aim_3d = Point3::new(player_pos.x, player_pos.y, -25.0);
            if let Some(aim_2d) =
                engine.transform_point(&[aim_3d.x, aim_3d.y, aim_3d.z], engine.camera.vp_matrix)
            {
                let rx = aim_2d.x;
                let ry = aim_2d.y;
                let reticle_color = Rgb565::new(0, 63, 20); // Bright emerald

                // Corner brackets [ + ]
                let sz = 7i32;
                Line::new(Point::new(rx - sz, ry - sz), Point::new(rx - 2, ry - sz))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();
                Line::new(Point::new(rx - sz, ry - sz), Point::new(rx - sz, ry - 2))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();

                Line::new(Point::new(rx + sz, ry - sz), Point::new(rx + 2, ry - sz))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();
                Line::new(Point::new(rx + sz, ry - sz), Point::new(rx + sz, ry - 2))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();

                Line::new(Point::new(rx - sz, ry + sz), Point::new(rx - 2, ry + sz))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();
                Line::new(Point::new(rx - sz, ry + sz), Point::new(rx - sz, ry + 2))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();

                Line::new(Point::new(rx + sz, ry + sz), Point::new(rx + 2, ry + sz))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();
                Line::new(Point::new(rx + sz, ry + sz), Point::new(rx + sz, ry + 2))
                    .into_styled(PrimitiveStyle::with_stroke(reticle_color, 1))
                    .draw(&mut display)
                    .ok();

                // Center cross dot
                Pixel(Point::new(rx, ry), Rgb565::CSS_YELLOW)
                    .draw(&mut display)
                    .ok();
            }
        }

        // 2. Top Header Bar: SHIELD GAUGE & SCORE
        // Shield label & background bar
        Text::new("SHIELD", Point::new(8, 14), hud_font)
            .draw(&mut display)
            .ok();

        let bar_x = 52i32;
        let bar_y = 6i32;
        let bar_w = 70i32;
        let bar_h = 9i32;

        // Border box
        Rectangle::new(
            Point::new(bar_x, bar_y),
            Size::new(bar_w as u32, bar_h as u32),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(0, 40, 31), 1))
        .draw(&mut display)
        .ok();

        // Fill bar based on health percentage
        let fill_w = ((bar_w - 2) * player_shield / 100).max(0);
        let shield_color = if player_shield > 60 {
            Rgb565::new(4, 60, 10) // Green
        } else if player_shield > 30 {
            Rgb565::new(31, 55, 0) // Yellow
        } else {
            Rgb565::new(31, 6, 0) // Red
        };

        if fill_w > 0 {
            Rectangle::new(
                Point::new(bar_x + 1, bar_y + 1),
                Size::new(fill_w as u32, (bar_h - 2) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(shield_color))
            .draw(&mut display)
            .ok();
        }

        // Lasers type indicator
        Text::new("LASER: TWIN", Point::new(132, 14), hud_cyan)
            .draw(&mut display)
            .ok();

        // Score & Hit Counters
        let hits_str = format!("HITS: {:02}", hits_count);
        Text::new(&hits_str, Point::new(212, 14), hud_font)
            .draw(&mut display)
            .ok();

        let score_str = format!("SCORE: {:05}", score);
        Text::new(&score_str, Point::new(212, 26), hud_bold)
            .draw(&mut display)
            .ok();

        // 3. Ring Bonus Banner
        if ring_bonus_popup_timer > 0.0 {
            let ring_banner = "★ SHIELD REPAIRED +500 ★";
            Text::with_alignment(
                ring_banner,
                Point::new((WIDTH / 2) as i32, 45),
                hud_bold,
                Alignment::Center,
            )
            .draw(&mut display)
            .ok();
        }

        // 4. Radio Transmission Dialog
        if let Some(ref msg) = current_radio {
            let box_y = (HEIGHT - 44) as i32;
            let box_h = 40u32;

            // Translucent dark dialog frame
            Rectangle::new(Point::new(6, box_y), Size::new((WIDTH - 12) as u32, box_h))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(Rgb565::new(2, 6, 12))
                        .stroke_color(Rgb565::new(0, 42, 31))
                        .stroke_width(1)
                        .build(),
                )
                .draw(&mut display)
                .ok();

            // Speaker Tag Box
            let (tag, tag_color) = match msg.speaker {
                RadioSpeaker::Leader => ("[ LEADER ]", Rgb565::new(31, 55, 10)),
                RadioSpeaker::Ace => ("[ ACE ]", Rgb565::new(31, 35, 12)),
                RadioSpeaker::Sparks => ("[ SPARKS ]", Rgb565::new(8, 60, 15)),
                RadioSpeaker::Hawk => ("[ HAWK ]", Rgb565::new(6, 40, 31)),
                RadioSpeaker::Unit64 => ("[ UNIT-64 ]", Rgb565::new(26, 52, 26)),
            };

            let speaker_style = MonoTextStyle::new(&FONT_6X10, tag_color);
            Text::new(tag, Point::new(12, box_y + 12), speaker_style)
                .draw(&mut display)
                .ok();

            // Message text
            Text::new(msg.text, Point::new(12, box_y + 26), hud_font)
                .draw(&mut display)
                .ok();
        }

        // 5. Game Over Banner
        if game_over {
            Rectangle::new(Point::new(40, 80), Size::new((WIDTH - 80) as u32, 70))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(Rgb565::new(16, 2, 2))
                        .stroke_color(Rgb565::new(31, 0, 0))
                        .stroke_width(2)
                        .build(),
                )
                .draw(&mut display)
                .ok();

            Text::with_alignment(
                "MISSION FAILED",
                Point::new((WIDTH / 2) as i32, 108),
                hud_bold,
                Alignment::Center,
            )
            .draw(&mut display)
            .ok();

            Text::with_alignment(
                "Press 'R' to Retry Mission",
                Point::new((WIDTH / 2) as i32, 130),
                hud_font,
                Alignment::Center,
            )
            .draw(&mut display)
            .ok();
        }

        // 6. Perf Counter (FPS)
        #[cfg(feature = "perfcounter")]
        {
            perf.print();
            Text::new(
                perf.get_text(),
                Point::new(6, HEIGHT as i32 - 48),
                hud_green,
            )
            .draw(&mut display)
            .ok();
        }

        window.update(&display);

        // Maintain smooth 60 FPS cap
        const FRAME_BUDGET: Duration = Duration::from_millis(16);
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_BUDGET {
            thread::sleep(FRAME_BUDGET - elapsed);
        }
    }
}
