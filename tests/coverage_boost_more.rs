#![allow(unused_imports)]
use core::future::Future;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use embedded_3dgfx::ZDepth;
use embedded_3dgfx::command_buffer::{CommandBuffer, RenderCommand};
use embedded_3dgfx::config::*;
use embedded_3dgfx::display_backend::*;
use embedded_3dgfx::draw::{
    DepthInterpolationMode, DitherConfig, FogConfig, InterlaceField, ScreenDoorConfig,
    draw_zbuffered_with_options,
};
#[cfg(feature = "textured")]
use embedded_3dgfx::draw::{draw_zbuffered_with_textures, draw_zbuffered_with_textures_mapped};
use embedded_3dgfx::engine::K3dengine;
use embedded_3dgfx::error::{BudgetKind, RenderError};
#[cfg(feature = "lighting")]
use embedded_3dgfx::lights::*;
use embedded_3dgfx::mesh::{Geometry, K3dMesh, RenderMode};
#[cfg(feature = "painters")]
use embedded_3dgfx::painters::DepthSortedTriangle;
#[cfg(feature = "physics")]
use embedded_3dgfx::physics::*;
use embedded_3dgfx::primitive::DrawPrimitive;
use embedded_3dgfx::renderer::*;
use embedded_3dgfx::retro::*;
use embedded_3dgfx::swapchain::*;
#[cfg(feature = "textured")]
use embedded_3dgfx::texture::*;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_framebuf::{FrameBuf, backends::EndianCorrectedBuffer};
use nalgebra::{Matrix4, Point2, Point3, Vector3};

fn make_static_slice<T: Copy + Default>(n: usize) -> &'static mut [T] {
    vec![T::default(); n].leak()
}

fn dummy_waker() -> Waker {
    fn raw_waker() -> RawWaker {
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            raw_waker()
        }
        let vtable = &RawWakerVTable::new(clone, noop, noop, noop);
        RawWaker::new(std::ptr::null(), vtable)
    }
    unsafe { Waker::from_raw(raw_waker()) }
}

fn block_on<F: Future>(mut fut: F) -> F::Output {
    let waker = dummy_waker();
    let mut cx = Context::from_waker(&waker);
    let mut pin = unsafe { core::pin::Pin::new_unchecked(&mut fut) };
    match pin.as_mut().poll(&mut cx) {
        Poll::Ready(res) => res,
        Poll::Pending => panic!("Future pending in block_on"),
    }
}

#[test]
fn test_swapchain_full_coverage() {
    let front = make_static_slice::<Rgb565>(16 * 16);
    let back = make_static_slice::<Rgb565>(16 * 16);
    let backend = SimulatorBackend::new();

    let mut sc = StandardSwapChain::<16, 16, _>::from_static_slices(front, back, false, backend);

    assert_eq!(sc.dimensions(), (16, 16));
    assert_eq!(sc.frame_count(), 0);
    assert_eq!(sc.current_interlace_field(), InterlaceField::Even);
    assert!(sc.is_ready());
    assert!(sc.get_front_buffer().is_some());

    let _bb = sc.get_back_buffer();

    assert!(sc.present().is_ok());
    assert_eq!(sc.frame_count(), 1);
    assert_eq!(sc.current_interlace_field(), InterlaceField::Odd);

    assert!(sc.try_present().is_ok());
    assert_eq!(sc.frame_count(), 2);

    let region = DisplayRegion::new(0, 0, 8, 8);
    assert!(sc.present_region(region).is_ok());
    assert_eq!(sc.frame_count(), 3);

    assert!(sc.try_present_region(region).is_ok());
    assert_eq!(sc.frame_count(), 4);

    sc.wait_for_vsync();
    sc.reset_frame_count();
    assert_eq!(sc.frame_count(), 0);

    block_on(async {
        sc.wait_for_vsync_async().await;
        assert!(sc.present_async().await.is_ok());
        assert!(sc.present_region_async(region).await.is_ok());
    });
}

#[cfg(feature = "triple-buffering")]
#[test]
fn test_triple_swapchain_coverage() {
    let d = make_static_slice::<Rgb565>(16 * 16);
    let r = make_static_slice::<Rgb565>(16 * 16);
    let m = make_static_slice::<Rgb565>(16 * 16);
    let backend = SimulatorBackend::new();

    let mut tsc = StandardTripleSwapChain::<16, 16, _>::from_static_slices(d, r, m, false, backend);

    assert_eq!(tsc.frame_count(), 0);
    let _rb = tsc.get_render_buffer();

    assert!(tsc.present().is_ok());
    assert_eq!(tsc.frame_count(), 1);

    assert!(tsc.try_present().is_ok());
    assert_eq!(tsc.frame_count(), 2);

    block_on(async {
        assert!(tsc.present_async().await.is_ok());
    });
}

#[test]
fn test_renderer_validation_and_dirty_region() {
    let mut zbuf = vec![ZDepth::MAX; 100];
    let invalid_frame = FrameCtx {
        zbuffer: &mut zbuf,
        width: 16,
        height: 16, // expected 256
    };
    let err = invalid_frame.validate().unwrap_err();
    assert!(matches!(
        err,
        RenderError::OutOfBudget(BudgetKind::ZBufferLength {
            expected: 256,
            got: 100
        })
    ));

    let valid_frame = FrameCtx {
        zbuffer: &mut zbuf,
        width: 10,
        height: 10,
    };
    assert!(valid_frame.validate().is_ok());

    assert_eq!(PickQuery::new(5, 10), PickQuery { x: 5, y: 10 });
}

#[test]
fn test_mesh_validity_and_edge_cases() {
    let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let invalid_faces = [[0, 1, 99]]; // index out of bounds
    let geom_bad_face = Geometry {
        vertices: &vertices,
        faces: &invalid_faces,
        ..Default::default()
    };

    let result = std::panic::catch_unwind(|| {
        let _mesh = K3dMesh::new(geom_bad_face);
    });
    assert!(result.is_err());

    let faces = [[0, 1, 2], [1, 2, 0]];
    let edges: heapless::Vec<(usize, usize), 2> = Geometry::lines_from_faces::<2>(&faces);
    assert_eq!(edges.len(), 2);

    let norm =
        Geometry::calculate_face_normal(&[0.0, 0.0, 0.0], &[0.0, 0.0, 0.0], &[0.0, 0.0, 0.0]);
    assert_eq!(norm, [0.0, 1.0, 0.0]);
}

#[test]
fn test_pipeline_outlines_large_vertex_cache_and_point_lights() {
    let mut engine = K3dengine::new(240, 240);
    engine.camera.set_position(Point3::new(0.0, 0.0, -5.0));
    engine.camera.set_target(Point3::origin());

    #[cfg(feature = "lighting")]
    {
        let light = PointLight::new(Point3::new(0.0, 0.0, -2.0), Rgb565::RED, 10.0);
        assert!(engine.add_point_light(light));
    }

    let mut verts_vec = Vec::new();
    for i in 0..270 {
        verts_vec.push([(i % 10) as f32 * 0.1, (i / 10) as f32 * 0.1, 0.0]);
    }
    let faces = [[0, 1, 260], [260, 261, 262]];
    let normals = [[0.0, 0.0, -1.0], [0.0, 0.0, -1.0]];

    let geom = Geometry {
        vertices: &verts_vec,
        faces: &faces,
        normals: &normals,
        ..Default::default()
    };

    let mut mesh = K3dMesh::new(geom);
    mesh.outline_color = Some(Rgb565::YELLOW);
    mesh.outline_width = 0.05;
    mesh.render_mode = RenderMode::Solid;

    let mut cmds = CommandBuffer::<64>::new();
    let res = engine.record([&mesh], &mut cmds, None);
    assert!(res.is_ok());

    #[cfg(feature = "textured")]
    {
        let uvs = vec![[0.5, 0.5]; 270];
        let geom_tex = Geometry {
            vertices: &verts_vec,
            faces: &faces,
            normals: &normals,
            uvs: &uvs,
            texture_id: Some(1),
            ..Default::default()
        };
        let mut mesh_tex = K3dMesh::new(geom_tex);
        mesh_tex.render_mode = RenderMode::Textured;

        let mut cmds2 = CommandBuffer::<64>::new();
        let res = engine.record([&mesh_tex], &mut cmds2, None);
        assert!(res.is_ok());

        mesh_tex.render_mode = RenderMode::MatCap;
        let mut cmds3 = CommandBuffer::<64>::new();
        let res = engine.record([&mesh_tex], &mut cmds3, None);
        assert!(res.is_ok());
    }

    #[cfg(feature = "lighting")]
    {
        engine.clear_point_lights();
    }
}

#[cfg(feature = "painters")]
#[test]
fn test_painters_algorithm_additional_modes() {
    let mut engine = K3dengine::new(240, 240);
    engine.camera.set_position(Point3::new(0.0, 0.0, -10.0));
    engine.camera.set_target(Point3::origin());

    let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let faces = [[0, 1, 2]];
    let normals = [[0.0, 0.0, -1.0]];

    let geom = Geometry {
        vertices: &vertices,
        faces: &faces,
        normals: &normals,
        ..Default::default()
    };

    let mut mesh = K3dMesh::new(geom);

    #[cfg(feature = "lighting")]
    {
        mesh.render_mode = RenderMode::SolidLightDir(Vector3::new(0.0, 0.0, 1.0));
        let mut scratch = vec![DepthSortedTriangle::default(); 10];
        let count = engine.render_painters_algorithm([&mesh], &mut scratch, |_| {});
        assert_eq!(count, 1);

        mesh.render_mode = RenderMode::BlinnPhong {
            light_dir: Vector3::new(0.0, 0.0, 1.0),
            specular_intensity: 0.5,
            shininess: 16.0,
        };
        let count = engine.render_painters_algorithm([&mesh], &mut scratch, |_| {});
        assert_eq!(count, 1);
    }
}

#[cfg(feature = "lighting")]
#[test]
fn test_lights_and_tonemapping_extra() {
    let mut spot_set = SpotLightSet::<4>::new();
    assert!(spot_set.is_empty());
    assert_eq!(spot_set.len(), 0);

    let spot = SpotLight::new(
        Point3::origin(),
        Vector3::new(0.0, 0.0, 1.0),
        Rgb565::RED,
        10.0,
        0.2,
        0.4,
    )
    .with_intensity(1.5);

    assert!(spot_set.add(spot));
    assert_eq!(spot_set.len(), 1);
    assert!(!spot_set.is_empty());

    let c = spot_set.accumulate(Point3::new(0.0, 0.0, 2.0));
    assert!(c.r() > 0);

    let c_tm = spot_set.accumulate_tonemapped(Point3::new(0.0, 0.0, 2.0), 31);
    assert!(c_tm.r() > 0);

    spot_set.clear();
    assert!(spot_set.is_empty());

    let mut point_set = PointLightSet::<4>::new();
    let point = PointLight::new(Point3::origin(), Rgb565::GREEN, 5.0).with_intensity(2.0);
    point_set.add(point);
    let pt_tm = point_set.accumulate_tonemapped(Point3::origin(), 31);
    assert!(pt_tm.g() > 0);

    assert_eq!(reinhard_extended_tonemap(1.0, 0.0), 1.0);
    assert_eq!(windowed_distance_attenuation(1.0, 0.0), 0.0);
}

#[cfg(feature = "physics")]
#[test]
fn test_physics_extra_coverage() {
    let mut world = PhysicsWorld::<16>::new();
    world.set_gravity(Vector3::new(0.0, -9.81, 0.0));
    assert_eq!(world.gravity(), Vector3::new(0.0, -9.81, 0.0));

    let body1 = RigidBody::new(2.0)
        .with_position(Vector3::new(0.0, 5.0, 0.0))
        .with_velocity(Vector3::new(1.0, 0.0, 0.0))
        .with_restitution(0.8)
        .with_damping(0.05)
        .with_friction(0.4)
        .with_angular_velocity(Vector3::new(0.0, 1.0, 0.0))
        .with_angular_damping(0.05)
        .with_inertia_sphere(1.0)
        .with_collider(Collider::Sphere { radius: 1.0 });

    let id1 = world.add_body(body1).unwrap();
    assert_eq!(world.body_count(), 1);

    let body2 = RigidBody::new(3.0)
        .with_position(Vector3::new(0.0, 0.0, 0.0))
        .with_inertia_box(Vector3::new(1.0, 1.0, 1.0))
        .with_collider(Collider::Aabb {
            half_extents: Vector3::new(1.0, 1.0, 1.0),
        });

    let id2 = world.add_body(body2).unwrap();

    let b1 = world.body_mut(id1).unwrap();
    b1.apply_force(Vector3::new(10.0, 0.0, 0.0));
    b1.apply_impulse(Vector3::new(2.0, 0.0, 0.0));
    b1.apply_torque(Vector3::new(0.0, 5.0, 0.0));
    b1.apply_angular_impulse(Vector3::new(0.0, 1.0, 0.0));

    world.step::<8>(0.016);
    assert!(world.remove_body(id2));
}

#[cfg(feature = "textured")]
#[test]
fn test_renderer_textured_effects_and_ssaa() {
    let data = make_static_slice::<Rgb565>(32 * 32);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let mut fb = FrameBuf::new(backend, 32, 32);
    let mut zbuf = vec![ZDepth::MAX; 32 * 32];

    let mut tex_mgr = TextureManager::<4>::new();
    let tex_pixels = make_static_slice::<Rgb565>(4);
    let tex = Texture::new(tex_pixels, 2, 2);
    let id = tex_mgr.add_texture(tex).unwrap();

    let mut cmd = CommandBuffer::<8>::new();
    cmd.push(RenderCommand::ClearColor(Rgb565::BLACK)).unwrap();
    cmd.push(RenderCommand::ClearDepth(ZDepth::MAX)).unwrap();
    cmd.push(RenderCommand::Draw(
        DrawPrimitive::TexturedTriangleWithDepth {
            points: [Point2::new(2, 2), Point2::new(20, 2), Point2::new(10, 20)],
            depths: [5.0; 3],
            ws: [1.0; 3],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            texture_id: id,
        },
    ))
    .unwrap();

    {
        let mut frame = FrameCtx {
            zbuffer: &mut zbuf,
            width: 32,
            height: 32,
        };
        let res = execute_commands_with_dirty_region_effects_textured(
            &mut fb,
            &mut frame,
            &cmd,
            &tex_mgr,
            None,
            None,
            None,
            StippleMode::Off,
            PaletteMode::Off,
            None,
            [0.0, 0.0, -1.0],
        );
        assert!(res.is_ok());
    }

    // Test wrapper draw_zbuffered_with_textures
    draw_zbuffered_with_textures(
        DrawPrimitive::TexturedTriangleWithDepth {
            points: [Point2::new(2, 2), Point2::new(20, 2), Point2::new(10, 20)],
            depths: [5.0; 3],
            ws: [1.0; 3],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            texture_id: id,
        },
        &mut fb,
        &mut zbuf,
        32,
        &tex_mgr,
        None,
        None,
    );

    // Test out of bounds early returns for textured triangles
    let out_left = DrawPrimitive::TexturedTriangleWithDepth {
        points: [
            Point2::new(-10, 2),
            Point2::new(-20, 2),
            Point2::new(-15, 20),
        ],
        depths: [5.0; 3],
        ws: [1.0; 3],
        uvs: [[0.0, 0.0]; 3],
        texture_id: id,
    };
    draw_zbuffered_with_textures_mapped(
        out_left,
        &mut fb,
        &mut zbuf,
        32,
        &tex_mgr,
        None,
        None,
        TextureMapping::PerspectiveCorrect,
        StippleMode::Off,
        None,
        PaletteMode::Off,
    );

    let out_right = DrawPrimitive::TexturedTriangleWithDepth {
        points: [
            Point2::new(100, 2),
            Point2::new(200, 2),
            Point2::new(150, 20),
        ],
        depths: [5.0; 3],
        ws: [1.0; 3],
        uvs: [[0.0, 0.0]; 3],
        texture_id: id,
    };
    draw_zbuffered_with_textures_mapped(
        out_right,
        &mut fb,
        &mut zbuf,
        32,
        &tex_mgr,
        None,
        None,
        TextureMapping::PerspectiveCorrect,
        StippleMode::Off,
        None,
        PaletteMode::Off,
    );

    let out_top = DrawPrimitive::TexturedTriangleWithDepth {
        points: [
            Point2::new(2, -10),
            Point2::new(20, -20),
            Point2::new(10, -15),
        ],
        depths: [5.0; 3],
        ws: [1.0; 3],
        uvs: [[0.0, 0.0]; 3],
        texture_id: id,
    };
    draw_zbuffered_with_textures_mapped(
        out_top,
        &mut fb,
        &mut zbuf,
        32,
        &tex_mgr,
        None,
        None,
        TextureMapping::PerspectiveCorrect,
        StippleMode::Off,
        None,
        PaletteMode::Off,
    );

    let out_bot = DrawPrimitive::TexturedTriangleWithDepth {
        points: [
            Point2::new(2, 100),
            Point2::new(20, 200),
            Point2::new(10, 150),
        ],
        depths: [5.0; 3],
        ws: [1.0; 3],
        uvs: [[0.0, 0.0]; 3],
        texture_id: id,
    };
    draw_zbuffered_with_textures_mapped(
        out_bot,
        &mut fb,
        &mut zbuf,
        32,
        &tex_mgr,
        None,
        None,
        TextureMapping::PerspectiveCorrect,
        StippleMode::Off,
        None,
        PaletteMode::Off,
    );

    #[cfg(feature = "aa")]
    {
        let mut frame = FrameCtx {
            zbuffer: &mut zbuf,
            width: 32,
            height: 32,
        };
        let res_aa = execute_commands_2xssaa(&mut fb, &mut frame, &cmd);
        assert!(res_aa.is_ok());
    }
}

#[test]
fn test_draw_effects_depth_modes_and_interlace() {
    let data = make_static_slice::<Rgb565>(32 * 32);
    let backend = EndianCorrectedBuffer::new(
        data,
        embedded_graphics_framebuf::backends::EndianCorrection::ToLittleEndian,
    );
    let mut fb = FrameBuf::new(backend, 32, 32);
    let mut zbuf = vec![ZDepth::MAX; 32 * 32];

    let prim = DrawPrimitive::ColoredTriangleWithDepth {
        points: [Point2::new(2, 2), Point2::new(20, 2), Point2::new(10, 20)],
        depths: [2.0, 4.0, 8.0],
        color: Rgb565::RED,
    };

    draw_zbuffered_with_options(
        prim.clone(),
        &mut fb,
        &mut zbuf,
        32,
        None,
        None,
        DepthInterpolationMode::FastAverage,
    );
    draw_zbuffered_with_options(
        prim.clone(),
        &mut fb,
        &mut zbuf,
        32,
        None,
        None,
        DepthInterpolationMode::FastMax,
    );

    assert_eq!(
        InterlaceField::Progressive.toggle(),
        InterlaceField::Progressive
    );
    assert_eq!(InterlaceField::Even.toggle(), InterlaceField::Odd);
    assert_eq!(InterlaceField::Odd.toggle(), InterlaceField::Even);

    assert!(InterlaceField::Progressive.includes_scanline(0));
    assert!(InterlaceField::Even.includes_scanline(0));
    assert!(!InterlaceField::Even.includes_scanline(1));
    assert!(InterlaceField::Odd.includes_scanline(1));
    assert!(!InterlaceField::Odd.includes_scanline(0));

    let sd = ScreenDoorConfig { alpha: 128 };
    assert_eq!(sd.alpha, 128);
}

#[cfg(feature = "physics")]
#[test]
fn test_physics_collisions_and_capsules() {
    let mut world = PhysicsWorld::<16>::new();

    let cap1 = RigidBody::new(1.0)
        .with_position(Vector3::new(0.0, 0.0, 0.0))
        .with_collider(Collider::Capsule {
            height: 2.0,
            radius: 0.5,
        });
    let id_cap1 = world.add_body(cap1).unwrap();

    let cap2 = RigidBody::new(1.0)
        .with_position(Vector3::new(0.0, 0.5, 0.0))
        .with_collider(Collider::Capsule {
            height: 2.0,
            radius: 0.5,
        });
    let _id_cap2 = world.add_body(cap2).unwrap();

    let sph = RigidBody::new(1.0)
        .with_position(Vector3::new(0.1, 0.1, 0.1))
        .with_collider(Collider::Sphere { radius: 1.0 });
    let _id_sph = world.add_body(sph).unwrap();

    let sph2 = RigidBody::new(1.0)
        .with_position(Vector3::new(0.1, 0.1, 0.1))
        .with_collider(Collider::Sphere { radius: 1.0 });
    let _id_sph2 = world.add_body(sph2).unwrap();

    let box1 = RigidBody::new(1.0)
        .with_position(Vector3::new(0.0, 0.0, 0.0))
        .with_collider(Collider::Aabb {
            half_extents: Vector3::new(2.0, 2.0, 2.0),
        });
    let _id_box1 = world.add_body(box1).unwrap();

    let sph_inside = RigidBody::new(1.0)
        .with_position(Vector3::new(0.0, 0.0, 0.0))
        .with_collider(Collider::Sphere { radius: 0.5 });
    let _id_sph_in = world.add_body(sph_inside).unwrap();

    let b = world.body(id_cap1).unwrap();
    assert_eq!(b.speed(), 0.0);
    assert_eq!(b.kinetic_energy(), 0.0);

    world.step::<16>(0.016);
    assert!(world.body(id_cap1).is_some());
}

#[test]
fn test_engine_configuration_and_recording_extras() {
    let mut engine = K3dengine::new(240, 240);
    engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
    engine.camera.set_target(Point3::origin());

    engine.set_fog(FogConfig::new(Rgb565::BLACK, 1.0, 10.0));
    engine.clear_fog();
    engine.set_dither(DitherConfig { intensity: 128 });
    engine.clear_dither();
    engine.set_vertex_snap_bits(4);
    engine.set_texture_mapping(TextureMapping::Affine);
    engine.set_light_levels(LightLevels::Doom32);
    engine.set_stipple_mode(StippleMode::Checkerboard);
    engine.set_screen_tint(ScreenTint {
        color: Rgb565::RED,
        strength: 64,
    });
    engine.clear_screen_tint();
    engine.set_palette_mode(PaletteMode::Rgb332);
    engine.set_sky(SkyConfig::retro_blue());
    engine.clear_sky();
    engine.apply_retro_style(RetroStyle::psx());

    engine.set_caps(PROFILE_M0_MIN);
    engine.clear_caps();
    engine.set_quality_tier(QualityTier::Fastest);
    engine.set_material_profile(MaterialProfile::Unlit);

    let pt = engine.project_point(Point3::new(0.0, 0.0, 0.0));
    assert!(pt.is_some());

    let pts = engine.transform_points(
        &[0, 1],
        &[[0.0, 0.0, 0.0], [0.1, 0.0, 0.0]],
        Matrix4::identity(),
    );
    assert!(pts.is_some());

    let pts_w = engine.transform_points_with_w(
        &[0, 1],
        &[[0.0, 0.0, 0.0], [0.1, 0.0, 0.0]],
        Matrix4::identity(),
    );
    assert!(pts_w.is_some());

    let vertices = [[0.0, 0.0, 0.0], [0.1, 0.0, 0.0], [0.0, 0.1, 0.0]];
    let faces = [[0, 1, 2]];
    let geom = Geometry {
        vertices: &vertices,
        faces: &faces,
        ..Default::default()
    };
    let mesh = K3dMesh::new(geom);
    let mut cmds = CommandBuffer::<64>::new();
    let res = engine.record_drop_shadow(&mesh, 0.0, 2.0, 10.0, 128, Rgb565::BLACK, &mut cmds);
    assert!(res.is_ok());

    let mut cmds_fb = CommandBuffer::<64>::new();
    let fb_res = engine.record_with_fallback([&mesh], [&mesh], &mut cmds_fb, None);
    assert!(fb_res.is_ok());

    let steps = [];
    let mut cmds_deg = CommandBuffer::<64>::new();
    let policy = DegradationPolicy { steps: &steps };
    let deg_res = engine.record_with_degradation(&[&mesh], &mut cmds_deg, policy, None);
    assert!(deg_res.is_ok());
}
