//! Skeletal animation system with subspace deformation (skinning).
//!
//! Provides hierarchical bone structures and linear blend skinning for
//! deforming meshes based on skeletal transformations.
//!
//! # Example
//! ```
//! use embedded_3dgfx::skeleton::{Skeleton, Bone, SkinningData};
//! use nalgebra::{Vector3, UnitQuaternion};
//!
//! let mut skeleton = Skeleton::<8>::new();
//!
//! // Create root bone
//! let root = skeleton.add_bone(Bone::new("root"), None).unwrap();
//!
//! // Create child bone
//! let child = skeleton.add_bone(
//!     Bone::new("arm").with_position(Vector3::new(0.0, 1.0, 0.0)),
//!     Some(root)
//! ).unwrap();
//!
//! // Update transforms
//! skeleton.update_transforms();
//! ```

use heapless::Vec;
use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector3};

#[allow(unused_imports)]
use nalgebra::ComplexField;

/// Maximum number of bones that can influence a single vertex.
pub const MAX_BONE_INFLUENCES: usize = 4;

/// Unique identifier for a bone within a skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoneId(pub usize);

/// A single bone in a skeleton hierarchy.
///
/// Each bone has a local transform relative to its parent,
/// and a computed world transform used for skinning.
#[derive(Debug, Clone)]
pub struct Bone {
    /// Bone name for debugging
    pub name: heapless::String<32>,

    /// Local position relative to parent
    pub position: Vector3<f32>,

    /// Local rotation relative to parent
    pub rotation: UnitQuaternion<f32>,

    /// Local scale
    pub scale: Vector3<f32>,

    /// Parent bone ID (None for root)
    pub parent: Option<BoneId>,

    /// Local transform matrix (position + rotation + scale)
    pub local_transform: Matrix4<f32>,

    /// World transform matrix (accumulated from root)
    pub world_transform: Matrix4<f32>,

    /// Inverse bind pose matrix (transforms from model space to bone space)
    pub inverse_bind_pose: Matrix4<f32>,
}

impl Bone {
    /// Create a new bone with default transform at origin.
    pub fn new(name: &str) -> Self {
        let mut name_str = heapless::String::new();
        let _ = name_str.push_str(name);

        Self {
            name: name_str,
            position: Vector3::zeros(),
            rotation: UnitQuaternion::identity(),
            scale: Vector3::new(1.0, 1.0, 1.0),
            parent: None,
            local_transform: Matrix4::identity(),
            world_transform: Matrix4::identity(),
            inverse_bind_pose: Matrix4::identity(),
        }
    }

    /// Set the bone's local position.
    pub fn with_position(mut self, position: Vector3<f32>) -> Self {
        self.position = position;
        self.update_local_transform();
        self
    }

    /// Set the bone's local rotation.
    pub fn with_rotation(mut self, rotation: UnitQuaternion<f32>) -> Self {
        self.rotation = rotation;
        self.update_local_transform();
        self
    }

    /// Set the bone's local scale.
    pub fn with_scale(mut self, scale: Vector3<f32>) -> Self {
        self.scale = scale;
        self.update_local_transform();
        self
    }

    /// Update the local transform matrix from position, rotation, and scale.
    pub fn update_local_transform(&mut self) {
        // Build transform matrix: T * R * S
        let translation = Matrix4::new_translation(&self.position);
        let rotation = self.rotation.to_homogeneous();
        let scale = Matrix4::new_nonuniform_scaling(&self.scale);

        self.local_transform = translation * rotation * scale;
    }

    /// Set the position and update transform.
    pub fn set_position(&mut self, position: Vector3<f32>) {
        self.position = position;
        self.update_local_transform();
    }

    /// Set the rotation and update transform.
    pub fn set_rotation(&mut self, rotation: UnitQuaternion<f32>) {
        self.rotation = rotation;
        self.update_local_transform();
    }
}

/// A hierarchical skeleton with bones.
///
/// The generic parameter `N` specifies the maximum number of bones.
#[derive(Debug, Clone)]
pub struct Skeleton<const N: usize> {
    pub bones: Vec<Bone, N>,
}

impl<const N: usize> Skeleton<N> {
    /// Create a new empty skeleton.
    pub fn new() -> Self {
        Self { bones: Vec::new() }
    }

    /// Add a bone to the skeleton.
    ///
    /// Returns the bone ID on success, or an error if the skeleton is full.
    pub fn add_bone(&mut self, mut bone: Bone, parent: Option<BoneId>) -> Result<BoneId, ()> {
        bone.parent = parent;
        bone.update_local_transform();

        let id = BoneId(self.bones.len());
        self.bones.push(bone).map_err(|_| ())?;

        Ok(id)
    }

    /// Get a bone by ID.
    pub fn get_bone(&self, id: BoneId) -> Option<&Bone> {
        self.bones.get(id.0)
    }

    /// Get a mutable reference to a bone by ID.
    pub fn get_bone_mut(&mut self, id: BoneId) -> Option<&mut Bone> {
        self.bones.get_mut(id.0)
    }

    /// Update world transforms for all bones based on hierarchy.
    ///
    /// Must be called after modifying any bone transforms and before skinning.
    pub fn update_transforms(&mut self) {
        // First pass: update local transforms
        for bone in self.bones.iter_mut() {
            bone.update_local_transform();
        }

        // Second pass: compute world transforms (parent-to-child order)
        for i in 0..self.bones.len() {
            let parent_transform = if let Some(parent_id) = self.bones[i].parent {
                self.bones[parent_id.0].world_transform
            } else {
                Matrix4::identity()
            };

            self.bones[i].world_transform = parent_transform * self.bones[i].local_transform;
        }
    }

    /// Compute inverse bind pose matrices for all bones.
    ///
    /// Should be called once after setting up the skeleton in its bind pose.
    pub fn compute_inverse_bind_poses(&mut self) {
        self.update_transforms();

        for bone in self.bones.iter_mut() {
            bone.inverse_bind_pose = bone
                .world_transform
                .try_inverse()
                .unwrap_or(Matrix4::identity());
        }
    }

    /// Get the skinning matrix for a bone (world_transform * inverse_bind_pose).
    pub fn get_skinning_matrix(&self, bone_id: BoneId) -> Matrix4<f32> {
        if let Some(bone) = self.get_bone(bone_id) {
            bone.world_transform * bone.inverse_bind_pose
        } else {
            Matrix4::identity()
        }
    }
}

impl<const N: usize> Default for Skeleton<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Skinning data for a single vertex.
///
/// Stores up to MAX_BONE_INFLUENCES bone indices and their weights.
#[derive(Debug, Clone, Copy)]
pub struct VertexSkinning {
    /// Bone indices (up to MAX_BONE_INFLUENCES)
    pub bone_indices: [usize; MAX_BONE_INFLUENCES],

    /// Bone weights (should sum to 1.0 for proper blending)
    pub bone_weights: [f32; MAX_BONE_INFLUENCES],

    /// Number of active bone influences (1-4)
    pub num_influences: usize,
}

impl VertexSkinning {
    /// Create vertex skinning with a single bone influence.
    pub fn single_bone(bone_index: usize) -> Self {
        Self {
            bone_indices: [bone_index, 0, 0, 0],
            bone_weights: [1.0, 0.0, 0.0, 0.0],
            num_influences: 1,
        }
    }

    /// Create vertex skinning with two bone influences.
    pub fn two_bones(bone0: usize, weight0: f32, bone1: usize, weight1: f32) -> Self {
        Self {
            bone_indices: [bone0, bone1, 0, 0],
            bone_weights: [weight0, weight1, 0.0, 0.0],
            num_influences: 2,
        }
    }

    /// Create vertex skinning with custom bone influences.
    ///
    /// Weights should sum to 1.0 for proper blending.
    pub fn new(
        bone_indices: [usize; MAX_BONE_INFLUENCES],
        bone_weights: [f32; MAX_BONE_INFLUENCES],
        num_influences: usize,
    ) -> Self {
        Self {
            bone_indices,
            bone_weights,
            num_influences: num_influences.min(MAX_BONE_INFLUENCES),
        }
    }
}

impl Default for VertexSkinning {
    fn default() -> Self {
        Self::single_bone(0)
    }
}

/// Skinning data for an entire mesh.
///
/// Associates each vertex with bone influences for deformation.
#[derive(Debug, Clone)]
pub struct SkinningData {
    /// Per-vertex skinning data
    pub vertex_skinning: heapless::Vec<VertexSkinning, 512>,
}

impl SkinningData {
    /// Create new skinning data with capacity for vertices.
    pub fn new() -> Self {
        Self {
            vertex_skinning: Vec::new(),
        }
    }

    /// Add skinning data for a vertex.
    pub fn add_vertex(&mut self, skinning: VertexSkinning) -> Result<(), ()> {
        self.vertex_skinning.push(skinning).map_err(|_| ())
    }
}

impl Default for SkinningData {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply skeletal subspace deformation to a set of vertices.
///
/// Performs linear blend skinning using the skeleton's current pose.
///
/// # Arguments
/// * `skeleton` - The skeleton with current bone transforms
/// * `skinning_data` - Per-vertex bone influences and weights
/// * `source_vertices` - Original vertex positions in bind pose
/// * `output_vertices` - Buffer to write deformed vertices
///
/// # Returns
/// The number of vertices processed.
pub fn apply_skinning<const N: usize>(
    skeleton: &Skeleton<N>,
    skinning_data: &SkinningData,
    source_vertices: &[[f32; 3]],
    output_vertices: &mut [[f32; 3]],
) -> usize {
    let count = source_vertices
        .len()
        .min(output_vertices.len())
        .min(skinning_data.vertex_skinning.len());

    for i in 0..count {
        let vertex = Point3::new(
            source_vertices[i][0],
            source_vertices[i][1],
            source_vertices[i][2],
        );

        let skinning = &skinning_data.vertex_skinning[i];
        let mut deformed = Point3::new(0.0, 0.0, 0.0);

        // Linear blend skinning: sum of weighted bone transforms
        for j in 0..skinning.num_influences {
            let bone_id = BoneId(skinning.bone_indices[j]);
            let weight = skinning.bone_weights[j];

            if weight > 0.0 {
                let skinning_matrix = skeleton.get_skinning_matrix(bone_id);
                let transformed = skinning_matrix.transform_point(&vertex);
                deformed += transformed.coords * weight;
            }
        }

        output_vertices[i] = [deformed.x, deformed.y, deformed.z];
    }

    count
}

/// Apply skeletal subspace deformation to normals.
///
/// Normals require special handling - they're transformed by the inverse transpose
/// of the skinning matrix to remain perpendicular to the surface.
///
/// # Arguments
/// * `skeleton` - The skeleton with current bone transforms
/// * `skinning_data` - Per-vertex bone influences and weights
/// * `source_normals` - Original normal vectors in bind pose
/// * `output_normals` - Buffer to write deformed normals
///
/// # Returns
/// The number of normals processed.
pub fn apply_skinning_to_normals<const N: usize>(
    skeleton: &Skeleton<N>,
    skinning_data: &SkinningData,
    source_normals: &[[f32; 3]],
    output_normals: &mut [[f32; 3]],
) -> usize {
    let count = source_normals
        .len()
        .min(output_normals.len())
        .min(skinning_data.vertex_skinning.len());

    for i in 0..count {
        let normal = Vector3::new(
            source_normals[i][0],
            source_normals[i][1],
            source_normals[i][2],
        );

        let skinning = &skinning_data.vertex_skinning[i];
        let mut deformed = Vector3::zeros();

        for j in 0..skinning.num_influences {
            let bone_id = BoneId(skinning.bone_indices[j]);
            let weight = skinning.bone_weights[j];

            if weight > 0.0 {
                let skinning_matrix = skeleton.get_skinning_matrix(bone_id);

                // For normals, use inverse transpose (approximated by the 3x3 rotation part)
                let rotation_part = skinning_matrix.fixed_view::<3, 3>(0, 0);
                let transformed = rotation_part * normal;
                deformed += transformed * weight;
            }
        }

        // Normalize the result
        let normalized = deformed.normalize();
        output_normals[i] = [normalized.x, normalized.y, normalized.z];
    }

    count
}

#[cfg(feature = "dsp")]
impl Bone {
    /// Perform spherical linear interpolation (SLERP) between two bone rotations using `embedded-dsp` quaternions.
    pub fn interpolate_rotation_dsp(&mut self, target_rotation: UnitQuaternion<f32>, t: f32) {
        let q1 = [self.rotation.w, self.rotation.i, self.rotation.j, self.rotation.k];
        let q2 = [target_rotation.w, target_rotation.i, target_rotation.j, target_rotation.k];
        let dot = q1[0] * q2[0] + q1[1] * q2[1] + q1[2] * q2[2] + q1[3] * q2[3];
        let q2_adj = if dot < 0.0 {
            [-q2[0], -q2[1], -q2[2], -q2[3]]
        } else {
            q2
        };
        let t_clamped = t.clamp(0.0, 1.0);
        let mut interpolated = [
            q1[0] + (q2_adj[0] - q1[0]) * t_clamped,
            q1[1] + (q2_adj[1] - q1[1]) * t_clamped,
            q1[2] + (q2_adj[2] - q1[2]) * t_clamped,
            q1[3] + (q2_adj[3] - q1[3]) * t_clamped,
        ];
        let _ = embedded_dsp::quaternion_normalize_f32(&mut interpolated);

        self.rotation = UnitQuaternion::new_normalize(
            nalgebra::Quaternion::new(interpolated[0], interpolated[1], interpolated[2], interpolated[3]),
        );
        self.update_local_transform();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bone_creation() {
        let bone = Bone::new("test_bone");
        assert_eq!(bone.name.as_str(), "test_bone");
        assert_eq!(bone.position, Vector3::zeros());
        assert_eq!(bone.parent, None);
    }

    #[test]
    fn test_skeleton_add_bone() {
        let mut skeleton = Skeleton::<4>::new();

        let root = skeleton.add_bone(Bone::new("root"), None);
        assert!(root.is_ok());

        let root_id = root.unwrap();
        let child = skeleton.add_bone(Bone::new("child"), Some(root_id));
        assert!(child.is_ok());

        assert_eq!(skeleton.bones.len(), 2);
    }

    #[test]
    fn test_hierarchy_transforms() {
        let mut skeleton = Skeleton::<4>::new();

        // Root at origin
        let root = skeleton.add_bone(Bone::new("root"), None).unwrap();

        // Child offset by (1, 0, 0)
        let child = skeleton
            .add_bone(
                Bone::new("child").with_position(Vector3::new(1.0, 0.0, 0.0)),
                Some(root),
            )
            .unwrap();

        skeleton.update_transforms();

        // Child's world position should be (1, 0, 0)
        let child_bone = skeleton.get_bone(child).unwrap();
        let world_pos = child_bone.world_transform.column(3);
        assert!((world_pos.x - 1.0).abs() < 0.001);
        assert!(world_pos.y.abs() < 0.001);
        assert!(world_pos.z.abs() < 0.001);
    }

    #[test]
    fn test_vertex_skinning_single_bone() {
        let skinning = VertexSkinning::single_bone(0);
        assert_eq!(skinning.num_influences, 1);
        assert_eq!(skinning.bone_weights[0], 1.0);
    }

    #[test]
    fn test_vertex_skinning_two_bones() {
        let skinning = VertexSkinning::two_bones(0, 0.7, 1, 0.3);
        assert_eq!(skinning.num_influences, 2);
        assert_eq!(skinning.bone_weights[0], 0.7);
        assert_eq!(skinning.bone_weights[1], 0.3);
    }
}
