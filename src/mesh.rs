use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};
use heapless::{Vec, FnvIndexSet};
use log::error;
use nalgebra::{Point3, Similarity3, UnitQuaternion, Vector3};

#[derive(Debug, PartialEq)]
pub enum RenderMode {
    Points,
    Lines,
    Solid,
    SolidLightDir(Vector3<f32>),
}
#[derive(Debug, Default)]
pub struct Geometry<'a> {
    pub vertices: &'a [[f32; 3]],
    pub faces: &'a [[usize; 3]],
    pub colors: &'a [Rgb565],
    pub lines: &'a [[usize; 2]],
    pub normals: &'a [[f32; 3]],
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

        true
    }

    pub fn lines_from_faces(faces: &[[usize; 3]]) -> Vec<(usize, usize), 512> {
        let mut set: FnvIndexSet<(usize, usize), 512> = FnvIndexSet::new();
        for face in faces {
            for &(a, b) in &[(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                let edge = if a < b { (a, b) } else { (b, a) };
                let _ = set.insert(edge);
            }
        }
        set.into_iter().copied().collect()
    }
}

pub struct K3dMesh<'a> {
    pub similarity: Similarity3<f32>,
    pub model_matrix: nalgebra::Matrix4<f32>,
    model_dirty: bool, // new field to track matrix validity

    pub color: Rgb565,
    pub render_mode: RenderMode,
    pub geometry: Geometry<'a>,
}

impl K3dMesh<'_> {
    pub fn new(geometry: Geometry) -> K3dMesh {
        assert!(geometry.check_validity());
        let sim = Similarity3::new(Vector3::new(0.0, 0.0, 0.0), nalgebra::zero(), 1.0);
        K3dMesh {
            model_matrix: sim.to_homogeneous(),
            similarity: sim,
            model_dirty: false,
            color: Rgb565::CSS_WHITE,
            render_mode: RenderMode::Points,
            geometry,
        }
    }

    pub fn set_color(&mut self, color: Rgb565) {
        self.color = color;
    }

    pub fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
    }

    pub fn set_position(&mut self, x: f32, y: f32, z: f32) {
        let t = &mut self.similarity.isometry.translation;
        if t.x != x || t.y != y || t.z != z {
            t.x = x;
            t.y = y;
            t.z = z;
            self.model_dirty = true;
        }
    }

    pub fn get_position(&self) -> Point3<f32> {
        self.similarity.isometry.translation.vector.into()
    }

    pub fn set_attitude(&mut self, roll: f32, pitch: f32, yaw: f32) {
        let new_rot = UnitQuaternion::from_euler_angles(roll, pitch, yaw);
        if self.similarity.isometry.rotation != new_rot {
            self.similarity.isometry.rotation = new_rot;
            self.model_dirty = true;
        }
    }

    pub fn set_target(&mut self, target: Point3<f32>) {
        let view = Similarity3::look_at_rh(
            &self.similarity.isometry.translation.vector.into(),
            &target,
            &Vector3::y(),
            1.0,
        );

        self.similarity = view;
        self.model_dirty = true;
    }

    pub fn set_scale(&mut self, s: f32) {
        if s != 0.0 && self.similarity.scaling() != s {
            self.similarity.set_scaling(s);
            self.model_dirty = true;
        }
    }

    pub fn get_scale(&self) -> f32 {
        self.similarity.scaling()
    }

    pub fn translate(&mut self, dx: f32, dy: f32, dz: f32) {
        if dx != 0.0 || dy != 0.0 || dz != 0.0 {
            self.similarity.isometry.translation.x += dx;
            self.similarity.isometry.translation.y += dy;
            self.similarity.isometry.translation.z += dz;
            self.model_dirty = true;
        }
    }

    pub fn rotate(&mut self, roll: f32, pitch: f32, yaw: f32) {
        let additional_rot = UnitQuaternion::from_euler_angles(roll, pitch, yaw);
        self.similarity.isometry.rotation *= additional_rot;
        self.model_dirty = true;
    }

    pub fn update_model_matrix(&mut self) {
        if self.model_dirty {
            self.model_matrix = self.similarity.to_homogeneous();
            self.model_dirty = false;
        }
    }

    pub fn get_model_matrix(&mut self) -> nalgebra::Matrix4<f32> {
        self.update_model_matrix();
        self.model_matrix
    }
}


impl<'a> Default for K3dMesh<'a> {
    fn default() -> Self {
        let geometry = Geometry {
            vertices: &[],
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
        };
        let sim = Similarity3::new(Vector3::zeros(), nalgebra::zero(), 1.0);
        Self {
            similarity: sim,
            model_matrix: sim.to_homogeneous(),
            model_dirty: false,
            color: Rgb565::CSS_WHITE,
            render_mode: RenderMode::Points,
            geometry,
        }
    }
}