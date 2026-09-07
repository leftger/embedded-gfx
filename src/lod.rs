//! Multi-level-of-detail (LOD) geometry selector for MCU rendering.

use crate::mesh::Geometry;
use nalgebra::Point3;

/// A single level of detail threshold binding a `Geometry` slice to a max viewing distance.
#[derive(Debug, Clone, Copy)]
pub struct LodLevel<'a> {
    pub max_distance: f32,
    pub geometry: Geometry<'a>,
}

impl<'a> LodLevel<'a> {
    pub const fn new(max_distance: f32, geometry: Geometry<'a>) -> Self {
        Self {
            max_distance,
            geometry,
        }
    }
}

/// Fixed-capacity multi-LOD mesh selector for up to `MAX_LODS` detail tiers (default 4).
#[derive(Debug, Clone)]
pub struct MeshLodSelector<'a, const MAX_LODS: usize = 4> {
    levels: heapless::Vec<LodLevel<'a>, MAX_LODS>,
    fallback: Option<Geometry<'a>>,
}

impl<'a, const MAX_LODS: usize> MeshLodSelector<'a, MAX_LODS> {
    /// Creates a new empty `MeshLodSelector`.
    pub fn new() -> Self {
        Self {
            levels: heapless::Vec::new(),
            fallback: None,
        }
    }

    /// Inserts an `LodLevel` sorted by `max_distance` ascending.
    ///
    /// Returns `Err(())` if `MAX_LODS` capacity is exceeded.
    pub fn add_level(&mut self, level: LodLevel<'a>) -> Result<(), ()> {
        if self.levels.is_full() {
            return Err(());
        }
        let pos = self
            .levels
            .iter()
            .position(|l| level.max_distance < l.max_distance)
            .unwrap_or(self.levels.len());
        self.levels.insert(pos, level).map_err(|_| ())
    }

    /// Sets the fallback `Geometry` returned when distance exceeds all levels.
    pub fn with_fallback(mut self, fallback: Geometry<'a>) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Returns the active levels.
    pub fn levels(&self) -> &[LodLevel<'a>] {
        &self.levels
    }

    /// Selects the appropriate `Geometry` based on distance between camera and mesh position.
    pub fn select_lod(
        &self,
        camera_pos: Point3<f32>,
        mesh_pos: Point3<f32>,
    ) -> Option<&Geometry<'a>> {
        let dist = (camera_pos - mesh_pos).norm();
        self.select_lod_by_dist(dist)
    }

    /// Selects the appropriate `Geometry` based on squared distance.
    pub fn select_lod_sq(&self, dist_sq: f32) -> Option<&Geometry<'a>> {
        for level in &self.levels {
            let max_sq = level.max_distance * level.max_distance;
            if dist_sq <= max_sq {
                return Some(&level.geometry);
            }
        }
        if let Some(ref fallback) = self.fallback {
            Some(fallback)
        } else {
            self.levels.last().map(|l| &l.geometry)
        }
    }

    /// Selects the appropriate `Geometry` based on linear distance.
    pub fn select_lod_by_dist(&self, dist: f32) -> Option<&Geometry<'a>> {
        for level in &self.levels {
            if dist <= level.max_distance {
                return Some(&level.geometry);
            }
        }
        if let Some(ref fallback) = self.fallback {
            Some(fallback)
        } else {
            self.levels.last().map(|l| &l.geometry)
        }
    }
}

impl<'a, const MAX_LODS: usize> Default for MeshLodSelector<'a, MAX_LODS> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lod_selection_by_distance() {
        let v1 = [[0.0, 0.0, 0.0]];
        let v2 = [[1.0, 1.0, 1.0]];
        let v_fb = [[2.0, 2.0, 2.0]];

        let geom1 = Geometry {
            vertices: &v1,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let geom2 = Geometry {
            vertices: &v2,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let geom_fb = Geometry {
            vertices: &v_fb,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut selector = MeshLodSelector::<4>::new();
        selector.add_level(LodLevel::new(10.0, geom1)).unwrap();
        selector.add_level(LodLevel::new(20.0, geom2)).unwrap();

        let camera_pos = Point3::new(0.0, 0.0, 0.0);

        // Close camera: distance 5.0 -> Level 1 (geom1)
        let selected = selector.select_lod(camera_pos, Point3::new(0.0, 0.0, 5.0));
        assert_eq!(selected.unwrap().vertices[0], [0.0, 0.0, 0.0]);

        // Mid distance camera: distance 15.0 -> Level 2 (geom2)
        let selected = selector.select_lod(camera_pos, Point3::new(0.0, 0.0, 15.0));
        assert_eq!(selected.unwrap().vertices[0], [1.0, 1.0, 1.0]);

        // Far camera: distance 30.0 -> highest distance level (geom2) since no fallback set
        let selected = selector.select_lod(camera_pos, Point3::new(0.0, 0.0, 30.0));
        assert_eq!(selected.unwrap().vertices[0], [1.0, 1.0, 1.0]);

        // With fallback set: distance 30.0 -> Fallback (geom_fb)
        let selector = selector.with_fallback(geom_fb);
        let selected = selector.select_lod(camera_pos, Point3::new(0.0, 0.0, 30.0));
        assert_eq!(selected.unwrap().vertices[0], [2.0, 2.0, 2.0]);

        // Test select_lod_sq
        let selected_sq = selector.select_lod_sq(25.0); // dist_sq = 25.0 (dist = 5.0) -> geom1
        assert_eq!(selected_sq.unwrap().vertices[0], [0.0, 0.0, 0.0]);
    }
}
