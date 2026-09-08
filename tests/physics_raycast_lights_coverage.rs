use embedded_3dgfx::absm::*;
use embedded_3dgfx::bounds::*;
use embedded_3dgfx::camera::Ray;
use embedded_3dgfx::lights::*;
use embedded_3dgfx::physics::*;
use embedded_3dgfx::pool::*;
use embedded_3dgfx::ray_primitive::*;
use embedded_3dgfx::tween::*;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use nalgebra::{Point3, Vector3};

#[test]
fn test_physics_world_and_colliders() {
    let mut world = PhysicsWorld::<16, 32>::new();
    world.set_gravity(Vector3::new(0.0, -9.81, 0.0));

    let rb_id = world.add_body(RigidBody::new(1.0)).unwrap();
    assert!(world.body(rb_id).is_some());

    world.step::<16>(0.016);
    let pos = world.body(rb_id).unwrap().position;
    assert!(pos.y <= 0.0);
}

#[test]
fn test_raycast_primitives() {
    let ray = Ray::new(Vector3::new(0.0, 0.0, -5.0), Vector3::new(0.0, 0.0, 1.0));
    let hit_sphere = ray_intersects_sphere(&ray, Vector3::new(0.0, 0.0, 0.0), 1.0);
    assert!(hit_sphere.is_some());

    let hit_plane = ray_intersects_plane(
        &ray,
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, -1.0),
    );
    assert!(hit_plane.is_some());

    let hit_disc = ray_intersects_disc(
        &ray,
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, -1.0),
        2.0,
    );
    assert!(hit_disc.is_some());

    let aabb = Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
    let hit_aabb = ray_intersects_aabb(&ray, &aabb);
    assert!(hit_aabb.is_some());
}

#[test]
fn test_lights_and_attenuation() {
    let p_light = PointLight::new(Point3::new(0.0, 5.0, 0.0), Rgb565::RED, 10.0);
    assert_eq!(p_light.radius, 10.0);
}

#[test]
fn test_absm_state_machine() {
    let mut absm = AnimationStateMachine::<4, 8, 4>::new(0);
    let tr = Transition {
        from: 0,
        to: 1,
        fade_duration: 0.1,
        rule: TransitionRule::Immediate,
    };
    absm.add_transition(tr);
    absm.set_param_float(0, 1.0);
    absm.update(0.016);
    assert_eq!(absm.current_state(), 0);
}

#[test]
fn test_pool_allocation_and_free() {
    let mut pool = Pool::<u32, 8>::new();
    let h1 = pool.spawn(42).unwrap();
    let h2 = pool.spawn(100).unwrap();

    assert_eq!(*pool.get(h1).unwrap(), 42);
    assert_eq!(*pool.get(h2).unwrap(), 100);

    pool.free(h1);
    assert!(pool.get(h1).is_none());
}

#[test]
fn test_tween_easings() {
    let mut t = Tween::new(0.0, 100.0, 1.0, Easing::EaseInOutCubic);
    assert_eq!(t.value(), 0.0);
    t.advance(0.5);
    assert!(t.value() > 0.0 && t.value() < 100.0);
    t.advance(0.5);
    assert_eq!(t.value(), 100.0);
}
