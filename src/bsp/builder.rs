//! BSP content builder helpers (std-only authoring path).
//!
//! This module provides a small, deterministic pipeline helper for generating
//! valid [`BspWorld`](crate::bsp::data::BspWorld) data from high-level room
//! descriptions. It is intended for offline/tooling and examples, not hot-path
//! runtime generation on constrained devices.

use std::ops::Range;
use std::vec::Vec;

use super::data::{BspWorld, Face, Leaf, Node, Plane};

/// Axis-aligned room descriptor used by [`build_room_strip`].
#[derive(Debug, Clone, Copy)]
pub struct RoomSpec {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub floor_texture_id: u32,
    pub ceiling_texture_id: u32,
    /// `0xFFFF` means no baked lightmap.
    pub lightmap_id: u16,
}

/// Build errors for BSP helper generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    NeedAtLeastOneRoom,
    InvalidBounds,
    RoomsNotSortedOrOverlappingX,
    TooManyRooms,
    TooManyNodes,
    TooManyFaces,
    TooManyMarksurfaces,
}

/// Owned BSP lump storage that can be borrowed as a [`BspWorld`].
#[derive(Debug, Default)]
pub struct OwnedBspWorld {
    pub planes: Vec<Plane>,
    pub nodes: Vec<Node>,
    pub leaves: Vec<Leaf>,
    pub faces: Vec<Face>,
    pub marksurfaces: Vec<u16>,
    pub vertices: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub lm_uvs: Vec<[f32; 2]>,
    pub vis: Vec<u8>,
    pub vis_offsets: Vec<u32>,
    pub num_clusters: u16,
}

impl OwnedBspWorld {
    /// Borrow owned lumps as a `BspWorld`.
    pub fn as_world(&self) -> BspWorld<'_> {
        BspWorld::new(
            &self.planes,
            &self.nodes,
            &self.leaves,
            &self.faces,
            &self.marksurfaces,
            &self.vertices,
            &self.uvs,
            &self.lm_uvs,
            &self.vis,
            &self.vis_offsets,
            self.num_clusters,
        )
    }
}

#[inline]
fn to_i16_clamped(v: f32) -> i16 {
    v.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn range_bounds_i16(rooms: &[RoomSpec], r: Range<usize>) -> ([i16; 3], [i16; 3]) {
    let mut mins = [f32::INFINITY; 3];
    let mut maxs = [f32::NEG_INFINITY; 3];
    for room in &rooms[r] {
        for a in 0..3 {
            mins[a] = mins[a].min(room.mins[a]);
            maxs[a] = maxs[a].max(room.maxs[a]);
        }
    }
    (
        [
            to_i16_clamped(mins[0]),
            to_i16_clamped(mins[1]),
            to_i16_clamped(mins[2]),
        ],
        [
            to_i16_clamped(maxs[0]),
            to_i16_clamped(maxs[1]),
            to_i16_clamped(maxs[2]),
        ],
    )
}

fn build_node_recursive(
    rooms: &[RoomSpec],
    range: Range<usize>,
    planes: &mut Vec<Plane>,
    nodes: &mut Vec<Node>,
) -> Result<i32, BuildError> {
    if range.end - range.start == 1 {
        return Ok(!(range.start as i32));
    }

    let mid = (range.start + range.end) / 2;
    let split_dist = (rooms[mid - 1].maxs[0] + rooms[mid].mins[0]) * 0.5;

    let plane_index = planes.len();
    if plane_index > u16::MAX as usize {
        return Err(BuildError::TooManyNodes);
    }
    planes.push(Plane {
        normal: [1.0, 0.0, 0.0],
        dist: split_dist,
    });

    let node_index = nodes.len();
    if node_index > i32::MAX as usize {
        return Err(BuildError::TooManyNodes);
    }
    nodes.push(Node {
        plane: plane_index as u16,
        children: [0, 0],
        mins: [0, 0, 0],
        maxs: [0, 0, 0],
        first_face: 0,
        num_faces: 0,
    });

    // For +X normal, front child is the higher-X partition.
    let front = build_node_recursive(rooms, mid..range.end, planes, nodes)?;
    let back = build_node_recursive(rooms, range.start..mid, planes, nodes)?;

    let (mins, maxs) = range_bounds_i16(rooms, range);
    let n = &mut nodes[node_index];
    n.children = [front, back];
    n.mins = mins;
    n.maxs = maxs;

    Ok(node_index as i32)
}

/// Build a simple BSP world from an X-sorted strip of axis-aligned rooms.
///
/// Generated geometry includes floor + ceiling quads per room (textured), full
/// visibility PVS, and an X-axis BSP split tree with root at node 0.
pub fn build_room_strip(rooms: &[RoomSpec]) -> Result<OwnedBspWorld, BuildError> {
    if rooms.is_empty() {
        return Err(BuildError::NeedAtLeastOneRoom);
    }
    if rooms.len() > i16::MAX as usize {
        return Err(BuildError::TooManyRooms);
    }

    for room in rooms {
        if room.mins[0] >= room.maxs[0]
            || room.mins[1] >= room.maxs[1]
            || room.mins[2] >= room.maxs[2]
        {
            return Err(BuildError::InvalidBounds);
        }
    }

    for i in 1..rooms.len() {
        if rooms[i - 1].maxs[0] > rooms[i].mins[0] {
            return Err(BuildError::RoomsNotSortedOrOverlappingX);
        }
    }

    let mut out = OwnedBspWorld::default();

    for (room_idx, room) in rooms.iter().enumerate() {
        let first_marksurface = out.marksurfaces.len();
        if first_marksurface > u16::MAX as usize {
            return Err(BuildError::TooManyMarksurfaces);
        }

        let floor_start = out.vertices.len() as u32;
        out.vertices.extend_from_slice(&[
            [room.mins[0], room.mins[1], room.mins[2]],
            [room.maxs[0], room.mins[1], room.mins[2]],
            [room.maxs[0], room.mins[1], room.maxs[2]],
            [room.mins[0], room.mins[1], room.maxs[2]],
        ]);
        out.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        out.lm_uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);

        let ceil_start = out.vertices.len() as u32;
        out.vertices.extend_from_slice(&[
            [room.mins[0], room.maxs[1], room.mins[2]],
            [room.maxs[0], room.maxs[1], room.mins[2]],
            [room.maxs[0], room.maxs[1], room.maxs[2]],
            [room.mins[0], room.maxs[1], room.maxs[2]],
        ]);
        out.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        out.lm_uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);

        let floor_face = out.faces.len();
        let ceil_face = floor_face + 1;
        if ceil_face > u16::MAX as usize {
            return Err(BuildError::TooManyFaces);
        }

        out.faces.push(Face {
            first_vert: floor_start,
            num_verts: 4,
            texture_id: room.floor_texture_id,
            lightmap_id: room.lightmap_id,
            plane: 0,
            side: 0,
        });
        out.faces.push(Face {
            first_vert: ceil_start,
            num_verts: 4,
            texture_id: room.ceiling_texture_id,
            lightmap_id: room.lightmap_id,
            plane: 0,
            side: 0,
        });

        out.marksurfaces.push(floor_face as u16);
        out.marksurfaces.push(ceil_face as u16);

        out.leaves.push(Leaf {
            cluster: room_idx as i16,
            mins: [
                to_i16_clamped(room.mins[0]),
                to_i16_clamped(room.mins[1]),
                to_i16_clamped(room.mins[2]),
            ],
            maxs: [
                to_i16_clamped(room.maxs[0]),
                to_i16_clamped(room.maxs[1]),
                to_i16_clamped(room.maxs[2]),
            ],
            first_marksurface: first_marksurface as u16,
            num_marksurfaces: 2,
        });
    }

    // Build node tree. Root will be node 0 when `rooms.len() > 1`.
    if rooms.len() > 1 {
        let _root = build_node_recursive(rooms, 0..rooms.len(), &mut out.planes, &mut out.nodes)?;
    }

    // Full visibility PVS.
    out.num_clusters = rooms.len() as u16;
    let clusters = out.num_clusters as usize;
    let row_bytes = clusters.div_ceil(8);
    out.vis_offsets.reserve(clusters);
    out.vis.reserve(clusters * row_bytes);
    for cluster in 0..clusters {
        out.vis_offsets.push((cluster * row_bytes) as u32);
        for byte_idx in 0..row_bytes {
            let mut byte = 0u8;
            for bit in 0..8 {
                let target = byte_idx * 8 + bit;
                if target < clusters {
                    byte |= 1 << bit;
                }
            }
            out.vis.push(byte);
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::K3dengine;
    use crate::bsp::scratch::BspScratch;
    use crate::command_buffer::{CommandBuffer, RenderCommand};
    use nalgebra::Point3;

    #[test]
    fn build_room_strip_basic_world_is_traversable() {
        let rooms = [
            RoomSpec {
                mins: [-5.0, -2.0, -3.0],
                maxs: [0.0, 2.0, 3.0],
                floor_texture_id: 0,
                ceiling_texture_id: 1,
                lightmap_id: 0xFFFF,
            },
            RoomSpec {
                mins: [0.0, -2.0, -3.0],
                maxs: [5.0, 2.0, 3.0],
                floor_texture_id: 0,
                ceiling_texture_id: 1,
                lightmap_id: 0xFFFF,
            },
        ];
        let owned = build_room_strip(&rooms).expect("builder should succeed");
        assert_eq!(owned.nodes.len(), 1);
        assert_eq!(owned.leaves.len(), 2);
        assert_eq!(owned.faces.len(), 4);
        assert_eq!(owned.marksurfaces.len(), 4);

        let world = owned.as_world();
        assert_eq!(world.leaf_for_point([-2.0, 0.0, 0.0]), 0);
        assert_eq!(world.leaf_for_point([2.0, 0.0, 0.0]), 1);

        let mut visframe = [0u32; 8];
        let mut scratch = BspScratch::new(&mut visframe);
        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(-2.0, 0.0, 0.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));
        let mut commands: CommandBuffer<512> = CommandBuffer::new();

        engine
            .record_bsp(&world, &mut scratch, &mut commands, None)
            .expect("record_bsp should succeed");
        let draw_count = commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::Draw(_)))
            .count();
        assert!(draw_count > 0);
    }

    #[test]
    fn build_room_strip_rejects_unsorted_or_overlapping() {
        let rooms = [
            RoomSpec {
                mins: [0.0, -2.0, -3.0],
                maxs: [5.0, 2.0, 3.0],
                floor_texture_id: 0,
                ceiling_texture_id: 0,
                lightmap_id: 0xFFFF,
            },
            RoomSpec {
                mins: [-1.0, -2.0, -3.0],
                maxs: [2.0, 2.0, 3.0],
                floor_texture_id: 0,
                ceiling_texture_id: 0,
                lightmap_id: 0xFFFF,
            },
        ];
        let err = build_room_strip(&rooms).expect_err("expected ordering failure");
        assert_eq!(err, BuildError::RoomsNotSortedOrOverlappingX);
    }

    #[test]
    fn build_room_strip_single_room_is_valid() {
        let rooms = [RoomSpec {
            mins: [-5.0, -2.0, -3.0],
            maxs: [0.0, 2.0, 3.0],
            floor_texture_id: 0,
            ceiling_texture_id: 1,
            lightmap_id: 0xFFFF,
        }];
        let owned = build_room_strip(&rooms).expect("single room should build");
        assert!(owned.nodes.is_empty());
        assert_eq!(owned.leaves.len(), 1);
        assert_eq!(owned.faces.len(), 2);
    }
}
