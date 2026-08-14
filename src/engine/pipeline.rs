#[allow(unused_imports)]
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
#[allow(unused_imports)]
use nalgebra::{Matrix4, Point3, Vector3, Vector4};

#[allow(unused_imports)]
use micromath::F32Ext;

use super::K3dengine;
#[allow(unused_imports)]
use super::transform::{transform_point, transform_point_with_w};
use crate::mesh::{K3dMesh, RenderMode};
use crate::primitive::DrawPrimitive;

#[cfg(feature = "lod-crossfade")]
#[inline]
pub(crate) fn apply_draw_alpha(prim: DrawPrimitive, alpha: u8) -> DrawPrimitive {
    match prim {
        DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color,
        } => DrawPrimitive::TranslucentTriangleWithDepth {
            points,
            depths,
            color,
            alpha,
        },
        DrawPrimitive::TranslucentTriangleWithDepth {
            points,
            depths,
            color,
            alpha: prev,
        } => DrawPrimitive::TranslucentTriangleWithDepth {
            points,
            depths,
            color,
            alpha: ((prev as u16 * alpha as u16) / 255) as u8,
        },
        other => other,
    }
}

#[cfg(feature = "lighting")]
#[inline]
pub(crate) fn light_tint_at(engine: &K3dengine, world_pos: Point3<f32>) -> Rgb565 {
    let mut acc_r = 0u16;
    let mut acc_g = 0u16;
    let mut acc_b = 0u16;
    for light in &engine.point_lights {
        let tint = light.contribution_at(world_pos);
        acc_r += tint.r() as u16;
        acc_g += tint.g() as u16;
        acc_b += tint.b() as u16;
    }
    Rgb565::new(
        (acc_r.min(31)) as u8,
        (acc_g.min(63)) as u8,
        (acc_b.min(31)) as u8,
    )
}

#[cfg(feature = "lighting")]
#[inline]
pub(crate) fn add_tint(base: Rgb565, tint: Rgb565) -> Rgb565 {
    Rgb565::new(
        (base.r() as u16 + tint.r() as u16).min(31) as u8,
        (base.g() as u16 + tint.g() as u16).min(63) as u8,
        (base.b() as u16 + tint.b() as u16).min(31) as u8,
    )
}

#[cfg(feature = "lighting")]
const DOOM_LIGHT_TABLE: [u8; 32] = [
    8, 12, 16, 20, 24, 28, 34, 40, 48, 56, 64, 72, 82, 92, 102, 112, 124, 136, 148, 160, 172, 184,
    196, 206, 216, 224, 232, 238, 244, 248, 252, 255,
];

#[cfg(feature = "lighting")]
#[inline]
pub(crate) fn sector_shaded_color(
    engine: &K3dengine,
    base: Rgb565,
    brightness: u8,
    face_center: Point3<f32>,
) -> Rgb565 {
    let level_u8 = match engine.light_levels {
        crate::retro::LightLevels::Linear => brightness,
        crate::retro::LightLevels::Doom32 => {
            let base_level = (brightness as usize * 31) / 255;
            let distance = (face_center - engine.camera.position).norm();
            let distance_drop = (distance * 2.0) as usize;
            let idx = base_level.saturating_sub(distance_drop).min(31);
            DOOM_LIGHT_TABLE[idx]
        }
    };

    let factor = level_u8 as f32 / 255.0;
    Rgb565::new(
        (base.r() as f32 * factor) as u8,
        (base.g() as f32 * factor) as u8,
        (base.b() as f32 * factor) as u8,
    )
}

#[cfg(feature = "lighting")]
#[inline]
pub(crate) fn face_world_center(
    face: &[usize; 3],
    vertices: &[[f32; 3]],
    model_matrix: Matrix4<f32>,
) -> Point3<f32> {
    let v0 = vertices[face[0]];
    let v1 = vertices[face[1]];
    let v2 = vertices[face[2]];
    let cx = (v0[0] + v1[0] + v2[0]) / 3.0;
    let cy = (v0[1] + v1[1] + v2[1]) / 3.0;
    let cz = (v0[2] + v1[2] + v2[2]) / 3.0;
    model_matrix.transform_point(&Point3::new(cx, cy, cz))
}

pub(crate) fn render<'a, MS, F>(engine: &K3dengine, meshes: MS, mut callback: F)
where
    MS: IntoIterator<Item = &'a K3dMesh<'a>>,
    F: FnMut(DrawPrimitive),
{
    for mesh in meshes {
        if mesh.geometry.vertices.is_empty() {
            continue;
        }

        if engine.should_cull_mesh(mesh) {
            continue;
        }

        let mesh_pos = mesh.get_position();
        let distance = (mesh_pos - engine.camera.position).norm();
        let geometry = mesh.select_lod(distance);
        #[cfg(feature = "lod-crossfade")]
        let alpha_override = mesh.draw_alpha.get();
        #[cfg(feature = "lod-crossfade")]
        let mut emit = |prim: DrawPrimitive| {
            let prim = match alpha_override {
                Some(a) => apply_draw_alpha(prim, a),
                None => prim,
            };
            callback(prim);
        };
        #[cfg(not(feature = "lod-crossfade"))]
        let mut emit = |prim: DrawPrimitive| {
            callback(prim);
        };

        let transform_matrix = engine.camera.vp_matrix * mesh.model_matrix;

        #[cfg(feature = "textured")]
        let is_textured = matches!(
            engine.resolve_render_mode(&mesh.render_mode),
            RenderMode::Textured | RenderMode::TexturedGouraud(_) | RenderMode::MatCap
        );
        #[cfg(not(feature = "textured"))]
        let is_textured = false;

        let mut v_cache_plain: [Option<Point3<i32>>; 256] = [None; 256];
        #[cfg(feature = "textured")]
        let mut v_cache_w: [Option<(Point3<i32>, f32)>; 256] = [None; 256];

        let cache_limit = geometry.vertices.len().min(256);
        if is_textured {
            #[cfg(feature = "textured")]
            for i in 0..cache_limit {
                v_cache_w[i] = transform_point_with_w(
                    &engine.camera,
                    engine.width,
                    engine.height,
                    &geometry.vertices[i],
                    transform_matrix,
                );
            }
        } else {
            for i in 0..cache_limit {
                v_cache_plain[i] = transform_point(
                    &engine.camera,
                    engine.width,
                    engine.height,
                    &geometry.vertices[i],
                    transform_matrix,
                );
            }
        }

        let mut get_pt = |idx: usize| -> Option<Point3<i32>> {
            if idx < 256 {
                v_cache_plain[idx]
            } else {
                transform_point(
                    &engine.camera,
                    engine.width,
                    engine.height,
                    &geometry.vertices[idx],
                    transform_matrix,
                )
            }
        };

        let tf_face = |indices: &[usize; 3]| -> Option<[Point3<i32>; 3]> {
            Some([
                get_pt(indices[0])?,
                get_pt(indices[1])?,
                get_pt(indices[2])?,
            ])
        };

        #[cfg(feature = "textured")]
        let tf_face_w = |indices: &[usize; 3]| -> Option<([Point3<i32>; 3], [f32; 3])> {
            let get_w = |idx: usize| -> Option<(Point3<i32>, f32)> {
                if idx < 256 {
                    v_cache_w[idx]
                } else {
                    transform_point_with_w(
                        &engine.camera,
                        engine.width,
                        engine.height,
                        &geometry.vertices[idx],
                        transform_matrix,
                    )
                }
            };
            let (p0, w0) = get_w(indices[0])?;
            let (p1, w1) = get_w(indices[1])?;
            let (p2, w2) = get_w(indices[2])?;
            Some(([p0, p1, p2], [w0, w1, w2]))
        };

        if let Some(out_color) = mesh.outline_color {
            let has_vertex_normals = !geometry.vertex_normals.is_empty();
            let has_face_normals = !geometry.normals.is_empty();
            if has_vertex_normals || has_face_normals {
                for (face_idx, face) in geometry.faces.iter().enumerate() {
                    let face_normal = if has_face_normals {
                        Vector3::new(
                            geometry.normals[face_idx][0],
                            geometry.normals[face_idx][1],
                            geometry.normals[face_idx][2],
                        )
                    } else {
                        let v0 = Vector3::new(
                            geometry.vertices[face[0]][0],
                            geometry.vertices[face[0]][1],
                            geometry.vertices[face[0]][2],
                        );
                        let v1 = Vector3::new(
                            geometry.vertices[face[1]][0],
                            geometry.vertices[face[1]][1],
                            geometry.vertices[face[1]][2],
                        );
                        let v2 = Vector3::new(
                            geometry.vertices[face[2]][0],
                            geometry.vertices[face[2]][1],
                            geometry.vertices[face[2]][2],
                        );
                        (v1 - v0).cross(&(v2 - v0)).normalize()
                    };

                    let transformed_normal = mesh.model_matrix.transform_vector(&face_normal);
                    if !engine.is_backface(
                        face,
                        geometry.vertices,
                        mesh.model_matrix,
                        &transformed_normal,
                    ) {
                        continue;
                    }

                    let mut pts = [Point3::origin(); 3];
                    let mut valid = true;
                    for i in 0..3 {
                        let vn = if has_vertex_normals {
                            Vector3::new(
                                geometry.vertex_normals[face[i]][0],
                                geometry.vertex_normals[face[i]][1],
                                geometry.vertex_normals[face[i]][2],
                            )
                        } else {
                            face_normal
                        };
                        let vpos = geometry.vertices[face[i]];
                        let ext_v = [
                            vpos[0] + vn.x * mesh.outline_width,
                            vpos[1] + vn.y * mesh.outline_width,
                            vpos[2] + vn.z * mesh.outline_width,
                        ];
                        if let Some(pt) = transform_point(
                            &engine.camera,
                            engine.width,
                            engine.height,
                            &ext_v,
                            transform_matrix,
                        ) {
                            pts[i] = pt;
                        } else {
                            valid = false;
                            break;
                        }
                    }

                    if valid {
                        emit(DrawPrimitive::ColoredTriangleWithDepth {
                            points: [pts[0].xy(), pts[1].xy(), pts[2].xy()],
                            depths: [pts[0].z as f32, pts[1].z as f32, pts[2].z as f32],
                            color: out_color,
                        });
                    }
                }
            }
        }

        let render_mode = engine.resolve_render_mode(&mesh.render_mode);
        match render_mode {
            RenderMode::Points => {
                let screen_space_points = (0..geometry.vertices.len()).filter_map(&mut get_pt);

                if geometry.colors.len() == geometry.vertices.len() {
                    for (point, color) in screen_space_points.zip(geometry.colors) {
                        emit(DrawPrimitive::ColoredPoint(point.xy(), *color));
                    }
                } else {
                    for point in screen_space_points {
                        emit(DrawPrimitive::ColoredPoint(point.xy(), mesh.color));
                    }
                }
            }

            RenderMode::Lines if !geometry.lines.is_empty() => {
                for line in geometry.lines {
                    if let (Some(p1), Some(p2)) = (get_pt(line[0]), get_pt(line[1])) {
                        emit(DrawPrimitive::Line([p1.xy(), p2.xy()], mesh.color));
                    }
                }
            }

            RenderMode::Lines if !geometry.faces.is_empty() => {
                for face in geometry.faces {
                    if let Some([p1, p2, p3]) = tf_face(face) {
                        emit(DrawPrimitive::Line([p1.xy(), p2.xy()], mesh.color));
                        emit(DrawPrimitive::Line([p2.xy(), p3.xy()], mesh.color));
                        emit(DrawPrimitive::Line([p3.xy(), p1.xy()], mesh.color));
                    }
                }
            }

            RenderMode::Lines => {}

            RenderMode::Solid => {
                if geometry.normals.is_empty() {
                    for face in geometry.faces.iter() {
                        #[cfg(feature = "lighting")]
                        let color = if !engine.point_lights.is_empty() {
                            let wc = face_world_center(face, geometry.vertices, mesh.model_matrix);
                            add_tint(mesh.color, light_tint_at(engine, wc))
                        } else {
                            mesh.color
                        };
                        #[cfg(not(feature = "lighting"))]
                        let color = mesh.color;
                        let v = &geometry.vertices;
                        let clip = [
                            transform_matrix
                                * Vector4::new(v[face[0]][0], v[face[0]][1], v[face[0]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[1]][0], v[face[1]][1], v[face[1]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[2]][0], v[face[2]][1], v[face[2]][2], 1.0),
                        ];
                        engine.emit_clipped(clip, color, &mut emit);
                    }
                } else {
                    for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                        let normal = Vector3::new(normal[0], normal[1], normal[2]);
                        let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                        if engine.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }
                        #[cfg(feature = "lighting")]
                        let color = if !engine.point_lights.is_empty() {
                            let wc = face_world_center(face, geometry.vertices, mesh.model_matrix);
                            add_tint(mesh.color, light_tint_at(engine, wc))
                        } else {
                            mesh.color
                        };
                        #[cfg(not(feature = "lighting"))]
                        let color = mesh.color;
                        let v = &geometry.vertices;
                        let clip = [
                            transform_matrix
                                * Vector4::new(v[face[0]][0], v[face[0]][1], v[face[0]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[1]][0], v[face[1]][1], v[face[1]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[2]][0], v[face[2]][1], v[face[2]][2], 1.0),
                        ];
                        engine.emit_clipped(clip, color, &mut emit);
                    }
                }
            }

            #[cfg(feature = "lighting")]
            RenderMode::SolidLightDir(direction) => {
                let color_as_float = Vector3::new(
                    mesh.color.r() as f32 / 32.0,
                    mesh.color.g() as f32 / 64.0,
                    mesh.color.b() as f32 / 32.0,
                );
                let ambient_color = color_as_float * 0.1;
                let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);

                for (face, normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                    let normal = Vector3::new(normal[0], normal[1], normal[2]);
                    let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                    if engine.is_backface(
                        face,
                        geometry.vertices,
                        mesh.model_matrix,
                        &transformed_normal,
                    ) {
                        continue;
                    }

                    if let Some([p1, p2, p3]) = tf_face(face) {
                        let intensity = transformed_normal.dot(&adjusted_dir).max(0.0);
                        let final_color = color_as_float * intensity + ambient_color;
                        let final_color = Vector3::new(
                            final_color.x.clamp(0.0, 1.0),
                            final_color.y.clamp(0.0, 1.0),
                            final_color.z.clamp(0.0, 1.0),
                        );
                        let mut color = Rgb565::new(
                            (final_color.x * 31.0) as u8,
                            (final_color.y * 63.0) as u8,
                            (final_color.z * 31.0) as u8,
                        );
                        if !engine.point_lights.is_empty() {
                            let wc = face_world_center(face, geometry.vertices, mesh.model_matrix);
                            color = add_tint(color, light_tint_at(engine, wc));
                        }
                        emit(DrawPrimitive::ColoredTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p1.z as f32, p1.z as f32],
                            color,
                        });
                    }
                }
            }

            #[cfg(feature = "lighting")]
            RenderMode::GouraudLightDir(direction) => {
                let color_as_float = Vector3::new(
                    mesh.color.r() as f32 / 32.0,
                    mesh.color.g() as f32 / 64.0,
                    mesh.color.b() as f32 / 32.0,
                );
                let ambient_color = color_as_float * 0.1;
                let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);

                for (face, face_normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                    let fn_vec = Vector3::new(face_normal[0], face_normal[1], face_normal[2]);
                    let transformed_fn = mesh.model_matrix.transform_vector(&fn_vec);

                    if engine.is_backface(
                        face,
                        geometry.vertices,
                        mesh.model_matrix,
                        &transformed_fn,
                    ) {
                        continue;
                    }

                    if let Some([p1, p2, p3]) = tf_face(face) {
                        let vertex_colors: [Rgb565; 3] = core::array::from_fn(|k| {
                            let vn = if !geometry.vertex_normals.is_empty() {
                                let vn_arr = geometry.vertex_normals[face[k]];
                                let vn_vec = Vector3::new(vn_arr[0], vn_arr[1], vn_arr[2]);
                                mesh.model_matrix.transform_vector(&vn_vec)
                            } else {
                                transformed_fn
                            };

                            let intensity = vn.dot(&adjusted_dir).max(0.0);
                            let c = color_as_float * intensity + ambient_color;
                            let mut vc = Rgb565::new(
                                (c.x.clamp(0.0, 1.0) * 31.0) as u8,
                                (c.y.clamp(0.0, 1.0) * 63.0) as u8,
                                (c.z.clamp(0.0, 1.0) * 31.0) as u8,
                            );
                            if !engine.point_lights.is_empty() {
                                let vpos = geometry.vertices[face[k]];
                                let wp = mesh
                                    .model_matrix
                                    .transform_point(&Point3::new(vpos[0], vpos[1], vpos[2]));
                                vc = add_tint(vc, light_tint_at(engine, wp));
                            }
                            vc
                        });

                        emit(DrawPrimitive::GouraudTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                            colors: vertex_colors,
                        });
                    }
                }
            }

            #[cfg(feature = "lighting")]
            RenderMode::SectorBright(brightness) => {
                if geometry.normals.is_empty() {
                    for face in geometry.faces.iter() {
                        let wc = face_world_center(face, geometry.vertices, mesh.model_matrix);
                        let mut color = sector_shaded_color(engine, mesh.color, brightness, wc);
                        if !engine.point_lights.is_empty() {
                            color = add_tint(color, light_tint_at(engine, wc));
                        }
                        let v = &geometry.vertices;
                        let clip = [
                            transform_matrix
                                * Vector4::new(v[face[0]][0], v[face[0]][1], v[face[0]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[1]][0], v[face[1]][1], v[face[1]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[2]][0], v[face[2]][1], v[face[2]][2], 1.0),
                        ];
                        engine.emit_clipped(clip, color, &mut emit);
                    }
                } else {
                    for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                        let normal = Vector3::new(normal[0], normal[1], normal[2]);
                        let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                        if engine.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }
                        let wc = face_world_center(face, geometry.vertices, mesh.model_matrix);
                        let mut color = sector_shaded_color(engine, mesh.color, brightness, wc);
                        if !engine.point_lights.is_empty() {
                            color = add_tint(color, light_tint_at(engine, wc));
                        }
                        let v = &geometry.vertices;
                        let clip = [
                            transform_matrix
                                * Vector4::new(v[face[0]][0], v[face[0]][1], v[face[0]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[1]][0], v[face[1]][1], v[face[1]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[2]][0], v[face[2]][1], v[face[2]][2], 1.0),
                        ];
                        engine.emit_clipped(clip, color, &mut emit);
                    }
                }
            }

            #[cfg(feature = "lighting")]
            RenderMode::Toon(direction, bands) => {
                let color_as_float = Vector3::new(
                    mesh.color.r() as f32 / 32.0,
                    mesh.color.g() as f32 / 64.0,
                    mesh.color.b() as f32 / 32.0,
                );
                let ambient_color = color_as_float * 0.15;
                let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);
                let bands_f = bands.max(1) as f32;

                for (face, normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                    let normal = Vector3::new(normal[0], normal[1], normal[2]);
                    let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                    if engine.is_backface(
                        face,
                        geometry.vertices,
                        mesh.model_matrix,
                        &transformed_normal,
                    ) {
                        continue;
                    }

                    if let Some([p1, p2, p3]) = tf_face(face) {
                        let raw_intensity = transformed_normal.dot(&adjusted_dir).max(0.0);
                        let intensity =
                            ((raw_intensity * bands_f).round() / bands_f).clamp(0.0, 1.0);

                        let final_color = color_as_float * intensity + ambient_color;
                        let final_color = Vector3::new(
                            final_color.x.clamp(0.0, 1.0),
                            final_color.y.clamp(0.0, 1.0),
                            final_color.z.clamp(0.0, 1.0),
                        );
                        let mut color = Rgb565::new(
                            (final_color.x * 31.0) as u8,
                            (final_color.y * 63.0) as u8,
                            (final_color.z * 31.0) as u8,
                        );
                        if !engine.point_lights.is_empty() {
                            let wc = face_world_center(face, geometry.vertices, mesh.model_matrix);
                            color = add_tint(color, light_tint_at(engine, wc));
                        }
                        emit(DrawPrimitive::ColoredTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                            color,
                        });
                    }
                }
            }

            #[cfg(feature = "lighting")]
            RenderMode::BlinnPhong {
                light_dir,
                specular_intensity,
                shininess,
            } => {
                let color_as_float = Vector3::new(
                    mesh.color.r() as f32 / 32.0,
                    mesh.color.g() as f32 / 64.0,
                    mesh.color.b() as f32 / 32.0,
                );
                let ambient_color = color_as_float * 0.1;
                let adjusted_light_dir = Vector3::new(light_dir.x, light_dir.y, -light_dir.z);
                let light_dir_normalized = adjusted_light_dir.normalize();

                for (face, normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                    let normal = Vector3::new(normal[0], normal[1], normal[2]);
                    let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                    let normalized_normal = transformed_normal.normalize();

                    if engine.is_backface(
                        face,
                        geometry.vertices,
                        mesh.model_matrix,
                        &normalized_normal,
                    ) {
                        continue;
                    }

                    if let Some([p1, p2, p3]) = tf_face(face) {
                        let v0 = geometry.vertices[face[0]];
                        let v1 = geometry.vertices[face[1]];
                        let v2 = geometry.vertices[face[2]];
                        let face_center = Point3::new(
                            (v0[0] + v1[0] + v2[0]) / 3.0,
                            (v0[1] + v1[1] + v2[1]) / 3.0,
                            (v0[2] + v1[2] + v2[2]) / 3.0,
                        );
                        let face_center_world = mesh.model_matrix.transform_point(&face_center);

                        let view_dir = (engine.camera.position - face_center_world).normalize();
                        let half_vector = (light_dir_normalized + view_dir).normalize();
                        let diffuse_intensity =
                            normalized_normal.dot(&light_dir_normalized).max(0.0);
                        let specular_term =
                            normalized_normal.dot(&half_vector).max(0.0).powf(shininess);

                        let diffuse_color = color_as_float * diffuse_intensity;
                        let specular_color =
                            Vector3::new(1.0, 1.0, 1.0) * specular_term * specular_intensity;
                        let final_color = ambient_color + diffuse_color + specular_color;

                        let final_color = Vector3::new(
                            final_color.x.clamp(0.0, 1.0),
                            final_color.y.clamp(0.0, 1.0),
                            final_color.z.clamp(0.0, 1.0),
                        );

                        let mut color = Rgb565::new(
                            (final_color.x * 31.0) as u8,
                            (final_color.y * 63.0) as u8,
                            (final_color.z * 31.0) as u8,
                        );
                        if !engine.point_lights.is_empty() {
                            color = add_tint(color, light_tint_at(engine, face_center_world));
                        }
                        emit(DrawPrimitive::ColoredTriangleWithDepth {
                            points: [p1.xy(), p2.xy(), p3.xy()],
                            depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                            color,
                        });
                    }
                }
            }

            #[cfg(feature = "textured")]
            RenderMode::Textured => {
                let Some(texture_id) = geometry.texture_id else {
                    return;
                };
                if geometry.uvs.is_empty() {
                    return;
                }

                if geometry.normals.is_empty() {
                    for face in geometry.faces.iter() {
                        if let Some((points, ws)) = tf_face_w(face) {
                            emit(DrawPrimitive::TexturedTriangleWithDepth {
                                points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                depths: [
                                    points[0].z as f32,
                                    points[1].z as f32,
                                    points[2].z as f32,
                                ],
                                ws,
                                uvs: [
                                    geometry.uvs[face[0]],
                                    geometry.uvs[face[1]],
                                    geometry.uvs[face[2]],
                                ],
                                texture_id,
                            });
                        }
                    }
                } else {
                    for (face, face_normal) in geometry.faces.iter().zip(geometry.normals) {
                        let normal = Vector3::new(face_normal[0], face_normal[1], face_normal[2]);
                        let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                        if engine.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }

                        if let Some((points, ws)) = tf_face_w(face) {
                            emit(DrawPrimitive::TexturedTriangleWithDepth {
                                points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                depths: [
                                    points[0].z as f32,
                                    points[1].z as f32,
                                    points[2].z as f32,
                                ],
                                ws,
                                uvs: [
                                    geometry.uvs[face[0]],
                                    geometry.uvs[face[1]],
                                    geometry.uvs[face[2]],
                                ],
                                texture_id,
                            });
                        }
                    }
                }
            }

            #[cfg(feature = "textured")]
            RenderMode::TexturedGouraud(direction) => {
                let Some(texture_id) = geometry.texture_id else {
                    return;
                };
                if geometry.uvs.is_empty() {
                    return;
                }

                let color_as_float = Vector3::new(
                    mesh.color.r() as f32 / 32.0,
                    mesh.color.g() as f32 / 64.0,
                    mesh.color.b() as f32 / 32.0,
                );
                let ambient_color = color_as_float * 0.1;
                let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);

                if geometry.normals.is_empty() {
                    for face in geometry.faces.iter() {
                        if let Some((points, ws)) = tf_face_w(face) {
                            emit(DrawPrimitive::TexturedTriangleWithDepth {
                                points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                depths: [
                                    points[0].z as f32,
                                    points[1].z as f32,
                                    points[2].z as f32,
                                ],
                                ws,
                                uvs: [
                                    geometry.uvs[face[0]],
                                    geometry.uvs[face[1]],
                                    geometry.uvs[face[2]],
                                ],
                                texture_id,
                            });
                        }
                    }
                } else {
                    for (face, face_normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                        let fn_vec = Vector3::new(face_normal[0], face_normal[1], face_normal[2]);
                        let transformed_fn = mesh.model_matrix.transform_vector(&fn_vec);

                        if engine.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_fn,
                        ) {
                            continue;
                        }

                        if let Some((points, ws)) = tf_face_w(face) {
                            let vertex_colors: [Rgb565; 3] = core::array::from_fn(|k| {
                                let vn = if !geometry.vertex_normals.is_empty() {
                                    let vn_arr = geometry.vertex_normals[face[k]];
                                    let vn_vec = Vector3::new(vn_arr[0], vn_arr[1], vn_arr[2]);
                                    mesh.model_matrix.transform_vector(&vn_vec)
                                } else {
                                    transformed_fn
                                };

                                let intensity = vn.dot(&adjusted_dir).max(0.0);
                                let c = color_as_float * intensity + ambient_color;
                                let mut vc = Rgb565::new(
                                    (c.x.clamp(0.0, 1.0) * 31.0) as u8,
                                    (c.y.clamp(0.0, 1.0) * 63.0) as u8,
                                    (c.z.clamp(0.0, 1.0) * 31.0) as u8,
                                );
                                if !engine.point_lights.is_empty() {
                                    let vpos = geometry.vertices[face[k]];
                                    let wp = mesh
                                        .model_matrix
                                        .transform_point(&Point3::new(vpos[0], vpos[1], vpos[2]));
                                    vc = add_tint(vc, light_tint_at(engine, wp));
                                }
                                vc
                            });

                            emit(DrawPrimitive::TexturedGouraudTriangleWithDepth {
                                points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                depths: [
                                    points[0].z as f32,
                                    points[1].z as f32,
                                    points[2].z as f32,
                                ],
                                ws,
                                uvs: [
                                    geometry.uvs[face[0]],
                                    geometry.uvs[face[1]],
                                    geometry.uvs[face[2]],
                                ],
                                colors: vertex_colors,
                                texture_id,
                            });
                        }
                    }
                }
            }

            #[cfg(feature = "textured")]
            RenderMode::MatCap => {
                let Some(texture_id) = geometry.texture_id else {
                    return;
                };
                let has_vertex_normals = !geometry.vertex_normals.is_empty();
                let has_face_normals = !geometry.normals.is_empty();
                if !has_vertex_normals && !has_face_normals {
                    return;
                }

                let mv = engine.camera.view_matrix * mesh.model_matrix;
                let normal_matrix = nalgebra::Matrix3::new(
                    mv[(0, 0)],
                    mv[(0, 1)],
                    mv[(0, 2)],
                    mv[(1, 0)],
                    mv[(1, 1)],
                    mv[(1, 2)],
                    mv[(2, 0)],
                    mv[(2, 1)],
                    mv[(2, 2)],
                );

                for (face_idx, face) in geometry.faces.iter().enumerate() {
                    let face_normal = if has_face_normals {
                        Vector3::new(
                            geometry.normals[face_idx][0],
                            geometry.normals[face_idx][1],
                            geometry.normals[face_idx][2],
                        )
                    } else {
                        let v0 = Vector3::new(
                            geometry.vertices[face[0]][0],
                            geometry.vertices[face[0]][1],
                            geometry.vertices[face[0]][2],
                        );
                        let v1 = Vector3::new(
                            geometry.vertices[face[1]][0],
                            geometry.vertices[face[1]][1],
                            geometry.vertices[face[1]][2],
                        );
                        let v2 = Vector3::new(
                            geometry.vertices[face[2]][0],
                            geometry.vertices[face[2]][1],
                            geometry.vertices[face[2]][2],
                        );
                        (v1 - v0).cross(&(v2 - v0)).normalize()
                    };

                    let transformed_normal = mesh.model_matrix.transform_vector(&face_normal);
                    if engine.is_backface(
                        face,
                        geometry.vertices,
                        mesh.model_matrix,
                        &transformed_normal,
                    ) {
                        continue;
                    }

                    if let Some((points, ws)) = tf_face_w(face) {
                        let mut uvs = [[0.0f32; 2]; 3];
                        for i in 0..3 {
                            let vertex_normal = if has_vertex_normals {
                                Vector3::new(
                                    geometry.vertex_normals[face[i]][0],
                                    geometry.vertex_normals[face[i]][1],
                                    geometry.vertex_normals[face[i]][2],
                                )
                            } else {
                                face_normal
                            };
                            let view_normal = (normal_matrix * vertex_normal).normalize();
                            let u = view_normal.x * 0.5 + 0.5;
                            let v = -view_normal.y * 0.5 + 0.5;
                            uvs[i] = [u, v];
                        }

                        emit(DrawPrimitive::TexturedTriangleWithDepth {
                            points: [points[0].xy(), points[1].xy(), points[2].xy()],
                            depths: [points[0].z as f32, points[1].z as f32, points[2].z as f32],
                            ws,
                            uvs,
                            texture_id,
                        });
                    }
                }
            }
        }
    }
}
