use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
use heapless::Vec;
use log::error;
use nalgebra::{Point3, Similarity3, UnitQuaternion, Vector3};

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
}
#[derive(Debug, Default, Copy, Clone)]
pub struct Geometry<'a> {
    pub vertices: &'a [[f32; 3]],
    pub faces: &'a [[usize; 3]],
    pub colors: &'a [Rgb565],
    pub lines: &'a [[usize; 2]],
    pub normals: &'a [[f32; 3]],
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
        let mut lines: Vec<(usize, usize), N> = Vec::new();
        for face in faces {
            for line in &[(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                let (a, b) = if line.0 < line.1 {
                    (line.0, line.1)
                } else {
                    (line.1, line.0)
                };
                if !lines.iter().any(|&(x, y)| x == a && y == b) {
                    if lines.push((a, b)).is_err() {
                        error!(
                            "lines_from_faces: heapless Vec capacity exceeded (max {}). Some edges will not be rendered.",
                            N
                        );
                        return lines;
                    }
                }
            }
        }
        lines
    }
}

/// Level of Detail configuration for a mesh
///
/// Defines distance thresholds for switching between LOD levels:
/// - 0 to high_distance: Use high detail geometry
/// - high_distance to medium_distance: Use medium detail geometry
/// - Beyond medium_distance: Use low detail geometry
#[derive(Debug, Clone, Copy)]
pub struct LODLevels {
    /// Distance threshold for high detail (0 to this distance)
    pub high_distance: f32,
    /// Distance threshold for medium detail (high_distance to this distance)
    pub medium_distance: f32,
}

impl Default for LODLevels {
    fn default() -> Self {
        Self {
            high_distance: 50.0,
            medium_distance: 100.0,
        }
    }
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
    ///
    /// Returns a reference to the geometry that should be used for rendering
    #[inline]
    pub fn select_lod(&self, distance: f32) -> &Geometry<'_> {
        if distance < self.lod_levels.high_distance {
            // High detail
            &self.geometry
        } else if distance < self.lod_levels.medium_distance {
            // Medium detail
            self.lod_medium.as_ref().unwrap_or(&self.geometry)
        } else {
            // Low detail
            self.lod_low
                .as_ref()
                .unwrap_or(self.lod_medium.as_ref().unwrap_or(&self.geometry))
        }
    }

    pub fn set_color(&mut self, color: Rgb565) {
        self.color = color;
    }

    pub fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
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
        self.update_model_matrix();
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

    /// Compute the squared bounding sphere radius of the mesh in model space.
    /// Returns squared radius to avoid expensive sqrt operation.
    /// This is used for frustum culling.
    #[inline]
    pub fn compute_bounding_radius_sq(&self) -> f32 {
        let mut max_dist_sq = 0.0f32;
        for vertex in self.geometry.vertices {
            let dist_sq = vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2];
            if dist_sq > max_dist_sq {
                max_dist_sq = dist_sq;
            }
        }
        let scale = self.similarity.scaling();
        max_dist_sq * scale * scale
    }
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
            uvs: &[],
            texture_id: None,
        };

        K3dMesh::new(geometry);
    }

    #[test]
    fn test_lines_from_faces_basic() {
        let faces = [[0, 1, 2]];
        let lines = Geometry::lines_from_faces::<10>(&faces);

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
        let lines = Geometry::lines_from_faces::<10>(&faces);

        // Two triangles sharing edge (0,2) should produce 5 unique edges
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_lines_from_faces_capacity_limit() {
        let faces = [[0, 1, 2], [3, 4, 5]];
        // Very small capacity that can't hold all edges
        let lines = Geometry::lines_from_faces::<2>(&faces);

        // Should only contain 2 edges due to capacity limit
        assert_eq!(lines.len(), 2);
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
}
