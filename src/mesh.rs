#[cfg(feature = "aabb-cull")]
use crate::bounds::Aabb;
#[cfg(feature = "render-layers")]
use crate::render_layers::RenderLayers;
#[cfg(feature = "lod-crossfade")]
use core::cell::Cell;
use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
use heapless::Vec;
use heapless::index_set::FnvIndexSet;
use log::error;
use nalgebra::{Point3, Similarity3, UnitQuaternion, Vector3};

#[cfg(not(feature = "std"))]
use micromath::F32Ext;

#[derive(Debug, PartialEq, Clone)]
pub enum RenderMode {
    Points,
    Lines,
    Solid,
    SolidLightDir(Vector3<f32>),
    BlinnPhong {
        light_dir: Vector3<f32>,
        specular_intensity: f32,
        shininess: f32,
    },
    GouraudLightDir(Vector3<f32>),
    /// Flat-shaded with a uniform brightness level (0=black, 255=full color).
    /// Used for Doom-style sector-based lighting.
    SectorBright(u8),
    /// Flat, unlit, texture-sampled -- the `Solid` mode's texture-mapped
    /// counterpart. Requires `geometry.uvs` (one per vertex) and
    /// `geometry.texture_id`; faces are silently skipped if either is
    /// missing. Must be drawn via [`crate::K3dengine::execute_with_textures`]
    /// (plain [`crate::K3dengine::execute`] can't resolve `texture_id`
    /// without a [`crate::texture::TextureManager`]).
    Textured,
    /// Textured surface combined with Gouraud lighting.
    TexturedGouraud(Vector3<f32>),
    /// Spherical Environment Mapping (MatCap) shading. Procedurally generates
    /// UV coordinates based on camera-space normals. Uses the mesh's
    /// `geometry.texture_id` as the MatCap sphere map.
    MatCap,
    /// Toon cel-shading with a light direction and number of discrete shading bands.
    Toon(Vector3<f32>, u8),
}
#[derive(Debug, Default, Copy, Clone)]
pub struct Geometry<'a> {
    pub vertices: &'a [[f32; 3]],
    pub faces: &'a [[usize; 3]],
    pub colors: &'a [Rgb565],
    pub lines: &'a [[usize; 2]],
    pub normals: &'a [[f32; 3]],
    /// Per-vertex normals for smooth (Gouraud) shading.
    /// If non-empty, must have the same length as `vertices`.
    pub vertex_normals: &'a [[f32; 3]],
    /// UV texture coordinates (one per vertex)
    pub uvs: &'a [[f32; 2]],
    /// Optional texture ID for this geometry
    pub texture_id: Option<u32>,
}

impl Geometry<'_> {
    fn check_validity(&self) -> bool {
        if self.vertices.is_empty() {
            error!("Vertices are empty");
            return false;
        }

        for face in self.faces {
            if face[0] >= self.vertices.len()
                || face[1] >= self.vertices.len()
                || face[2] >= self.vertices.len()
            {
                error!("Face vertices are out of bounds");
                return false;
            }
        }

        for line in self.lines {
            if line[0] >= self.vertices.len() || line[1] >= self.vertices.len() {
                error!("Line vertices are out of bounds");
                return false;
            }
        }

        if !self.colors.is_empty() && self.colors.len() != self.vertices.len() {
            error!("Colors are not the same length as vertices");
            return false;
        }

        if !self.uvs.is_empty() && self.uvs.len() != self.vertices.len() {
            error!("UVs are not the same length as vertices");
            return false;
        }

        if !self.vertex_normals.is_empty() && self.vertex_normals.len() != self.vertices.len() {
            error!("Vertex normals are not the same length as vertices");
            return false;
        }

        true
    }

    /// Converts faces to unique edge pairs for line rendering.
    ///
    /// # Type Parameters
    /// * `N` - Maximum capacity for the edges buffer. For a closed mesh, a good estimate is
    ///   `faces.len() * 3 / 2` since each edge is typically shared by 2 faces.
    ///
    /// # Returns
    /// A heapless Vec containing unique edge pairs. If capacity is exceeded, returns
    /// partial results with an error logged.
    pub fn lines_from_faces<const N: usize>(faces: &[[usize; 3]]) -> Vec<(usize, usize), N> {
        let mut set: FnvIndexSet<(usize, usize), N> = FnvIndexSet::new();
        for face in faces {
            for &(i1, i2) in &[(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                let edge = if i1 < i2 { (i1, i2) } else { (i2, i1) };
                if set.insert(edge).is_err() {
                    error!(
                        "lines_from_faces: heapless Vec capacity exceeded (max {}). Some edges will not be rendered.",
                        N
                    );
                    break;
                }
            }
        }
        set.iter().copied().collect()
    }
}

/// Level of Detail configuration for a mesh
///
/// Defines distance thresholds for switching between LOD levels:
/// - 0 to high_distance: Use high detail geometry
/// - high_distance to medium_distance: Use medium detail geometry
/// - Beyond medium_distance: Use low detail geometry
///
/// When [`Self::fade_margin`] > 0, LODs crossfade over that distance band
/// (distance-band margins) instead of switching abruptly.
#[derive(Debug, Clone, Copy)]
pub struct LODLevels {
    /// Distance threshold for high detail (0 to this distance)
    pub high_distance: f32,
    /// Distance threshold for medium detail (high_distance to this distance)
    pub medium_distance: f32,
    /// Crossfade half-width in world units around each LOD boundary.
    /// `0.0` (default) keeps abrupt switches. Requires `lod-crossfade`.
    #[cfg(feature = "lod-crossfade")]
    pub fade_margin: f32,
}

impl Default for LODLevels {
    fn default() -> Self {
        Self {
            high_distance: 50.0,
            medium_distance: 100.0,
            #[cfg(feature = "lod-crossfade")]
            fade_margin: 0.0,
        }
    }
}

/// Result of LOD selection, including optional crossfade.
#[cfg(feature = "lod-crossfade")]
#[derive(Debug, Clone, Copy)]
pub enum LodPick<'a> {
    /// Single geometry, fully opaque.
    Single(&'a Geometry<'a>),
    /// Blend between two LOD levels. `t = 0` is fully `near`, `t = 1` is fully `far`.
    Crossfade {
        near: &'a Geometry<'a>,
        far: &'a Geometry<'a>,
        t: f32,
    },
}

/// A mesh with optional Level of Detail (LOD) support
pub struct K3dMesh<'a> {
    pub similarity: Similarity3<f32>,
    pub model_matrix: nalgebra::Matrix4<f32>,

    pub color: Rgb565,
    pub render_mode: RenderMode,
    pub geometry: Geometry<'a>,

    /// Optional LOD geometries (medium detail, low detail)
    /// If None, only the main geometry is used
    pub lod_medium: Option<Geometry<'a>>,
    pub lod_low: Option<Geometry<'a>>,
    pub lod_levels: LODLevels,
    pub priority: u8,
    pub outline_color: Option<Rgb565>,
    pub outline_width: f32,
    /// Cached model-space AABB used for two-stage frustum culling / raycast.
    /// Call [`Self::cache_aabb`] after geometry changes (or at load time).
    #[cfg(feature = "aabb-cull")]
    pub aabb: Option<Aabb>,
    /// Visibility layers. Default: layer 0 (intersects the default camera).
    #[cfg(feature = "render-layers")]
    pub layers: RenderLayers,
    /// Force a specific LOD level for the next render pass (`0` high, `1` medium, `2` low).
    /// Used internally for crossfade; clear after use.
    #[cfg(feature = "lod-crossfade")]
    pub(crate) lod_force: Cell<Option<u8>>,
    /// Optional per-primitive alpha override for the next render pass (crossfade).
    #[cfg(feature = "lod-crossfade")]
    pub(crate) draw_alpha: Cell<Option<u8>>,
}

impl<'a> K3dMesh<'a> {
    pub fn new(geometry: Geometry) -> K3dMesh {
        assert!(geometry.check_validity());
        let sim = Similarity3::new(Vector3::new(0.0, 0.0, 0.0), nalgebra::zero(), 1.0);
        K3dMesh {
            model_matrix: sim.to_homogeneous(),
            similarity: sim,
            color: Rgb565::CSS_WHITE,
            render_mode: RenderMode::Points,
            geometry,
            lod_medium: None,
            lod_low: None,
            lod_levels: LODLevels::default(),
            priority: 128,
            outline_color: None,
            outline_width: 0.0,
            #[cfg(feature = "aabb-cull")]
            aabb: Aabb::enclosing(geometry.vertices.iter()),
            #[cfg(feature = "render-layers")]
            layers: RenderLayers::DEFAULT,
            #[cfg(feature = "lod-crossfade")]
            lod_force: Cell::new(None),
            #[cfg(feature = "lod-crossfade")]
            draw_alpha: Cell::new(None),
        }
    }

    /// Set LOD geometries for this mesh
    ///
    /// # Arguments
    /// * `medium` - Medium detail geometry (optional)
    /// * `low` - Low detail geometry (optional)
    /// * `levels` - Distance thresholds for switching LOD levels
    pub fn set_lod<'b>(
        &mut self,
        medium: Option<Geometry<'b>>,
        low: Option<Geometry<'b>>,
        levels: LODLevels,
    ) where
        'b: 'a,
    {
        if let Some(ref geom) = medium {
            assert!(geom.check_validity());
        }
        if let Some(ref geom) = low {
            assert!(geom.check_validity());
        }
        self.lod_medium = medium;
        self.lod_low = low;
        self.lod_levels = levels;
    }

    /// Select the appropriate geometry based on distance from camera
    #[inline]
    pub fn select_lod(&self, distance: f32) -> &Geometry<'_> {
        #[cfg(feature = "lod-crossfade")]
        {
            if let Some(level) = self.lod_force.get() {
                return self.geometry_for_lod_level(level);
            }
            match self.select_lod_pick(distance) {
                LodPick::Single(g) => g,
                LodPick::Crossfade { near, far, t } => {
                    if t < 0.5 {
                        near
                    } else {
                        far
                    }
                }
            }
        }
        #[cfg(not(feature = "lod-crossfade"))]
        {
            if distance < self.lod_levels.high_distance {
                &self.geometry
            } else if distance < self.lod_levels.medium_distance {
                self.lod_medium.as_ref().unwrap_or(&self.geometry)
            } else {
                self.lod_low
                    .as_ref()
                    .unwrap_or(self.lod_medium.as_ref().unwrap_or(&self.geometry))
            }
        }
    }

    #[cfg(feature = "lod-crossfade")]
    #[inline]
    fn geometry_for_lod_level(&self, level: u8) -> &Geometry<'_> {
        match level {
            0 => &self.geometry,
            1 => self.lod_medium.as_ref().unwrap_or(&self.geometry),
            _ => self
                .lod_low
                .as_ref()
                .unwrap_or(self.lod_medium.as_ref().unwrap_or(&self.geometry)),
        }
    }

    /// Map a geometry pointer from [`Self::select_lod_pick`] back to a LOD level id.
    #[cfg(feature = "lod-crossfade")]
    #[inline]
    pub(crate) fn lod_level_of(&self, geom: &Geometry<'_>) -> u8 {
        if core::ptr::eq(geom as *const _, &self.geometry as *const _) {
            0
        } else if self
            .lod_medium
            .as_ref()
            .is_some_and(|m| core::ptr::eq(geom as *const _, m as *const _))
        {
            1
        } else {
            2
        }
    }

    /// LOD selection with optional crossfade when [`LODLevels::fade_margin`] > 0.
    #[cfg(feature = "lod-crossfade")]
    #[inline]
    pub fn select_lod_pick(&self, distance: f32) -> LodPick<'_> {
        let high = &self.geometry;
        let medium = self.lod_medium.as_ref().unwrap_or(high);
        let low = self
            .lod_low
            .as_ref()
            .unwrap_or(self.lod_medium.as_ref().unwrap_or(high));

        let margin = self.lod_levels.fade_margin;
        if margin <= 0.0 {
            return if distance < self.lod_levels.high_distance {
                LodPick::Single(high)
            } else if distance < self.lod_levels.medium_distance {
                LodPick::Single(medium)
            } else {
                LodPick::Single(low)
            };
        }

        let h = self.lod_levels.high_distance;
        let m = self.lod_levels.medium_distance;

        if distance < h - margin {
            LodPick::Single(high)
        } else if distance < h + margin {
            let t = ((distance - (h - margin)) / (2.0 * margin)).clamp(0.0, 1.0);
            if core::ptr::eq(high as *const _, medium as *const _) {
                LodPick::Single(high)
            } else {
                LodPick::Crossfade {
                    near: high,
                    far: medium,
                    t,
                }
            }
        } else if distance < m - margin {
            LodPick::Single(medium)
        } else if distance < m + margin {
            let t = ((distance - (m - margin)) / (2.0 * margin)).clamp(0.0, 1.0);
            if core::ptr::eq(medium as *const _, low as *const _) {
                LodPick::Single(medium)
            } else {
                LodPick::Crossfade {
                    near: medium,
                    far: low,
                    t,
                }
            }
        } else {
            LodPick::Single(low)
        }
    }

    pub fn set_color(&mut self, color: Rgb565) {
        self.color = color;
    }

    pub fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
    }

    pub fn set_priority(&mut self, priority: u8) {
        self.priority = priority;
    }

    #[cfg(feature = "render-layers")]
    pub fn set_layers(&mut self, layers: RenderLayers) {
        self.layers = layers;
    }

    /// Recompute and cache the model-space AABB from the primary geometry.
    #[cfg(feature = "aabb-cull")]
    pub fn cache_aabb(&mut self) {
        self.aabb = Aabb::enclosing(self.geometry.vertices.iter());
    }

    /// Override the cached AABB (e.g. skinned bounds in model space).
    #[cfg(feature = "aabb-cull")]
    pub fn set_aabb(&mut self, aabb: Aabb) {
        self.aabb = Some(aabb);
    }

    /// Model-space AABB, computing on demand if not cached.
    #[cfg(feature = "aabb-cull")]
    #[inline]
    pub fn model_aabb(&self) -> Aabb {
        self.aabb
            .unwrap_or_else(|| Aabb::enclosing(self.geometry.vertices.iter()).unwrap_or(Aabb::ZERO))
    }

    pub fn set_position(&mut self, x: f32, y: f32, z: f32) {
        self.similarity.isometry.translation.x = x;
        self.similarity.isometry.translation.y = y;
        self.similarity.isometry.translation.z = z;
        self.update_model_matrix();
    }

    pub fn get_position(&self) -> Point3<f32> {
        self.similarity.isometry.translation.vector.into()
    }

    pub fn set_attitude(&mut self, roll: f32, pitch: f32, yaw: f32) {
        self.similarity.isometry.rotation = UnitQuaternion::from_euler_angles(roll, pitch, yaw);
        self.update_model_matrix();
    }

    /// Set orientation directly from a unit quaternion.
    pub fn set_rotation(&mut self, rotation: UnitQuaternion<f32>) {
        self.similarity.isometry.rotation = rotation;
        self.update_model_matrix();
    }

    pub fn set_target(&mut self, target: Point3<f32>) {
        let view = Similarity3::look_at_rh(
            &self.similarity.isometry.translation.vector.into(),
            &target,
            &Vector3::y(),
            1.0,
        );

        self.similarity = view;
        self.model_matrix = self.similarity.to_homogeneous();
    }

    pub fn set_scale(&mut self, s: f32) {
        if s == 0.0 {
            return;
        }
        self.similarity.set_scaling(s);
        self.update_model_matrix();
    }

    fn update_model_matrix(&mut self) {
        self.model_matrix = self.similarity.to_homogeneous();
    }

    /// Compute the squared bounding sphere radius of the mesh in world-ish
    /// scale (model-space radius × uniform scale²).
    ///
    /// With `aabb-cull`, uses the cached AABB when present (tighter than
    /// origin-centered verts).
    #[inline]
    pub fn compute_bounding_radius_sq(&self) -> f32 {
        let scale = self.similarity.scaling();
        let scale_sq = scale * scale;
        #[cfg(feature = "aabb-cull")]
        if let Some(aabb) = self.aabb {
            let center_sq = aabb.center.norm_squared();
            let r = aabb.radius();
            let rad = r + center_sq.sqrt();
            return rad * rad * scale_sq;
        }
        let mut max_dist_sq = 0.0f32;
        for vertex in self.geometry.vertices {
            let dist_sq = vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2];
            if dist_sq > max_dist_sq {
                max_dist_sq = dist_sq;
            }
        }
        max_dist_sq * scale_sq
    }
}

/// Compute per-vertex normals by averaging the face normals of all faces
/// that share each vertex, then normalizing.
///
/// # Type Parameters
/// * `V` - Maximum number of vertices (capacity of the returned Vec)
///
/// # Returns
/// A heapless Vec with one normal per vertex. Vertices not referenced by any
/// face get a zero normal.
pub fn compute_vertex_normals<const V: usize>(
    vertices: &[[f32; 3]],
    faces: &[[usize; 3]],
    face_normals: &[[f32; 3]],
) -> Vec<[f32; 3], V> {
    let mut normals = Vec::<[f32; 3], V>::new();
    for _ in 0..vertices.len() {
        if normals.push([0.0, 0.0, 0.0]).is_err() {
            break;
        }
    }

    for (face, fn_arr) in faces.iter().zip(face_normals.iter()) {
        for &vi in face {
            if vi < normals.len() {
                normals[vi][0] += fn_arr[0];
                normals[vi][1] += fn_arr[1];
                normals[vi][2] += fn_arr[2];
            }
        }
    }

    for n in normals.iter_mut() {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-10 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        }
    }

    normals
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn test_geometry_validation_valid() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let faces = [[0, 1, 2]];

        let geometry = Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        assert!(geometry.check_validity());
    }

    #[test]
    #[should_panic]
    fn test_geometry_validation_invalid_face_index() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let faces = [[0, 1, 5]]; // Index 5 is out of bounds

        let geometry = Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        // This should panic because we call assert! in K3dMesh::new
        K3dMesh::new(geometry);
    }

    #[test]
    #[should_panic]
    fn test_geometry_validation_invalid_line_index() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let lines = [[0, 10]]; // Index 10 is out of bounds

        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &lines,
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        K3dMesh::new(geometry);
    }

    #[test]
    #[should_panic]
    fn test_geometry_validation_color_length_mismatch() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let colors = [Rgb565::CSS_RED]; // Only 1 color for 2 vertices

        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &colors,
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        K3dMesh::new(geometry);
    }

    #[test]
    fn test_lines_from_faces_basic() {
        let faces = [[0, 1, 2]];
        let lines = Geometry::lines_from_faces::<16>(&faces);

        // Triangle should produce 3 unique edges
        assert_eq!(lines.len(), 3);

        // Check that edges are unique and normalized (smaller index first)
        let expected_edges = [(0, 1), (0, 2), (1, 2)];
        for edge in expected_edges.iter() {
            assert!(lines.contains(edge));
        }
    }

    #[test]
    fn test_lines_from_faces_shared_edges() {
        let faces = [[0, 1, 2], [0, 2, 3]];
        let lines = Geometry::lines_from_faces::<16>(&faces);

        // Two triangles sharing edge (0,2) should produce 5 unique edges
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_lines_from_faces_capacity_limit() {
        let faces = [[0, 1, 2], [3, 4, 5]];
        // Small capacity that can't hold all 6 edges (needs at least 16 for IndexSet)
        let lines = Geometry::lines_from_faces::<16>(&faces);

        // Should contain all 6 edges since capacity is sufficient
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn test_mesh_creation() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mesh = K3dMesh::new(geometry);
        assert_eq!(mesh.color, Rgb565::CSS_WHITE);
        assert_eq!(mesh.render_mode, RenderMode::Points);
        assert_eq!(mesh.get_position(), Point3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn test_mesh_set_color() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_color(Rgb565::CSS_RED);
        assert_eq!(mesh.color, Rgb565::CSS_RED);
    }

    #[test]
    fn test_mesh_set_position() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_position(5.0, 10.0, 15.0);
        assert_eq!(mesh.get_position(), Point3::new(5.0, 10.0, 15.0));
    }

    #[test]
    fn test_mesh_set_scale() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_scale(2.0);
        assert!((mesh.similarity.scaling() - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_mesh_set_scale_zero_ignored() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        let original_scale = mesh.similarity.scaling();
        mesh.set_scale(0.0);
        // Scale should remain unchanged
        assert_eq!(mesh.similarity.scaling(), original_scale);
    }

    #[test]
    fn test_mesh_set_attitude() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_attitude(0.1, 0.2, 0.3);
        // Just verify it doesn't panic and updates the matrix
        assert_ne!(mesh.model_matrix, nalgebra::Matrix4::identity());
    }

    #[test]
    fn test_mesh_set_target() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);
        mesh.set_position(5.0, 5.0, 5.0);
        mesh.set_target(Point3::new(0.0, 0.0, 0.0));
        // Mesh should now be oriented toward origin
        // Just verify it doesn't panic
        assert_ne!(mesh.model_matrix, nalgebra::Matrix4::identity());
    }

    #[test]
    fn test_mesh_render_mode_changes() {
        let vertices = [[0.0, 0.0, 0.0]];
        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = K3dMesh::new(geometry);

        mesh.set_render_mode(RenderMode::Lines);
        assert_eq!(mesh.render_mode, RenderMode::Lines);

        mesh.set_render_mode(RenderMode::Solid);
        assert_eq!(mesh.render_mode, RenderMode::Solid);

        mesh.set_render_mode(RenderMode::SolidLightDir(Vector3::new(0.0, 1.0, 0.0)));
        assert!(matches!(mesh.render_mode, RenderMode::SolidLightDir(_)));
    }

    #[test]
    fn test_compute_vertex_normals_single_triangle() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let faces = [[0, 1, 2]];
        let face_normals = [[0.0, 0.0, 1.0]];

        let vn = compute_vertex_normals::<8>(&vertices, &faces, &face_normals);
        assert_eq!(vn.len(), 3);
        for n in vn.iter() {
            assert!((n[0] - 0.0).abs() < 1e-5);
            assert!((n[1] - 0.0).abs() < 1e-5);
            assert!((n[2] - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_compute_vertex_normals_shared_edge() {
        // Two triangles sharing edge (0,1), with normals pointing in +Z and +Y
        let vertices = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.5, 0.0, 1.0],
            [0.5, 1.0, 0.0],
        ];
        let faces = [[0, 1, 2], [0, 1, 3]];
        let face_normals = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0]];

        let vn = compute_vertex_normals::<8>(&vertices, &faces, &face_normals);
        assert_eq!(vn.len(), 4);

        // Shared vertices 0 and 1 should have averaged normals
        let _expected_len = (0.5f32 * 0.5 + 0.5 * 0.5).sqrt(); // ~0.707
        for i in 0..2 {
            let n = &vn[i];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "Normal should be unit length");
            assert!(
                (n[1] - n[2]).abs() < 1e-5,
                "Y and Z components should be equal for shared verts"
            );
        }

        // Vertex 2: only in face 0, should be [0,0,1]
        assert!((vn[2][2] - 1.0).abs() < 1e-5);
        // Vertex 3: only in face 1, should be [0,1,0]
        assert!((vn[3][1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_geometry_validation_vertex_normals_length_mismatch() {
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let vn = [[0.0, 0.0, 1.0]]; // Only 1 normal for 2 vertices

        let geometry = Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &vn,
            uvs: &[],
            texture_id: None,
        };

        assert!(!geometry.check_validity());
    }

    #[cfg(feature = "lod-crossfade")]
    #[test]
    fn test_lod_crossfade_pick() {
        let high = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let med = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let faces = [[0usize, 1, 2]];
        let mut mesh = K3dMesh::new(Geometry {
            vertices: &high,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        });
        mesh.set_lod(
            Some(Geometry {
                vertices: &med,
                faces: &faces,
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

    #[cfg(feature = "aabb-cull")]
    #[test]
    fn test_mesh_aabb_cached_on_new() {
        let vertices = [[-2.0, -1.0, 0.0], [2.0, 1.0, 0.0]];
        let mesh = K3dMesh::new(Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        });
        let aabb = mesh.aabb.expect("cached");
        assert!((aabb.half_extents.x - 2.0).abs() < 1e-5);
        assert!((aabb.half_extents.y - 1.0).abs() < 1e-5);
    }
}
