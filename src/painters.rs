//! Painter's Algorithm Implementation
//!
//! Renders triangles sorted by depth (back-to-front) without a Z-buffer.
//! This trades sorting overhead for significant memory savings (no Z-buffer needed).
//!
//! Benefits:
//! - Saves ~1.92MB RAM for 800x600 resolution (u32 Z-buffer)
//! - Good for simple scenes with few overlapping triangles
//! - O(n log n) sorting cost, acceptable when n is small
//!
//! Limitations:
//! - Doesn't handle cyclic overlaps perfectly
//! - Sorting cost increases with triangle count
//! - Best for scenes with ~1000-5000 triangles
//!
//! Note: callers supply a fixed triangle scratch buffer; no heap allocation is required.

use crate::engine::K3dengine;
use crate::mesh::{K3dMesh, RenderMode};
use crate::primitive::DrawPrimitive;
use core::cmp::Ordering;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use nalgebra::{Vector3, Vector4};

/// A triangle with its average depth for sorting
#[derive(Debug, Clone)]
pub struct DepthSortedTriangle {
    pub primitive: DrawPrimitive,
    pub avg_depth: f32,
}

impl DepthSortedTriangle {
    pub const DUMMY: Self = Self {
        primitive: DrawPrimitive::ColoredPoint(nalgebra::Point2::new(0, 0), Rgb565::BLACK),
        avg_depth: 0.0,
    };

    /// Create a new depth-sorted triangle
    pub fn new(primitive: DrawPrimitive, avg_depth: f32) -> Self {
        Self {
            primitive,
            avg_depth,
        }
    }
}

impl Default for DepthSortedTriangle {
    fn default() -> Self {
        Self::DUMMY
    }
}

impl K3dengine {
    /// Render using Painter's Algorithm (back-to-front sorting, no Z-buffer)
    ///
    /// Collects all triangles, sorts them by depth, and renders back-to-front.
    /// This eliminates the need for a Z-buffer, saving significant memory.
    ///
    /// # Arguments
    /// * `meshes` - Iterator of meshes to render
    /// * `triangles` - Buffer to store sorted triangles (must be large enough!)
    /// * `callback` - Drawing callback for each primitive
    ///
    /// # Returns
    /// Number of triangles rendered
    pub fn render_painters_algorithm<'a, MS, F>(
        &self,
        meshes: MS,
        triangles: &mut [DepthSortedTriangle],
        mut callback: F,
    ) -> usize
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
        F: FnMut(DrawPrimitive),
    {
        let mut count = 0;

        // Collect all triangles with their depths
        for mesh in meshes {
            if mesh.geometry.vertices.is_empty() {
                continue;
            }

            // Frustum culling
            if self.should_cull_mesh(mesh) {
                continue;
            }

            // LOD Selection
            let mesh_pos = mesh.get_position();
            let distance = (mesh_pos - self.camera.position).norm();
            let geometry = mesh.select_lod(distance);

            let transform_matrix = self.camera.vp_matrix * mesh.model_matrix;
            let render_mode = self.resolve_render_mode(&mesh.render_mode);

            // Only collect renderable triangles (solid-style modes)
            #[cfg(feature = "lighting")]
            let solidish = matches!(
                render_mode,
                RenderMode::Solid
                    | RenderMode::SolidLightDir(_)
                    | RenderMode::BlinnPhong { .. }
                    | RenderMode::SectorBright(_)
            );
            #[cfg(not(feature = "lighting"))]
            let solidish = matches!(render_mode, RenderMode::Solid);
            match render_mode {
                _ if solidish => {
                    for (face_idx, face) in geometry.faces.iter().enumerate() {
                        let v = geometry.vertices;
                        if face[0] >= v.len() || face[1] >= v.len() || face[2] >= v.len() {
                            continue;
                        }

                        #[cfg(feature = "lighting")]
                        let v0_world = mesh.model_matrix.transform_point(&nalgebra::Point3::new(
                            v[face[0]][0],
                            v[face[0]][1],
                            v[face[0]][2],
                        ));

                        // Determine a usable world-space face normal.
                        //
                        // If explicit per-face normals are available, trust those and use
                        // position-based backface culling.
                        //
                        // If normals are absent, keep faces (no backface cull). Many existing
                        // demo meshes use mixed winding; forcing cull from inferred winding
                        // makes Painter mode appear "corrupted" because visible faces are
                        // incorrectly discarded.
                        #[cfg(feature = "lighting")]
                        let mut normal_world_opt: Option<Vector3<f32>> = None;
                        if !geometry.normals.is_empty() && face_idx < geometry.normals.len() {
                            let n = geometry.normals[face_idx];
                            let n_world = mesh
                                .model_matrix
                                .transform_vector(&Vector3::new(n[0], n[1], n[2]));
                            if n_world.norm_squared() > 1e-8 {
                                let n_world = n_world.normalize();
                                if self.is_backface(
                                    face,
                                    geometry.vertices,
                                    mesh.model_matrix,
                                    &n_world,
                                ) {
                                    continue;
                                }
                                #[cfg(feature = "lighting")]
                                {
                                    normal_world_opt = Some(n_world);
                                }
                            }
                        }

                        // Fallback normal for flat lighting when explicit normals are absent.
                        #[cfg(feature = "lighting")]
                        let normal_world = if let Some(n) = normal_world_opt {
                            n
                        } else {
                            let v0 = Vector3::new(v[face[0]][0], v[face[0]][1], v[face[0]][2]);
                            let v1 = Vector3::new(v[face[1]][0], v[face[1]][1], v[face[1]][2]);
                            let v2 = Vector3::new(v[face[2]][0], v[face[2]][1], v[face[2]][2]);
                            let normal_model = (v1 - v0).cross(&(v2 - v0));
                            if normal_model.norm_squared() <= 1e-8 {
                                continue;
                            }
                            let mut n = mesh
                                .model_matrix
                                .transform_vector(&normal_model)
                                .normalize();
                            // Flip inferred normal toward camera so flat directional lighting
                            // remains visually stable for mixed-winding assets.
                            if (self.camera.position - v0_world).dot(&n) < 0.0 {
                                n = -n;
                            }
                            n
                        };

                        // Determine flat color based on render mode.
                        #[cfg(feature = "lighting")]
                        let mut color = match render_mode {
                            RenderMode::SolidLightDir(light_dir) => {
                                let adjusted_dir =
                                    Vector3::new(light_dir.x, light_dir.y, -light_dir.z)
                                        .normalize();
                                let intensity = normal_world.dot(&adjusted_dir).max(0.0);
                                let ambient = 0.1;
                                let final_intensity =
                                    (ambient + (1.0 - ambient) * intensity).clamp(0.0, 1.0);
                                Rgb565::new(
                                    (mesh.color.r() as f32 * final_intensity) as u8,
                                    (mesh.color.g() as f32 * final_intensity) as u8,
                                    (mesh.color.b() as f32 * final_intensity) as u8,
                                )
                            }
                            RenderMode::SectorBright(brightness) => {
                                let wc = Self::face_world_center(
                                    face,
                                    geometry.vertices,
                                    mesh.model_matrix,
                                );
                                self.sector_shaded_color(mesh.color, brightness, wc)
                            }
                            // Full per-pixel Blinn-Phong is not supported in painter mode.
                            RenderMode::BlinnPhong { .. } | RenderMode::Solid => mesh.color,
                            _ => mesh.color,
                        };
                        #[cfg(not(feature = "lighting"))]
                        let color = mesh.color;

                        #[cfg(feature = "lighting")]
                        if !self.point_lights.is_empty() {
                            let wc =
                                Self::face_world_center(face, geometry.vertices, mesh.model_matrix);
                            color = Self::add_tint(color, self.light_tint_at(wc));
                        }

                        let clip = [
                            transform_matrix
                                * Vector4::new(v[face[0]][0], v[face[0]][1], v[face[0]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[1]][0], v[face[1]][1], v[face[1]][2], 1.0),
                            transform_matrix
                                * Vector4::new(v[face[2]][0], v[face[2]][1], v[face[2]][2], 1.0),
                        ];
                        // Use a face-coherent sort depth so all clipped fan pieces of the same
                        // source polygon remain locked together during rotation.
                        let face_sort_depth = {
                            let d = (clip[0].w + clip[1].w + clip[2].w) / 3.0;
                            if d.is_finite() { d } else { 0.0 }
                        };

                        self.emit_clipped(clip, color, &mut |prim| {
                            if let DrawPrimitive::ColoredTriangleWithDepth {
                                points,
                                depths: _,
                                color,
                            } = prim
                            {
                                if count < triangles.len() {
                                    triangles[count] = DepthSortedTriangle::new(
                                        DrawPrimitive::ColoredTriangle(points, color),
                                        face_sort_depth,
                                    );
                                    count += 1;
                                }
                            }
                        });
                    }
                }
                // Lines and Points don't need depth sorting
                _ => {}
            }
        }

        // Sort triangles by depth (back-to-front = largest depth first)
        triangles[0..count].sort_unstable_by(|a, b| {
            b.avg_depth
                .partial_cmp(&a.avg_depth)
                .unwrap_or(Ordering::Equal)
        });

        // Render sorted triangles
        for triangle in triangles[0..count].iter() {
            callback(triangle.primitive.clone());
        }

        count
    }

    /// Helper to calculate lit color for directional lighting
    #[allow(dead_code)]
    fn calculate_lit_color(
        &self,
        face: &[usize; 3],
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        base_color: Rgb565,
        light_dir: nalgebra::Vector3<f32>,
    ) -> Rgb565 {
        // Calculate face normal if not provided
        let normal = if !normals.is_empty() && face[0] < normals.len() {
            nalgebra::Vector3::new(
                normals[face[0]][0],
                normals[face[0]][1],
                normals[face[0]][2],
            )
        } else {
            // Compute face normal from vertices
            let v0 = &vertices[face[0]];
            let v1 = &vertices[face[1]];
            let v2 = &vertices[face[2]];

            let edge1 = nalgebra::Vector3::new(v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]);
            let edge2 = nalgebra::Vector3::new(v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]);

            let normal = edge1.cross(&edge2);
            if normal.norm() > 0.0 {
                normal.normalize()
            } else {
                nalgebra::Vector3::new(0.0, 1.0, 0.0)
            }
        };

        // Simple diffuse lighting
        let light_intensity = normal.dot(&light_dir.normalize()).max(0.0);
        let ambient = 0.3;
        let final_intensity = (ambient + (1.0 - ambient) * light_intensity).clamp(0.0, 1.0);

        // Apply lighting to color
        let r = (base_color.r() as f32 * final_intensity) as u8;
        let g = (base_color.g() as f32 * final_intensity) as u8;
        let b = (base_color.b() as f32 * final_intensity) as u8;

        Rgb565::new(r, g, b)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use core::cmp::Ordering;
    use embedded_graphics_core::pixelcolor::Rgb565;
    use nalgebra::Point3;

    #[test]
    fn test_sorting_by_depth() {
        let mut triangles = [
            DepthSortedTriangle {
                primitive: DrawPrimitive::Line(
                    [nalgebra::Point2::new(0, 0), nalgebra::Point2::new(1, 1)],
                    Rgb565::new(31, 0, 0),
                ),
                avg_depth: 10.0,
            },
            DepthSortedTriangle {
                primitive: DrawPrimitive::Line(
                    [nalgebra::Point2::new(0, 0), nalgebra::Point2::new(1, 1)],
                    Rgb565::new(0, 63, 0),
                ),
                avg_depth: 5.0,
            },
            DepthSortedTriangle {
                primitive: DrawPrimitive::Line(
                    [nalgebra::Point2::new(0, 0), nalgebra::Point2::new(1, 1)],
                    Rgb565::new(0, 0, 31),
                ),
                avg_depth: 15.0,
            },
        ];

        triangles.sort_by(|a, b| {
            b.avg_depth
                .partial_cmp(&a.avg_depth)
                .unwrap_or(Ordering::Equal)
        });

        // Should be sorted furthest to nearest (15, 10, 5)
        assert_eq!(triangles[0].avg_depth, 15.0);
        assert_eq!(triangles[1].avg_depth, 10.0);
        assert_eq!(triangles[2].avg_depth, 5.0);
    }

    #[test]
    fn painters_algorithm_clips_partially_offscreen_triangle() {
        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let vertices = [
            [-1.0f32, -1.0, 0.0],
            [1.0f32, -1.0, 0.0],
            [20.0f32, 2.0, 0.0], // intentionally far outside horizontal frustum
        ];
        let faces = [[0usize, 1usize, 2usize]];
        let geometry = crate::mesh::Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut mesh = crate::mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(crate::mesh::RenderMode::Solid);
        mesh.set_color(Rgb565::new(31, 0, 0));

        let mut triangles = [DepthSortedTriangle::DUMMY; 256];
        let count =
            engine.render_painters_algorithm(core::iter::once(&mesh), &mut triangles, |_| {});

        // Regression guard: painter mode must clip partially off-screen faces
        // instead of dropping them entirely.
        assert!(count > 0);
    }

    #[test]
    fn painters_clipped_fan_pieces_share_sort_depth() {
        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let vertices = [
            [-1.0f32, -1.0, 0.0],
            [1.0f32, -1.0, 0.0],
            [30.0f32, 2.0, 0.0], // heavily clipped, typically produces a fan
        ];
        let faces = [[0usize, 1usize, 2usize]];
        let geometry = crate::mesh::Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut mesh = crate::mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(crate::mesh::RenderMode::Solid);
        mesh.set_color(Rgb565::new(0, 63, 0));

        let mut triangles = [DepthSortedTriangle::DUMMY; 256];
        let count =
            engine.render_painters_algorithm(core::iter::once(&mesh), &mut triangles, |_| {});
        assert!(count > 0);
        let first = triangles[0].avg_depth;
        assert!(
            triangles[0..count]
                .iter()
                .all(|t| (t.avg_depth - first).abs() < 1e-5)
        );
    }

    #[test]
    fn painters_calculate_lit_color_and_modes() {
        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(0.0, 0.0, 5.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let face = [0usize, 1usize, 2usize];
        let vertices = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0f32, 0.0, 1.0]];

        let lit_color = engine.calculate_lit_color(
            &face,
            &vertices,
            &normals,
            Rgb565::RED,
            nalgebra::Vector3::new(0.0, 0.0, 1.0),
        );
        assert!(lit_color.r() > 0);

        let lit_color_no_norm = engine.calculate_lit_color(
            &face,
            &vertices,
            &[],
            Rgb565::GREEN,
            nalgebra::Vector3::new(0.0, 0.0, 1.0),
        );
        assert!(lit_color_no_norm.g() > 0);

        let lit_color_zero = engine.calculate_lit_color(
            &face,
            &[[0.0; 3]; 3],
            &[],
            Rgb565::BLUE,
            nalgebra::Vector3::new(0.0, 0.0, 1.0),
        );
        assert!(lit_color_zero.b() > 0);

        // Test SectorBright & point lights in painters
        #[cfg(feature = "lighting")]
        {
            let light =
                crate::lights::PointLight::new(Point3::new(0.0, 0.0, 1.0), Rgb565::WHITE, 10.0);
            engine.add_point_light(light);

            let geom = crate::mesh::Geometry {
                vertices: &vertices,
                faces: &[face],
                normals: &normals,
                colors: &[],
                lines: &[],
                vertex_normals: &[],
                uvs: &[],
                texture_id: None,
            };
            let mut mesh = crate::mesh::K3dMesh::new(geom);
            mesh.set_render_mode(crate::mesh::RenderMode::SectorBright(200));

            let mut triangles = [DepthSortedTriangle::DUMMY; 16];
            let count =
                engine.render_painters_algorithm(core::iter::once(&mesh), &mut triangles, |_| {});
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn painters_out_of_bounds_and_empty_vertices() {
        let engine = K3dengine::new(320, 240);
        let vertices_empty = [[0.0f32, 0.0, 0.0]];
        let geom_empty = crate::mesh::Geometry {
            vertices: &vertices_empty,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mesh_empty = crate::mesh::K3dMesh::new(geom_empty);
        let mut triangles = [DepthSortedTriangle::DUMMY; 16];
        let count =
            engine.render_painters_algorithm(core::iter::once(&mesh_empty), &mut triangles, |_| {});
        assert_eq!(count, 0);

        let vertices = [[0.0f32, 0.0, 0.0]];
        let geom_invalid_face = crate::mesh::Geometry {
            vertices: &vertices,
            faces: &[[0, 1, 2]],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        assert!(!geom_invalid_face.check_validity());
    }
}
