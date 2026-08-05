//! Tests for opt-in scene extras (`scene-extras` / individual features).
//!
//! Run with:
//! ```bash
//! cargo test --test scene_extras --features "std,scene-extras"
//! ```

#![cfg(feature = "aabb-cull")]

extern crate std;

use embedded_3dgfx::mesh::{Geometry, K3dMesh, LODLevels};
use embedded_3dgfx::{Aabb, K3dengine, mesh_ray_cast, mesh_ray_cast_bounded, mesh_ray_cast_mesh};
use nalgebra::{Matrix4, Point3, Vector3};

fn unit_tri_mesh() -> (
    K3dMesh<'static>,
    &'static [[f32; 3]; 3],
    &'static [[usize; 3]; 1],
) {
    // Leak small statics for 'static Geometry in tests — fine for unit tests.
    let verts: &'static [[f32; 3]; 3] = Box::leak(Box::new([
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    ]));
    let faces: &'static [[usize; 3]; 1] = Box::leak(Box::new([[0, 1, 2]]));
    let mesh = K3dMesh::new(Geometry {
        vertices: verts,
        faces: faces,
        colors: &[],
        lines: &[],
        normals: &[],
        vertex_normals: &[],
        uvs: &[],
        texture_id: None,
    });
    (mesh, verts, faces)
}

#[test]
fn aabb_cached_and_encloses_geometry() {
    let (mesh, _, _) = unit_tri_mesh();
    let aabb = mesh.aabb.expect("cached on new");
    assert!(aabb.half_extents.x >= 0.5);
    assert!(aabb.half_extents.y >= 0.5);
}

#[test]
fn aabb_ray_broadphase_rejects_miss() {
    let aabb = Aabb::from_min_max(Vector3::new(-0.5, -0.5, -0.5), Vector3::new(0.5, 0.5, 0.5));
    assert!(
        aabb.intersect_ray(
            Vector3::new(10.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            100.0
        )
        .is_none()
    );
}

#[test]
fn mesh_ray_cast_bounded_hits_triangle() {
    let (mesh, _, _) = unit_tri_mesh();
    let hit = mesh_ray_cast_bounded(
        Vector3::new(0.2, 0.2, 5.0),
        Vector3::new(0.0, 0.0, -1.0),
        &mesh.geometry,
        &Matrix4::identity(),
        100.0,
        Some(&mesh.model_aabb()),
    );
    assert!(hit.is_some());
    let hit = hit.unwrap();
    assert!(hit.distance > 0.0);
    assert_eq!(hit.face_index, 0);
}

#[test]
fn mesh_ray_cast_mesh_matches_unbounded_when_aabb_loose() {
    let (mut mesh, _, _) = unit_tri_mesh();
    mesh.set_position(0.0, 0.0, 0.0);
    let origin = Vector3::new(0.25, 0.25, 3.0);
    let dir = Vector3::new(0.0, 0.0, -1.0);
    let a = mesh_ray_cast(origin, dir, &mesh.geometry, &mesh.model_matrix, 50.0);
    let b = mesh_ray_cast_mesh(origin, dir, &mesh, 50.0);
    assert_eq!(a.is_some(), b.is_some());
    if let (Some(a), Some(b)) = (a, b) {
        assert!((a.distance - b.distance).abs() < 1e-3);
    }
}

#[test]
fn two_stage_cull_rejects_far_mesh() {
    let (mut mesh, _, _) = unit_tri_mesh();
    mesh.set_position(0.0, 0.0, -1000.0);
    let mut engine = K3dengine::new(320, 240);
    engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));
    // Behind the camera / far outside frustum — record should emit no draws.
    let mut cmds = embedded_3dgfx::command_buffer::CommandBuffer::<64>::new();
    engine
        .record(core::iter::once(&mesh), &mut cmds, None)
        .unwrap();
    let draws = cmds
        .iter()
        .filter(|c| matches!(c, embedded_3dgfx::command_buffer::RenderCommand::Draw(_)))
        .count();
    assert_eq!(draws, 0);
}

#[cfg(feature = "render-layers")]
mod layers {
    use super::*;
    use embedded_3dgfx::RenderLayers;

    #[test]
    fn layer_mismatch_culls_mesh() {
        let (mut mesh, _, _) = unit_tri_mesh();
        mesh.set_position(0.0, 0.0, 0.0);
        mesh.set_layers(RenderLayers::layer(3));
        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));
        // Default camera is layer 0 only.
        let mut cmds = embedded_3dgfx::command_buffer::CommandBuffer::<64>::new();
        engine
            .record(core::iter::once(&mesh), &mut cmds, None)
            .unwrap();
        let draws = cmds
            .iter()
            .filter(|c| matches!(c, embedded_3dgfx::command_buffer::RenderCommand::Draw(_)))
            .count();
        assert_eq!(draws, 0);

        engine.camera.set_layers(RenderLayers::layer(3));
        let mut cmds2 = embedded_3dgfx::command_buffer::CommandBuffer::<256>::new();
        // Solid mode so triangles actually emit.
        mesh.set_render_mode(embedded_3dgfx::mesh::RenderMode::Solid);
        engine
            .record(core::iter::once(&mesh), &mut cmds2, None)
            .unwrap();
        // May still be 0 if triangle fails projection; at least layers no longer cull.
        // Force a point mode which is more reliable at origin.
        mesh.set_render_mode(embedded_3dgfx::mesh::RenderMode::Points);
        let mut cmds3 = embedded_3dgfx::command_buffer::CommandBuffer::<256>::new();
        engine
            .record(core::iter::once(&mesh), &mut cmds3, None)
            .unwrap();
        let draws = cmds3
            .iter()
            .filter(|c| matches!(c, embedded_3dgfx::command_buffer::RenderCommand::Draw(_)))
            .count();
        assert!(draws > 0, "matching layers should allow recording");
    }
}

#[cfg(feature = "lod-crossfade")]
mod lod {
    use super::*;
    use embedded_3dgfx::mesh::LodPick;

    #[test]
    fn fade_margin_produces_crossfade() {
        let verts: &'static [[f32; 3]; 3] = Box::leak(Box::new([
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ]));
        let faces: &'static [[usize; 3]; 1] = Box::leak(Box::new([[0usize, 1, 2]]));
        let mut mesh = K3dMesh::new(Geometry {
            vertices: verts,
            faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        });
        let med_verts: &'static [[f32; 3]; 3] = Box::leak(Box::new([
            [0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.0, 0.5, 0.0],
        ]));
        mesh.set_lod(
            Some(Geometry {
                vertices: med_verts,
                faces,
                colors: &[],
                lines: &[],
                normals: &[],
                vertex_normals: &[],
                uvs: &[],
                texture_id: None,
            }),
            None,
            LODLevels {
                high_distance: 10.0,
                medium_distance: 20.0,
                fade_margin: 2.0,
            },
        );
        assert!(matches!(mesh.select_lod_pick(5.0), LodPick::Single(_)));
        assert!(matches!(
            mesh.select_lod_pick(10.0),
            LodPick::Crossfade { .. }
        ));
    }
}

#[cfg(feature = "anim-blend")]
mod anim {
    use embedded_3dgfx::skeleton::{
        AnimClip, Bone, BonePose, Skeleton, SkeletonKeyframe, SkinningData, VertexSkinning,
        blend_clips_onto_skeleton, compute_joint_aabbs, skinned_model_aabb,
    };
    use nalgebra::{UnitQuaternion, Vector3};

    #[test]
    fn bone_pose_blend_midpoint() {
        let a = BonePose {
            position: Vector3::zeros(),
            rotation: UnitQuaternion::identity(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let b = BonePose {
            position: Vector3::new(2.0, 0.0, 0.0),
            rotation: UnitQuaternion::identity(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let m = BonePose::blend(a, b, 0.5);
        assert!((m.position.x - 1.0).abs() < 1e-5);
    }

    #[test]
    fn blend_clips_updates_bone() {
        let poses_a: &'static [BonePose] = Box::leak(Box::new([BonePose {
            position: Vector3::zeros(),
            rotation: UnitQuaternion::identity(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        }]));
        let poses_b: &'static [BonePose] = Box::leak(Box::new([BonePose {
            position: Vector3::new(0.0, 2.0, 0.0),
            rotation: UnitQuaternion::identity(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        }]));
        let kfs: &'static [SkeletonKeyframe] = Box::leak(Box::new([
            SkeletonKeyframe {
                time: 0.0,
                poses: poses_a,
            },
            SkeletonKeyframe {
                time: 1.0,
                poses: poses_b,
            },
        ]));
        let clip = AnimClip {
            keyframes: kfs,
            looping: false,
        };
        let mut skel = Skeleton::<4>::new();
        let _ = skel.add_bone(Bone::new("root"), None).unwrap();
        blend_clips_onto_skeleton::<4, 1>(&mut skel, &[(&clip, 0.5, 1.0)]);
        let bone = skel.get_bone(embedded_3dgfx::skeleton::BoneId(0)).unwrap();
        assert!((bone.position.y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn skinned_aabb_from_joint_bounds() {
        let mut skel = Skeleton::<4>::new();
        let _ = skel.add_bone(Bone::new("root"), None).unwrap();
        skel.compute_inverse_bind_poses();
        let verts = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mut skin = SkinningData::new();
        let _ = skin.add_vertex(VertexSkinning::single_bone(0));
        let _ = skin.add_vertex(VertexSkinning::single_bone(0));
        let _ = skin.add_vertex(VertexSkinning::single_bone(0));
        let joints = compute_joint_aabbs::<4>(skel.bones.len(), &skin, &verts);
        let aabb = skinned_model_aabb(&skel, &joints).expect("aabb");
        assert!(aabb.half_extents.x > 0.0 || aabb.half_extents.y > 0.0);
    }
}

#[cfg(feature = "gizmos")]
mod gizmos_tests {
    use embedded_3dgfx::DrawPrimitive;
    use embedded_3dgfx::bounds::Aabb;
    use embedded_3dgfx::gizmos::emit_aabb_wireframe_projected;
    use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
    use nalgebra::{Matrix4, Point3, Vector3};

    #[test]
    fn aabb_gizmo_emits_twelve_lines() {
        let aabb = Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        let mut lines = 0usize;
        emit_aabb_wireframe_projected(
            &aabb,
            &Matrix4::identity(),
            |p| Some(Point3::new((p[0] * 10.0) as i32, (p[1] * 10.0) as i32, 1)),
            Rgb565::CSS_LIME,
            |prim| {
                if matches!(prim, DrawPrimitive::Line(_, _)) {
                    lines += 1;
                }
            },
        );
        assert_eq!(lines, 12);
    }
}

#[cfg(feature = "record-sort")]
mod sort {
    use super::*;
    use embedded_3dgfx::mesh::RenderMode;
    use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};

    #[test]
    fn higher_priority_recorded_before_lower() {
        // Both meshes as points at origin-ish positions in view.
        let (mut hi, _, _) = unit_tri_mesh();
        let (mut lo, _, _) = unit_tri_mesh();
        hi.set_priority(200);
        lo.set_priority(10);
        hi.set_render_mode(RenderMode::Points);
        lo.set_render_mode(RenderMode::Points);
        hi.set_color(Rgb565::CSS_RED);
        lo.set_color(Rgb565::CSS_BLUE);
        hi.set_position(0.0, 0.0, 0.0);
        lo.set_position(0.1, 0.0, 0.0);

        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let mut cmds = embedded_3dgfx::command_buffer::CommandBuffer::<256>::new();
        // Input order: low then high — sort should put high first.
        engine
            .record([&lo, &hi].into_iter(), &mut cmds, None)
            .unwrap();
        let colors: std::vec::Vec<_> = cmds
            .iter()
            .filter_map(|c| match c {
                embedded_3dgfx::command_buffer::RenderCommand::Draw(
                    embedded_3dgfx::DrawPrimitive::ColoredPoint(_, col),
                ) => Some(*col),
                _ => None,
            })
            .collect();
        assert!(!colors.is_empty());
        assert_eq!(colors[0], Rgb565::CSS_RED);
    }
}
