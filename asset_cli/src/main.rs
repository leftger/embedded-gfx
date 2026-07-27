use std::fs;
use std::io::{self, Write};
use std::path::Path;

// ---------------------------------------------------------------------------
// Quake 1 BSP (version 29) importer
// ---------------------------------------------------------------------------

const BSP_VERSION: u32 = 29;
const LUMP_PLANES: usize = 1;
const LUMP_VERTICES: usize = 3;
const LUMP_VIS: usize = 4;
const LUMP_NODES: usize = 5;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_LIGHTING: usize = 8;
const LUMP_LEAVES: usize = 10;
const LUMP_MARKSURFACES: usize = 11;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;

fn read_i16_le(data: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([data[offset], data[offset + 1]])
}
fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}
fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
fn read_f32_le(data: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

struct LumpDir {
    offset: usize,
    length: usize,
}

fn parse_lump_dirs(data: &[u8]) -> io::Result<Vec<LumpDir>> {
    if data.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "bsp too small",
        ));
    }
    let ver = read_u32_le(data, 0);
    if ver != BSP_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported BSP version {ver}, expected {BSP_VERSION}"),
        ));
    }
    let mut lumps = Vec::with_capacity(15);
    for i in 0..15usize {
        let base = 4 + i * 8;
        if base + 8 > data.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "lump dir truncated",
            ));
        }
        lumps.push(LumpDir {
            offset: read_u32_le(data, base) as usize,
            length: read_u32_le(data, base + 4) as usize,
        });
    }
    Ok(lumps)
}

fn lump_slice<'a>(data: &'a [u8], lumps: &[LumpDir], idx: usize) -> io::Result<&'a [u8]> {
    let l = &lumps[idx];
    if l.offset + l.length > data.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("lump {idx} out of range"),
        ));
    }
    Ok(&data[l.offset..l.offset + l.length])
}

// Quake BSP plane: normal[3] f32, dist f32, type i32 (20 bytes)
struct QPlane {
    normal: [f32; 3],
    dist: f32,
}
fn parse_planes(raw: &[u8]) -> Vec<QPlane> {
    raw.chunks_exact(20)
        .map(|c| QPlane {
            normal: [read_f32_le(c, 0), read_f32_le(c, 4), read_f32_le(c, 8)],
            dist: read_f32_le(c, 12),
        })
        .collect()
}

// Quake BSP node: planenum i32, children[2] i16, mins/maxs[6] i16, firstface u16, numfaces u16 (24 bytes)
struct QNode {
    plane: u32,
    children: [i16; 2],
    mins: [i16; 3],
    maxs: [i16; 3],
    first_face: u16,
    num_faces: u16,
}
fn parse_nodes(raw: &[u8]) -> Vec<QNode> {
    raw.chunks_exact(24)
        .map(|c| QNode {
            plane: read_u32_le(c, 0),
            children: [read_i16_le(c, 4), read_i16_le(c, 6)],
            mins: [read_i16_le(c, 8), read_i16_le(c, 10), read_i16_le(c, 12)],
            maxs: [read_i16_le(c, 14), read_i16_le(c, 16), read_i16_le(c, 18)],
            first_face: read_u16_le(c, 20),
            num_faces: read_u16_le(c, 22),
        })
        .collect()
}

// Quake BSP leaf: contents i32, visofs i32, mins/maxs[6] i16, firstmark u16, nummark u16, ambient[4] u8 (28 bytes)
struct QLeaf {
    contents: i32,
    visofs: i32,
    mins: [i16; 3],
    maxs: [i16; 3],
    first_mark: u16,
    num_mark: u16,
}
fn parse_leaves(raw: &[u8]) -> Vec<QLeaf> {
    raw.chunks_exact(28)
        .map(|c| QLeaf {
            contents: read_i32_le(c, 0),
            visofs: read_i32_le(c, 4),
            mins: [read_i16_le(c, 8), read_i16_le(c, 10), read_i16_le(c, 12)],
            maxs: [read_i16_le(c, 14), read_i16_le(c, 16), read_i16_le(c, 18)],
            first_mark: read_u16_le(c, 20),
            num_mark: read_u16_le(c, 22),
        })
        .collect()
}

// Quake BSP texinfo: vecs[2][4] f32, miptex i32, flags i32 (40 bytes)
struct QTexinfo {
    vecs: [[f32; 4]; 2],
    miptex: i32,
}
fn parse_texinfos(raw: &[u8]) -> Vec<QTexinfo> {
    raw.chunks_exact(40)
        .map(|c| QTexinfo {
            vecs: [
                [
                    read_f32_le(c, 0),
                    read_f32_le(c, 4),
                    read_f32_le(c, 8),
                    read_f32_le(c, 12),
                ],
                [
                    read_f32_le(c, 16),
                    read_f32_le(c, 20),
                    read_f32_le(c, 24),
                    read_f32_le(c, 28),
                ],
            ],
            miptex: read_i32_le(c, 32),
        })
        .collect()
}

// Quake BSP face: planenum u16, side i16, firstedge i32, numedges i16, texinfo u16, styles[4] u8, lightofs i32 (20 bytes)
struct QFace {
    planenum: u16,
    side: i16,
    firstedge: i32,
    numedges: i16,
    texinfo: u16,
    lightofs: i32,
}
fn parse_faces(raw: &[u8]) -> Vec<QFace> {
    raw.chunks_exact(20)
        .map(|c| QFace {
            planenum: read_u16_le(c, 0),
            side: read_i16_le(c, 2),
            firstedge: read_i32_le(c, 4),
            numedges: read_i16_le(c, 8),
            texinfo: read_u16_le(c, 10),
            // styles[4] at offset 12
            lightofs: read_i32_le(c, 16),
        })
        .collect()
}

fn parse_vertices(raw: &[u8]) -> Vec<[f32; 3]> {
    raw.chunks_exact(12)
        .map(|c| [read_f32_le(c, 0), read_f32_le(c, 4), read_f32_le(c, 8)])
        .collect()
}
fn parse_edges(raw: &[u8]) -> Vec<[u16; 2]> {
    raw.chunks_exact(4)
        .map(|c| [read_u16_le(c, 0), read_u16_le(c, 2)])
        .collect()
}
fn parse_surfedges(raw: &[u8]) -> Vec<i32> {
    raw.chunks_exact(4).map(|c| read_i32_le(c, 0)).collect()
}
fn parse_marksurfaces(raw: &[u8]) -> Vec<u16> {
    raw.chunks_exact(2).map(|c| read_u16_le(c, 0)).collect()
}

/// Get the first vertex of a surfedge.
fn surfedge_vert(se: i32, edges: &[[u16; 2]]) -> usize {
    if se >= 0 {
        edges[se as usize][0] as usize
    } else {
        edges[(-se) as usize][1] as usize
    }
}

/// Encode a Quake int16 child as our i32 child (Quake uses int16; negative values
/// indicate leaves via ~x, positive are node indices).
fn quake_child_to_i32(c: i16) -> i32 {
    if c < 0 {
        // Leaf: Quake encodes leaf n as -(n+1), i.e., child = ~n where n is 0-based
        // Our convention: leaf index = !child (one's complement)
        // Quake child = -(leaf_index + 1) = !leaf_index in two's complement for i16
        // We need our i32 to satisfy: !child as usize = leaf_index
        // Quake: child_i16 = -(n+1) → n = -child_i16 - 1
        let leaf_idx = (-c as i32) - 1;
        !leaf_idx
    } else {
        c as i32 // node index
    }
}

// Next-power-of-two for textures
fn next_pow2(x: usize) -> usize {
    if x == 0 {
        return 1;
    }
    let mut p = 1usize;
    while p < x {
        p <<= 1;
    }
    p
}

// Convert byte lightmap value (0-255) to a grayscale RGB565 value
fn lm_byte_to_rgb565_pair(v: u8) -> [u8; 2] {
    let r5 = ((v as u16 * 31) / 255) as u8;
    let g6 = ((v as u16 * 63) / 255) as u8;
    let packed: u16 = ((r5 as u16) << 11) | ((g6 as u16) << 5) | (r5 as u16);
    packed.to_le_bytes()
}

// Lightmap for one face
struct FaceLightmap {
    width: usize, // texels
    height: usize,
    pixels: Vec<[u8; 2]>, // RGB565 little-endian pairs
    s_min: f32,
    t_min: f32,
}

fn compute_face_lightmap(
    face: &QFace,
    q_verts: &[[f32; 3]],
    surfedges: &[i32],
    edges: &[[u16; 2]],
    texinfo: &QTexinfo,
    lighting: &[u8],
) -> Option<FaceLightmap> {
    if face.lightofs < 0 {
        return None;
    }

    let mut s_coords: Vec<f32> = Vec::new();
    let mut t_coords: Vec<f32> = Vec::new();
    for k in 0..face.numedges as usize {
        let se = surfedges[face.firstedge as usize + k];
        let vi = surfedge_vert(se, edges);
        let v = &q_verts[vi];
        let s = v[0] * texinfo.vecs[0][0]
            + v[1] * texinfo.vecs[0][1]
            + v[2] * texinfo.vecs[0][2]
            + texinfo.vecs[0][3];
        let t = v[0] * texinfo.vecs[1][0]
            + v[1] * texinfo.vecs[1][1]
            + v[2] * texinfo.vecs[1][2]
            + texinfo.vecs[1][3];
        s_coords.push(s);
        t_coords.push(t);
    }

    let s_min_raw = s_coords.iter().cloned().fold(f32::MAX, f32::min);
    let t_min_raw = t_coords.iter().cloned().fold(f32::MAX, f32::min);
    let s_max_raw = s_coords.iter().cloned().fold(f32::MIN, f32::max);
    let t_max_raw = t_coords.iter().cloned().fold(f32::MIN, f32::max);

    let s_min = (s_min_raw / 16.0).floor();
    let s_max = (s_max_raw / 16.0).ceil();
    let t_min = (t_min_raw / 16.0).floor();
    let t_max = (t_max_raw / 16.0).ceil();

    let lm_w = ((s_max - s_min) as usize) + 1;
    let lm_h = ((t_max - t_min) as usize) + 1;
    let n_texels = lm_w * lm_h;

    let lo = face.lightofs as usize;
    if lo + n_texels > lighting.len() {
        return None;
    }

    let pixels: Vec<[u8; 2]> = lighting[lo..lo + n_texels]
        .iter()
        .map(|&b| lm_byte_to_rgb565_pair(b))
        .collect();

    Some(FaceLightmap {
        width: lm_w,
        height: lm_h,
        pixels,
        s_min: s_min * 16.0,
        t_min: t_min * 16.0,
    })
}

/// Simple shelf-based lightmap atlas packer.
struct LightmapAtlas {
    width: usize,
    height: usize,
    pixels: Vec<u16>, // RGB565 as u16
    shelf_x: usize,
    shelf_y: usize,
    shelf_h: usize,
}

impl LightmapAtlas {
    fn new(size: usize) -> Self {
        Self {
            width: size,
            height: size,
            pixels: vec![0u16; size * size],
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
        }
    }

    /// Pack a lightmap into the atlas. Returns (atlas_x, atlas_y) or None if full.
    fn pack(&mut self, lm: &FaceLightmap) -> Option<(usize, usize)> {
        // Start a new shelf if this doesn't fit horizontally
        if self.shelf_x + lm.width > self.width {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + lm.height > self.height {
            return None;
        }

        let ax = self.shelf_x;
        let ay = self.shelf_y;

        for ty in 0..lm.height {
            for tx in 0..lm.width {
                let pair = lm.pixels[ty * lm.width + tx];
                let val = u16::from_le_bytes(pair);
                self.pixels[(ay + ty) * self.width + (ax + tx)] = val;
            }
        }

        self.shelf_x += lm.width;
        if lm.height > self.shelf_h {
            self.shelf_h = lm.height;
        }
        Some((ax, ay))
    }

    fn into_rgb565_bytes(self) -> Vec<u8> {
        self.pixels.iter().flat_map(|&p| p.to_le_bytes()).collect()
    }
}

pub fn import_bsp(bsp_path: &Path, out_path: &Path, with_lightmaps: bool) -> io::Result<()> {
    let data = fs::read(bsp_path)?;
    let lumps = parse_lump_dirs(&data)?;

    let q_planes = parse_planes(lump_slice(&data, &lumps, LUMP_PLANES)?);
    let q_vertices = parse_vertices(lump_slice(&data, &lumps, LUMP_VERTICES)?);
    let vis_raw = lump_slice(&data, &lumps, LUMP_VIS)?;
    let q_nodes = parse_nodes(lump_slice(&data, &lumps, LUMP_NODES)?);
    let q_texinfos = parse_texinfos(lump_slice(&data, &lumps, LUMP_TEXINFO)?);
    let q_faces = parse_faces(lump_slice(&data, &lumps, LUMP_FACES)?);
    let lighting_raw = lump_slice(&data, &lumps, LUMP_LIGHTING)?;
    let q_leaves = parse_leaves(lump_slice(&data, &lumps, LUMP_LEAVES)?);
    let q_marksurfs = parse_marksurfaces(lump_slice(&data, &lumps, LUMP_MARKSURFACES)?);
    let q_edges = parse_edges(lump_slice(&data, &lumps, LUMP_EDGES)?);
    let q_surfedges = parse_surfedges(lump_slice(&data, &lumps, LUMP_SURFEDGES)?);

    // ----- Build output vertices, UVs, and faces -----
    // For each Quake face, we build a vertex fan and append to flat arrays.
    let mut out_vertices: Vec<[f32; 3]> = Vec::new();
    let mut out_uvs: Vec<[f32; 2]> = Vec::new();
    let mut out_lm_uvs: Vec<[f32; 2]> = Vec::new();
    let mut out_faces: Vec<(u32, u16, u16, u8, u16, u16)> = Vec::new();
    // (first_vert, num_verts, plane, side, texture_id, lightmap_id)

    // Use texinfo miptex index as texture_id (0xFFFF if invalid)
    // Lightmap atlas
    let atlas_size = if with_lightmaps { 512usize } else { 1 };
    let mut atlas = LightmapAtlas::new(next_pow2(atlas_size));
    let mut lm_texture_id: Option<u32> = None; // will be 1 (surface texture is 0)

    // Unique surface textures: map miptex_idx → texture_id
    let mut tex_id_map: std::collections::HashMap<i32, u32> = std::collections::HashMap::new();
    let mut next_tex_id = 0u32;

    for face in &q_faces {
        let te_idx = face.texinfo as usize;
        let texinfo = if te_idx < q_texinfos.len() {
            &q_texinfos[te_idx]
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "texinfo out of range",
            ));
        };

        // Surface texture ID (unique per miptex index)
        let tex_id = *tex_id_map.entry(texinfo.miptex).or_insert_with(|| {
            let id = next_tex_id;
            next_tex_id += 1;
            id
        });

        let first_vert = out_vertices.len() as u32;
        let n = face.numedges as usize;
        if n < 3 {
            continue;
        }

        // Build surface UVs — we need texel-space UV normalised to [0,1]
        // We don't have texture dimensions here, so use raw texel coords and
        // normalise by an assumed 64×64 (users can adjust by scaling UVs).
        // A more complete implementation would embed texture sizes from the miptex lump.
        let assumed_tex_size = 64.0f32;

        // Compute lightmap for this face
        let lm_data = if with_lightmaps {
            compute_face_lightmap(
                face,
                &q_vertices,
                &q_surfedges,
                &q_edges,
                texinfo,
                lighting_raw,
            )
        } else {
            None
        };

        let lm_atlas_pos = if let Some(ref lm) = lm_data {
            atlas.pack(lm)
        } else {
            None
        };

        for k in 0..n {
            let se = q_surfedges[(face.firstedge as usize) + k];
            let vi = surfedge_vert(se, &q_edges);
            let v = q_vertices[vi];
            let s = v[0] * texinfo.vecs[0][0]
                + v[1] * texinfo.vecs[0][1]
                + v[2] * texinfo.vecs[0][2]
                + texinfo.vecs[0][3];
            let t = v[0] * texinfo.vecs[1][0]
                + v[1] * texinfo.vecs[1][1]
                + v[2] * texinfo.vecs[1][2]
                + texinfo.vecs[1][3];
            out_vertices.push(v);
            out_uvs.push([s / assumed_tex_size, t / assumed_tex_size]);

            // Lightmap UV
            let lm_uv = if let (Some(lm), Some((ax, ay))) = (&lm_data, lm_atlas_pos) {
                let lu = (s - lm.s_min) / (lm.width as f32 * 16.0);
                let lv = (t - lm.t_min) / (lm.height as f32 * 16.0);
                // Offset into atlas
                let au = (ax as f32 + lu * lm.width as f32) / atlas.width as f32;
                let av = (ay as f32 + lv * lm.height as f32) / atlas.height as f32;
                [au, av]
            } else {
                [0.0f32, 0.0]
            };
            out_lm_uvs.push(lm_uv);
        }

        let lightmap_id: u16 = if lm_atlas_pos.is_some() {
            if lm_texture_id.is_none() {
                lm_texture_id = Some(next_tex_id);
                next_tex_id += 1;
            }
            lm_texture_id.unwrap() as u16
        } else {
            0xFFFF
        };

        out_faces.push((
            first_vert,
            n as u16,
            face.planenum,
            face.side as u8,
            tex_id as u16,
            lightmap_id,
        ));
    }

    // ----- Marksurfaces: indices into q_faces which map 1:1 to out_faces -----
    let out_marksurfs: Vec<u16> = q_marksurfs.iter().cloned().collect();

    // ----- Nodes -----
    // q_nodes: children are i16 (negative = leaf via -(n+1))
    // Quake leaf 0 is the outside solid leaf; actual content leaves start at 1.
    // We keep indices as-is and translate children.
    let out_nodes: Vec<(u16, i32, i32, i16, i16, i16, i16, i16, i16, u16, u16)> = q_nodes
        .iter()
        .map(|n| {
            (
                n.plane as u16,
                quake_child_to_i32(n.children[0]),
                quake_child_to_i32(n.children[1]),
                n.mins[0],
                n.mins[1],
                n.mins[2],
                n.maxs[0],
                n.maxs[1],
                n.maxs[2],
                n.first_face,
                n.num_faces,
            )
        })
        .collect();

    // ----- Leaves -----
    // cluster = leaf_index (Quake 1: one cluster per leaf, leaf 0 = solid)
    // We map leaf_index directly. vis_offsets[leaf_i] = leaf.visofs (or u32::MAX if -1).
    let mut vis_offsets: Vec<u32> = Vec::with_capacity(q_leaves.len());
    let out_leaves: Vec<(i16, i16, i16, i16, i16, i16, i16, u16, u16)> = q_leaves
        .iter()
        .enumerate()
        .map(|(li, l)| {
            let cluster = if l.contents < 0 && l.visofs >= 0 {
                li as i16
            } else {
                -1i16
            };
            vis_offsets.push(if l.visofs >= 0 {
                l.visofs as u32
            } else {
                u32::MAX
            });
            (
                cluster,
                l.mins[0],
                l.mins[1],
                l.mins[2],
                l.maxs[0],
                l.maxs[1],
                l.maxs[2],
                l.first_mark,
                l.num_mark,
            )
        })
        .collect();

    let num_clusters = q_leaves.len() as u16;

    // ----- Write Rust source -----
    let mut out = String::new();
    out.push_str("// Auto-generated by asset_cli import-bsp — DO NOT EDIT\n");
    out.push_str("// Source: ");
    out.push_str(&bsp_path.to_string_lossy());
    out.push('\n');
    out.push_str("use embedded_3dgfx::bsp::data::*;\n\n");

    // Planes
    out.push_str(&format!(
        "pub static BSP_PLANES: [Plane; {}] = [\n",
        q_planes.len()
    ));
    for p in &q_planes {
        out.push_str(&format!(
            "    Plane {{ normal: [{:.6}, {:.6}, {:.6}], dist: {:.6} }},\n",
            p.normal[0], p.normal[1], p.normal[2], p.dist
        ));
    }
    out.push_str("];\n\n");

    // Nodes
    out.push_str(&format!(
        "pub static BSP_NODES: [Node; {}] = [\n",
        out_nodes.len()
    ));
    for (plane, c0, c1, mn0, mn1, mn2, mx0, mx1, mx2, ff, nf) in &out_nodes {
        out.push_str(&format!(
            "    Node {{ plane: {plane}, children: [{c0}, {c1}], mins: [{mn0}, {mn1}, {mn2}], maxs: [{mx0}, {mx1}, {mx2}], first_face: {ff}, num_faces: {nf} }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Leaves
    out.push_str(&format!(
        "pub static BSP_LEAVES: [Leaf; {}] = [\n",
        out_leaves.len()
    ));
    for (cl, mn0, mn1, mn2, mx0, mx1, mx2, fm, nm) in &out_leaves {
        out.push_str(&format!(
            "    Leaf {{ cluster: {cl}, mins: [{mn0}, {mn1}, {mn2}], maxs: [{mx0}, {mx1}, {mx2}], first_marksurface: {fm}, num_marksurfaces: {nm} }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Faces
    out.push_str(&format!(
        "pub static BSP_FACES: [Face; {}] = [\n",
        out_faces.len()
    ));
    for (fv, nv, plane, side, tex, lm) in &out_faces {
        out.push_str(&format!(
            "    Face {{ first_vert: {fv}, num_verts: {nv}, texture_id: {tex}, lightmap_id: {lm}, plane: {plane}, side: {side} }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Marksurfaces
    out.push_str(&format!(
        "pub static BSP_MARKSURFACES: [u16; {}] = [",
        out_marksurfs.len()
    ));
    for (i, ms) in out_marksurfs.iter().enumerate() {
        if i % 16 == 0 {
            out.push_str("\n    ");
        }
        out.push_str(&format!("{ms}, "));
    }
    out.push_str("\n];\n\n");

    // Vertices
    out.push_str(&format!(
        "pub static BSP_VERTICES: [[f32; 3]; {}] = [\n",
        out_vertices.len()
    ));
    for v in &out_vertices {
        out.push_str(&format!("    [{:.6}, {:.6}, {:.6}],\n", v[0], v[1], v[2]));
    }
    out.push_str("];\n\n");

    // UVs
    out.push_str(&format!(
        "pub static BSP_UVS: [[f32; 2]; {}] = [\n",
        out_uvs.len()
    ));
    for uv in &out_uvs {
        out.push_str(&format!("    [{:.6}, {:.6}],\n", uv[0], uv[1]));
    }
    out.push_str("];\n\n");

    // Lightmap UVs
    if with_lightmaps {
        out.push_str(&format!(
            "pub static BSP_LM_UVS: [[f32; 2]; {}] = [\n",
            out_lm_uvs.len()
        ));
        for uv in &out_lm_uvs {
            out.push_str(&format!("    [{:.6}, {:.6}],\n", uv[0], uv[1]));
        }
        out.push_str("];\n\n");
    }

    // VIS blob
    out.push_str(&format!(
        "pub static BSP_VIS: [u8; {}] = [\n    ",
        vis_raw.len()
    ));
    for (i, b) in vis_raw.iter().enumerate() {
        out.push_str(&format!("0x{b:02X}, "));
        if i % 16 == 15 {
            out.push_str("\n    ");
        }
    }
    out.push_str("\n];\n\n");

    // VIS offsets
    out.push_str(&format!(
        "pub static BSP_VIS_OFFSETS: [u32; {}] = [",
        vis_offsets.len()
    ));
    for (i, v) in vis_offsets.iter().enumerate() {
        if i % 16 == 0 {
            out.push_str("\n    ");
        }
        out.push_str(&format!("{v}, "));
    }
    out.push_str("\n];\n\n");

    out.push_str(&format!(
        "pub const BSP_NUM_CLUSTERS: u16 = {num_clusters};\n\n"
    ));

    // Lightmap atlas texture data
    if with_lightmaps {
        let atlas_w = atlas.width;
        let atlas_h = atlas.height;
        let atlas_bytes = atlas.into_rgb565_bytes();
        out.push_str(&format!(
            "pub static BSP_LIGHTMAP_ATLAS: [u8; {}] = [\n    ",
            atlas_bytes.len()
        ));
        for (i, b) in atlas_bytes.iter().enumerate() {
            out.push_str(&format!("0x{b:02X}, "));
            if i % 16 == 15 {
                out.push_str("\n    ");
            }
        }
        out.push_str("\n];\n\n");
        out.push_str(&format!(
            "pub const BSP_LIGHTMAP_ATLAS_WIDTH: u32 = {atlas_w};\n"
        ));
        out.push_str(&format!(
            "pub const BSP_LIGHTMAP_ATLAS_HEIGHT: u32 = {atlas_h};\n\n"
        ));
    }

    // bsp_world() constructor
    let lm_uvs_ref = if with_lightmaps { "&BSP_LM_UVS" } else { "&[]" };
    out.push_str("pub fn bsp_world() -> BspWorld<'static> {\n");
    out.push_str("    BspWorld::new(\n");
    out.push_str("        &BSP_PLANES,\n        &BSP_NODES,\n        &BSP_LEAVES,\n");
    out.push_str("        &BSP_FACES,\n        &BSP_MARKSURFACES,\n        &BSP_VERTICES,\n");
    out.push_str("        &BSP_UVS,\n");
    out.push_str(&format!("        {lm_uvs_ref},\n"));
    out.push_str("        &BSP_VIS,\n        &BSP_VIS_OFFSETS,\n        BSP_NUM_CLUSTERS,\n");
    out.push_str("    )\n}\n");

    fs::write(out_path, out.as_bytes())?;
    eprintln!(
        "Wrote {}: {} planes, {} nodes, {} leaves, {} faces, {} vertices",
        out_path.display(),
        q_planes.len(),
        out_nodes.len(),
        out_leaves.len(),
        out_faces.len(),
        out_vertices.len()
    );
    Ok(())
}

const WAD_LUMP_VERTEXES: &str = "VERTEXES";
const WAD_LUMP_LINEDEFS: &str = "LINEDEFS";
const WAD_LUMP_SIDEDEFS: &str = "SIDEDEFS";
const WAD_LUMP_SECTORS: &str = "SECTORS";
const WAD_LUMP_SEGS: &str = "SEGS";
const WAD_LUMP_SSECTORS: &str = "SSECTORS";
const WAD_LUMP_NODES: &str = "NODES";

#[derive(Debug, Clone, Copy)]
struct WadDirEntry {
    offset: usize,
    size: usize,
    name: [u8; 8],
}

#[derive(Debug, Clone, Copy)]
struct WadVertex {
    x: i16,
    y: i16,
}

#[derive(Debug, Clone, Copy)]
struct WadLinedef {
    start_vertex: u16,
    end_vertex: u16,
    right_side: i16,
    left_side: i16,
}

#[derive(Debug, Clone, Copy)]
struct WadSidedef {
    sector: u16,
}

#[derive(Debug, Clone, Copy)]
struct WadSector {
    floor_height: i16,
    ceiling_height: i16,
    light: i16,
    sector_type: i16,
}

#[derive(Debug, Clone, Copy)]
struct WadSeg {
    start_vertex: u16,
    end_vertex: u16,
    linedef: u16,
    direction: i16,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct WadSsector {
    seg_count: u16,
    first_seg: u16,
}

fn wad_name_to_string(name: &[u8; 8]) -> String {
    let end = name.iter().position(|b| *b == 0).unwrap_or(8);
    String::from_utf8_lossy(&name[..end]).to_string()
}

fn parse_wad_dir(data: &[u8]) -> io::Result<Vec<WadDirEntry>> {
    if data.len() < 12 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "WAD header truncated",
        ));
    }
    let lump_count = read_u32_le(data, 4) as usize;
    let dir_offset = read_u32_le(data, 8) as usize;
    let dir_bytes = lump_count
        .checked_mul(16)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "directory overflow"))?;
    if dir_offset + dir_bytes > data.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "WAD directory out of range",
        ));
    }

    let mut out = Vec::with_capacity(lump_count);
    for i in 0..lump_count {
        let base = dir_offset + i * 16;
        let offset = read_u32_le(data, base) as usize;
        let size = read_u32_le(data, base + 4) as usize;
        let mut name = [0u8; 8];
        name.copy_from_slice(&data[base + 8..base + 16]);
        out.push(WadDirEntry { offset, size, name });
    }
    Ok(out)
}

fn wad_find_map_index(dir: &[WadDirEntry], map_name: Option<&str>) -> io::Result<usize> {
    let required = [
        WAD_LUMP_VERTEXES,
        WAD_LUMP_LINEDEFS,
        WAD_LUMP_SIDEDEFS,
        WAD_LUMP_SECTORS,
        WAD_LUMP_SEGS,
    ];

    if let Some(name) = map_name {
        for i in 0..dir.len() {
            if wad_name_to_string(&dir[i].name).eq_ignore_ascii_case(name) {
                return Ok(i);
            }
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "requested map was not found in WAD",
        ));
    }

    for i in 0..dir.len() {
        let has_required = required.iter().all(|needle| {
            dir.iter()
                .skip(i + 1)
                .take(14)
                .any(|entry| wad_name_to_string(&entry.name).eq_ignore_ascii_case(needle))
        });
        if has_required {
            return Ok(i);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "could not find a map marker with required lumps",
    ))
}

fn wad_find_lump_after<'a>(
    dir: &'a [WadDirEntry],
    map_index: usize,
    name: &str,
) -> Option<&'a WadDirEntry> {
    dir.iter()
        .skip(map_index + 1)
        .take(14)
        .find(|entry| wad_name_to_string(&entry.name).eq_ignore_ascii_case(name))
}

fn wad_lump_slice<'a>(data: &'a [u8], entry: &WadDirEntry) -> io::Result<&'a [u8]> {
    if entry.offset + entry.size > data.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "WAD lump out of bounds",
        ));
    }
    Ok(&data[entry.offset..entry.offset + entry.size])
}

fn parse_wad_vertices(raw: &[u8]) -> Vec<WadVertex> {
    raw.chunks_exact(4)
        .map(|c| WadVertex {
            x: read_i16_le(c, 0),
            y: read_i16_le(c, 2),
        })
        .collect()
}

fn parse_wad_linedefs(raw: &[u8]) -> Vec<WadLinedef> {
    raw.chunks_exact(14)
        .map(|c| WadLinedef {
            start_vertex: read_u16_le(c, 0),
            end_vertex: read_u16_le(c, 2),
            right_side: read_i16_le(c, 10),
            left_side: read_i16_le(c, 12),
        })
        .collect()
}

fn parse_wad_sidedefs(raw: &[u8]) -> Vec<WadSidedef> {
    raw.chunks_exact(30)
        .map(|c| WadSidedef {
            sector: read_u16_le(c, 28),
        })
        .collect()
}

fn parse_wad_sectors(raw: &[u8]) -> Vec<WadSector> {
    raw.chunks_exact(26)
        .map(|c| WadSector {
            floor_height: read_i16_le(c, 0),
            ceiling_height: read_i16_le(c, 2),
            light: read_i16_le(c, 20),
            sector_type: read_i16_le(c, 22),
        })
        .collect()
}

fn parse_wad_segs(raw: &[u8]) -> Vec<WadSeg> {
    raw.chunks_exact(12)
        .map(|c| WadSeg {
            start_vertex: read_u16_le(c, 0),
            end_vertex: read_u16_le(c, 2),
            linedef: read_u16_le(c, 6),
            direction: read_i16_le(c, 8),
        })
        .collect()
}

fn parse_wad_ssectors(raw: &[u8]) -> Vec<WadSsector> {
    raw.chunks_exact(4)
        .map(|c| WadSsector {
            seg_count: read_u16_le(c, 0),
            first_seg: read_u16_le(c, 2),
        })
        .collect()
}

fn clamp_u8(v: i16) -> u8 {
    v.clamp(0, 255) as u8
}

fn wad_sector_effect(sector_type: i16) -> Option<(&'static str, f32, f32)> {
    match sector_type {
        1 => Some(("Random", 20.0, 0.06)),
        17 => Some(("Random", 8.0, 0.5)),
        3 | 12 => Some(("Alternate", 1.0, 0.85)),
        2 | 4 | 13 => Some(("Alternate", 2.0, 0.7)),
        8 => Some(("Glow", 0.5, 0.0)),
        _ => None,
    }
}

fn approx_eq2(a: [f32; 2], b: [f32; 2], eps: f32) -> bool {
    (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps
}

fn canonicalize_polygon(points: &mut Vec<[f32; 2]>) {
    const EPS: f32 = 1e-4;
    if points.len() < 3 {
        return;
    }

    // Remove duplicate points.
    let mut unique: Vec<[f32; 2]> = Vec::with_capacity(points.len());
    for p in points.iter().copied() {
        if !unique.iter().any(|u| approx_eq2(*u, p, EPS)) {
            unique.push(p);
        }
    }
    if unique.len() < 3 {
        points.clear();
        return;
    }

    // Sort around centroid so fan triangulation produces coherent geometry.
    let mut cx = 0.0f32;
    let mut cz = 0.0f32;
    for p in &unique {
        cx += p[0];
        cz += p[1];
    }
    cx /= unique.len() as f32;
    cz /= unique.len() as f32;

    unique.sort_by(|a, b| {
        let aa = (a[1] - cz).atan2(a[0] - cx);
        let bb = (b[1] - cz).atan2(b[0] - cx);
        aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Rotate so the first vertex is stable (lowest z, then lowest x).
    let mut first = 0usize;
    for i in 1..unique.len() {
        let u = unique[i];
        let f = unique[first];
        if u[1] < f[1] || (u[1] == f[1] && u[0] < f[0]) {
            first = i;
        }
    }
    unique.rotate_left(first);
    *points = unique;
}

fn clean_polygon_ordered(points: &mut Vec<[f32; 2]>) {
    const EPS: f32 = 1e-5;
    if points.len() < 3 {
        points.clear();
        return;
    }

    // Drop consecutive duplicates while preserving original edge order.
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(points.len());
    for p in points.iter().copied() {
        if out.last().map(|q| !approx_eq2(*q, p, EPS)).unwrap_or(true) {
            out.push(p);
        }
    }
    if out.len() >= 2 && approx_eq2(out[0], *out.last().unwrap_or(&out[0]), EPS) {
        out.pop();
    }
    if out.len() < 3 {
        points.clear();
        return;
    }

    // Drop collinear vertices to avoid tiny sliver ears.
    let mut i = 0usize;
    while out.len() >= 3 && i < out.len() {
        let n = out.len();
        let a = out[(i + n - 1) % n];
        let b = out[i];
        let c = out[(i + 1) % n];
        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if cross.abs() <= EPS {
            out.remove(i);
            continue;
        }
        i += 1;
    }

    if out.len() < 3 {
        points.clear();
        return;
    }
    *points = out;
}

fn polygon_area2(points: &[[f32; 2]]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut area2 = 0.0f32;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        area2 += a[0] * b[1] - b[0] * a[1];
    }
    area2
}

fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let v0 = [c[0] - a[0], c[1] - a[1]];
    let v1 = [b[0] - a[0], b[1] - a[1]];
    let v2 = [p[0] - a[0], p[1] - a[1]];

    let dot00 = v0[0] * v0[0] + v0[1] * v0[1];
    let dot01 = v0[0] * v1[0] + v0[1] * v1[1];
    let dot02 = v0[0] * v2[0] + v0[1] * v2[1];
    let dot11 = v1[0] * v1[0] + v1[1] * v1[1];
    let dot12 = v1[0] * v2[0] + v1[1] * v2[1];
    let denom = dot00 * dot11 - dot01 * dot01;
    if denom.abs() < 1e-8 {
        return false;
    }
    let inv = 1.0 / denom;
    let u = (dot11 * dot02 - dot01 * dot12) * inv;
    let v = (dot00 * dot12 - dot01 * dot02) * inv;
    u >= -1e-5 && v >= -1e-5 && (u + v) <= 1.0 + 1e-5
}

fn triangulate_polygon_ear_clip(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }
    if points.len() == 3 {
        return vec![[0, 1, 2]];
    }

    let area2 = polygon_area2(points);
    if area2.abs() < 1e-6 {
        return Vec::new();
    }
    let ccw = area2 > 0.0;
    let mut idxs: Vec<usize> = (0..points.len()).collect();
    let mut tris: Vec<[usize; 3]> = Vec::with_capacity(points.len() - 2);
    let mut guard = 0usize;

    while idxs.len() > 3 && guard < points.len() * points.len() {
        guard += 1;
        let n = idxs.len();
        let mut ear_found = false;
        for i in 0..n {
            let i0 = idxs[(i + n - 1) % n];
            let i1 = idxs[i];
            let i2 = idxs[(i + 1) % n];
            let a = points[i0];
            let b = points[i1];
            let c = points[i2];

            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if ccw {
                if cross <= 1e-7 {
                    continue;
                }
            } else if cross >= -1e-7 {
                continue;
            }

            let mut contains_other = false;
            for &j in &idxs {
                if j == i0 || j == i1 || j == i2 {
                    continue;
                }
                if point_in_triangle(points[j], a, b, c) {
                    contains_other = true;
                    break;
                }
            }
            if contains_other {
                continue;
            }

            tris.push([i0, i1, i2]);
            idxs.remove(i);
            ear_found = true;
            break;
        }
        if !ear_found {
            return Vec::new();
        }
    }

    if idxs.len() == 3 {
        tris.push([idxs[0], idxs[1], idxs[2]]);
    }
    tris
}

#[allow(dead_code)]
fn build_subsector_loop(seg_slice: &[WadSeg]) -> Vec<u16> {
    if seg_slice.is_empty() {
        return Vec::new();
    }
    // Primary path: Doom subsector segs are generally already ordered around
    // the perimeter. Using this preserves true boundary shape.
    let ordered: Vec<u16> = seg_slice.iter().map(|s| s.start_vertex).collect();
    if ordered.len() >= 3 {
        let mut ok = true;
        for i in 0..seg_slice.len() {
            let a = seg_slice[i].end_vertex;
            let b = seg_slice[(i + 1) % seg_slice.len()].start_vertex;
            if a != b {
                ok = false;
                break;
            }
        }
        if ok {
            return ordered;
        }
    }

    // Fallback: build a boundary loop by walking connected seg endpoints.
    // Build a boundary loop by walking connected seg endpoints.
    let mut used = vec![false; seg_slice.len()];
    let mut loop_ids: Vec<u16> = Vec::with_capacity(seg_slice.len());

    loop_ids.push(seg_slice[0].start_vertex);
    let mut current = seg_slice[0].end_vertex;
    used[0] = true;

    for _ in 0..seg_slice.len() {
        if current == loop_ids[0] {
            break;
        }
        loop_ids.push(current);
        let mut found = None;
        for (i, s) in seg_slice.iter().enumerate() {
            if used[i] {
                continue;
            }
            if s.start_vertex == current {
                found = Some((i, s.end_vertex));
                break;
            }
            if s.end_vertex == current {
                found = Some((i, s.start_vertex));
                break;
            }
        }
        if let Some((idx, next)) = found {
            used[idx] = true;
            current = next;
        } else {
            break;
        }
    }

    // Remove immediate duplicates.
    let mut dedup: Vec<u16> = Vec::with_capacity(loop_ids.len());
    for v in loop_ids {
        if dedup.last().copied() != Some(v) {
            dedup.push(v);
        }
    }
    dedup
}

fn import_wad(wad_path: &Path, out_path: &Path, map_name: Option<&str>) -> io::Result<()> {
    let data = fs::read(wad_path)?;
    let dir = parse_wad_dir(&data)?;
    let map_index = wad_find_map_index(&dir, map_name)?;
    let map_label = wad_name_to_string(&dir[map_index].name);

    let vertices_raw = wad_find_lump_after(&dir, map_index, WAD_LUMP_VERTEXES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "VERTEXES lump missing"))?;
    let linedefs_raw = wad_find_lump_after(&dir, map_index, WAD_LUMP_LINEDEFS)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "LINEDEFS lump missing"))?;
    let sidedefs_raw = wad_find_lump_after(&dir, map_index, WAD_LUMP_SIDEDEFS)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "SIDEDEFS lump missing"))?;
    let sectors_raw = wad_find_lump_after(&dir, map_index, WAD_LUMP_SECTORS)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "SECTORS lump missing"))?;
    let segs_raw = wad_find_lump_after(&dir, map_index, WAD_LUMP_SEGS)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "SEGS lump missing"))?;
    let ssectors_raw = wad_find_lump_after(&dir, map_index, WAD_LUMP_SSECTORS)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "SSECTORS lump missing"))?;
    let _ = wad_find_lump_after(&dir, map_index, WAD_LUMP_NODES);

    let vertices = parse_wad_vertices(wad_lump_slice(&data, vertices_raw)?);
    let linedefs = parse_wad_linedefs(wad_lump_slice(&data, linedefs_raw)?);
    let sidedefs = parse_wad_sidedefs(wad_lump_slice(&data, sidedefs_raw)?);
    let sectors = parse_wad_sectors(wad_lump_slice(&data, sectors_raw)?);
    let segs = parse_wad_segs(wad_lump_slice(&data, segs_raw)?);
    let _ssectors = parse_wad_ssectors(wad_lump_slice(&data, ssectors_raw)?);

    const UNIT_SCALE: f32 = 1.0 / 64.0;

    let mut out_vertices: Vec<[f32; 3]> = Vec::new();
    let mut out_uvs: Vec<[f32; 2]> = Vec::new();
    let mut out_faces: Vec<(u32, u16, u32, u16, u16, u8, u16)> = Vec::new();
    let mut nav_wall_segments: Vec<[f32; 4]> = Vec::new();
    let mut nav_floor_points: Vec<[f32; 2]> = Vec::new();
    let mut nav_floor_regions: Vec<(u32, u16, f32, f32)> = Vec::new();
    let mut emitted_floor_loops = 0usize;

    for seg in &segs {
        let linedef = if let Some(v) = linedefs.get(seg.linedef as usize) {
            *v
        } else {
            continue;
        };
        let side_idx = if seg.direction == 0 {
            linedef.right_side
        } else {
            linedef.left_side
        };
        let back_side_idx = if seg.direction == 0 {
            linedef.left_side
        } else {
            linedef.right_side
        };
        if side_idx < 0 {
            continue;
        }
        // Skip two-sided segs for this phase to avoid coplanar duplicate walls.
        if back_side_idx >= 0 {
            continue;
        }
        let sidedef = if let Some(v) = sidedefs.get(side_idx as usize) {
            *v
        } else {
            continue;
        };
        let sector_idx = sidedef.sector as usize;
        let sector = if let Some(v) = sectors.get(sector_idx) {
            *v
        } else {
            continue;
        };
        let v1 = if let Some(v) = vertices.get(seg.start_vertex as usize) {
            *v
        } else {
            continue;
        };
        let v2 = if let Some(v) = vertices.get(seg.end_vertex as usize) {
            *v
        } else {
            continue;
        };

        let floor = sector.floor_height as f32 * UNIT_SCALE;
        let ceil = sector.ceiling_height as f32 * UNIT_SCALE;
        let x1 = v1.x as f32 * UNIT_SCALE;
        let z1 = v1.y as f32 * UNIT_SCALE;
        let x2 = v2.x as f32 * UNIT_SCALE;
        let z2 = v2.y as f32 * UNIT_SCALE;

        let first_vert = out_vertices.len() as u32;
        out_vertices.extend_from_slice(&[
            [x1, floor, z1],
            [x2, floor, z2],
            [x2, ceil, z2],
            [x1, ceil, z1],
        ]);
        out_uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        out_faces.push((
            first_vert,
            4,
            0,
            0xFFFF,
            0,
            0,
            sector_idx.min(u16::MAX as usize) as u16,
        ));
        nav_wall_segments.push([x1, z1, x2, z2]);
    }

    // Floor/ceiling generation from linedef loops per sector.
    let mut sector_edges: Vec<Vec<(u16, u16)>> = vec![Vec::new(); sectors.len()];
    for ld in &linedefs {
        if ld.right_side >= 0
            && let Some(sd) = sidedefs.get(ld.right_side as usize)
            && let Some(edges) = sector_edges.get_mut(sd.sector as usize)
        {
            edges.push((ld.start_vertex, ld.end_vertex));
        }
        if ld.left_side >= 0
            && let Some(sd) = sidedefs.get(ld.left_side as usize)
            && let Some(edges) = sector_edges.get_mut(sd.sector as usize)
        {
            edges.push((ld.end_vertex, ld.start_vertex));
        }
    }

    for (sid_usize, edges) in sector_edges.iter().enumerate() {
        if edges.is_empty() {
            continue;
        }
        let sid = sid_usize.min(u16::MAX as usize) as u16;
        let sector = sectors[sid_usize];
        let floor = sector.floor_height as f32 * UNIT_SCALE;
        let ceil = sector.ceiling_height as f32 * UNIT_SCALE;

        let mut used = vec![false; edges.len()];
        for seed in 0..edges.len() {
            if used[seed] {
                continue;
            }
            used[seed] = true;
            let (start_v, mut current_v) = edges[seed];
            let mut loop_ids = vec![start_v, current_v];
            let mut guard = 0usize;
            while current_v != start_v && guard < edges.len() * 2 {
                guard += 1;
                let mut next: Option<(usize, u16)> = None;
                for (ei, (a, b)) in edges.iter().enumerate() {
                    if used[ei] {
                        continue;
                    }
                    if *a == current_v {
                        next = Some((ei, *b));
                        break;
                    }
                }
                if let Some((ei, nv)) = next {
                    used[ei] = true;
                    current_v = nv;
                    loop_ids.push(current_v);
                } else {
                    break;
                }
            }
            if loop_ids.len() < 4 || *loop_ids.last().unwrap_or(&u16::MAX) != start_v {
                continue;
            }
            loop_ids.pop();

            let mut poly: Vec<[f32; 2]> = Vec::with_capacity(loop_ids.len());
            for vid in loop_ids {
                if let Some(v) = vertices.get(vid as usize) {
                    poly.push([v.x as f32 * UNIT_SCALE, v.y as f32 * UNIT_SCALE]);
                }
            }
            clean_polygon_ordered(&mut poly);
            if poly.len() < 3 {
                continue;
            }
            // Ensure floor winding is consistent (CCW in XZ -> upward normal).
            if polygon_area2(&poly) < 0.0 {
                poly.reverse();
            }

            let mut tri_indices = triangulate_polygon_ear_clip(&poly);
            if tri_indices.is_empty() {
                canonicalize_polygon(&mut poly);
                clean_polygon_ordered(&mut poly);
                tri_indices = triangulate_polygon_ear_clip(&poly);
            }
            if tri_indices.is_empty() {
                for i in 1..(poly.len() - 1) {
                    tri_indices.push([0, i, i + 1]);
                }
            }
            if tri_indices.is_empty() {
                continue;
            }

            emitted_floor_loops += 1;
            let nav_first = nav_floor_points.len() as u32;
            for p in &poly {
                nav_floor_points.push(*p);
            }
            nav_floor_regions.push((nav_first, poly.len() as u16, floor, ceil));

            let mut min_px = f32::INFINITY;
            let mut max_px = f32::NEG_INFINITY;
            let mut min_pz = f32::INFINITY;
            let mut max_pz = f32::NEG_INFINITY;
            for p in &poly {
                min_px = min_px.min(p[0]);
                max_px = max_px.max(p[0]);
                min_pz = min_pz.min(p[1]);
                max_pz = max_pz.max(p[1]);
            }
            let span_x = (max_px - min_px).max(1e-3);
            let span_z = (max_pz - min_pz).max(1e-3);

            for tri in &tri_indices {
                let first_floor = out_vertices.len() as u32;
                for &pi in tri {
                    let p = poly[pi];
                    out_vertices.push([p[0], floor, p[1]]);
                    out_uvs.push([(p[0] - min_px) / span_x, (p[1] - min_pz) / span_z]);
                }
                out_faces.push((first_floor, 3, 0, 0xFFFF, 0, 0, sid));

                let first_ceil = out_vertices.len() as u32;
                for &pi in [tri[0], tri[2], tri[1]].iter() {
                    let p = poly[pi];
                    out_vertices.push([p[0], ceil, p[1]]);
                    out_uvs.push([(p[0] - min_px) / span_x, (p[1] - min_pz) / span_z]);
                }
                out_faces.push((first_ceil, 3, 0, 0xFFFF, 0, 0, sid));
            }
        }
    }

    let mut out = String::new();
    out.push_str("// Auto-generated by asset_cli import-wad — DO NOT EDIT\n");
    out.push_str("// Source: ");
    out.push_str(&wad_path.to_string_lossy());
    out.push('\n');
    out.push_str("// Map: ");
    out.push_str(&map_label);
    out.push('\n');
    out.push_str("use embedded_3dgfx::bsp::data::*;\n");
    out.push_str("use embedded_3dgfx::sector_lights::{LightEffectKind, SectorLight};\n\n");

    out.push_str("pub static BSP_PLANES: [Plane; 0] = [];\n");
    out.push_str("pub static BSP_NODES: [Node; 0] = [];\n\n");
    out.push_str("pub static BSP_LEAVES: [Leaf; 1] = [\n");
    out.push_str(&format!(
        "    Leaf {{ cluster: 0, mins: [{}, {}, {}], maxs: [{}, {}, {}], first_marksurface: 0, num_marksurfaces: {} }},\n",
        -32768i16, -32768i16, -32768i16, 32767i16, 32767i16, 32767i16, out_faces.len().min(u16::MAX as usize)
    ));
    out.push_str("];\n\n");

    out.push_str(&format!(
        "pub static BSP_FACES: [Face; {}] = [\n",
        out_faces.len()
    ));
    for (fv, nv, tex, lm, plane, side, sector_light_id) in &out_faces {
        out.push_str(&format!(
            "    Face {{ first_vert: {fv}, num_verts: {nv}, texture_id: {tex}, lightmap_id: {lm}, plane: {plane}, side: {side}, sector_light_id: {sector_light_id} }},\n"
        ));
    }
    out.push_str("];\n\n");

    out.push_str(&format!(
        "pub static BSP_MARKSURFACES: [u16; {}] = [\n    ",
        out_faces.len()
    ));
    for i in 0..out_faces.len() {
        out.push_str(&format!("{}, ", i.min(u16::MAX as usize)));
        if i % 16 == 15 {
            out.push_str("\n    ");
        }
    }
    out.push_str("\n];\n\n");

    out.push_str(&format!(
        "pub static BSP_VERTICES: [[f32; 3]; {}] = [\n",
        out_vertices.len()
    ));
    for v in &out_vertices {
        out.push_str(&format!("    [{:.6}, {:.6}, {:.6}],\n", v[0], v[1], v[2]));
    }
    out.push_str("];\n\n");

    out.push_str(&format!(
        "pub static BSP_UVS: [[f32; 2]; {}] = [\n",
        out_uvs.len()
    ));
    for uv in &out_uvs {
        out.push_str(&format!("    [{:.6}, {:.6}],\n", uv[0], uv[1]));
    }
    out.push_str("];\n\n");

    out.push_str("#[derive(Clone, Copy, Debug)]\n");
    out.push_str("pub struct NavFloorRegion {\n");
    out.push_str("    pub first_point: u32,\n");
    out.push_str("    pub point_count: u16,\n");
    out.push_str("    pub floor_y: f32,\n");
    out.push_str("    pub ceil_y: f32,\n");
    out.push_str("}\n\n");

    out.push_str(&format!(
        "pub static BSP_NAV_WALL_SEGMENTS: [[f32; 4]; {}] = [\n",
        nav_wall_segments.len()
    ));
    for seg in &nav_wall_segments {
        out.push_str(&format!(
            "    [{:.6}, {:.6}, {:.6}, {:.6}],\n",
            seg[0], seg[1], seg[2], seg[3]
        ));
    }
    out.push_str("];\n\n");

    out.push_str(&format!(
        "pub static BSP_NAV_FLOOR_POINTS: [[f32; 2]; {}] = [\n",
        nav_floor_points.len()
    ));
    for p in &nav_floor_points {
        out.push_str(&format!("    [{:.6}, {:.6}],\n", p[0], p[1]));
    }
    out.push_str("];\n\n");

    out.push_str(&format!(
        "pub static BSP_NAV_FLOOR_REGIONS: [NavFloorRegion; {}] = [\n",
        nav_floor_regions.len()
    ));
    for (first_point, point_count, floor_y, ceil_y) in &nav_floor_regions {
        out.push_str(&format!(
            "    NavFloorRegion {{ first_point: {first_point}, point_count: {point_count}, floor_y: {floor_y:.6}, ceil_y: {ceil_y:.6} }},\n"
        ));
    }
    out.push_str("];\n\n");

    out.push_str("pub static BSP_LM_UVS: [[f32; 2]; 0] = [];\n");
    out.push_str("pub static BSP_VIS: [u8; 1] = [0x01];\n");
    out.push_str("pub static BSP_VIS_OFFSETS: [u32; 1] = [0];\n");
    out.push_str("pub const BSP_NUM_CLUSTERS: u16 = 1;\n\n");

    out.push_str(&format!(
        "pub static BSP_SECTOR_LIGHTS: [SectorLight; {}] = [\n",
        sectors.len()
    ));
    for (i, s) in sectors.iter().enumerate() {
        let base = clamp_u8(s.light);
        let alt = base.saturating_sub(64);
        if let Some((kind, speed, duration)) = wad_sector_effect(s.sector_type) {
            out.push_str(&format!(
                "    SectorLight {{ base: {base}, alt: {alt}, speed: {speed:.3}, duration: {duration:.3}, sync: {sync:.3}, effect: Some(LightEffectKind::{kind}) }},\n",
                sync = (i as f32 * 0.137).fract()
            ));
        } else {
            out.push_str(&format!(
                "    SectorLight {{ base: {base}, alt: {alt}, speed: 0.0, duration: 0.0, sync: 0.0, effect: None }},\n"
            ));
        }
    }
    out.push_str("];\n\n");

    out.push_str("pub fn bsp_world() -> BspWorld<'static> {\n");
    out.push_str("    BspWorld::new(\n");
    out.push_str("        &BSP_PLANES,\n        &BSP_NODES,\n        &BSP_LEAVES,\n");
    out.push_str("        &BSP_FACES,\n        &BSP_MARKSURFACES,\n        &BSP_VERTICES,\n");
    out.push_str(
        "        &BSP_UVS,\n        &BSP_LM_UVS,\n        &BSP_VIS,\n        &BSP_VIS_OFFSETS,\n        BSP_NUM_CLUSTERS,\n",
    );
    out.push_str("    )\n}\n\n");
    out.push_str("pub fn bsp_sector_lights() -> &'static [SectorLight] {\n");
    out.push_str("    &BSP_SECTOR_LIGHTS\n}\n");
    out.push_str("pub fn bsp_nav_wall_segments() -> &'static [[f32; 4]] {\n");
    out.push_str("    &BSP_NAV_WALL_SEGMENTS\n}\n");
    out.push_str("pub fn bsp_nav_floor_points() -> &'static [[f32; 2]] {\n");
    out.push_str("    &BSP_NAV_FLOOR_POINTS\n}\n");
    out.push_str("pub fn bsp_nav_floor_regions() -> &'static [NavFloorRegion] {\n");
    out.push_str("    &BSP_NAV_FLOOR_REGIONS\n}\n");
    out.push_str("\n");
    out.push_str("// Keep this file valid when Cargo compiles it as a standalone example.\n");
    out.push_str("pub fn main() {}\n");

    fs::write(out_path, out)?;
    eprintln!(
        "Wrote {} from map {}: {} vertices, {} faces, {} sectors (floor_regions: {}, floor_loops: {})",
        out_path.display(),
        map_label,
        out_vertices.len(),
        out_faces.len(),
        sectors.len(),
        nav_floor_regions.len(),
        emitted_floor_loops
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct QuantizedMesh {
    vertices_q: Vec<[i16; 3]>,
    faces: Vec<[u16; 3]>,
    scale: f32,
}

fn usage() {
    eprintln!(
        "Usage:
  asset_cli convert-mesh --input <mesh.txt> --output <mesh.bin> [--scale 1024]
  asset_cli transcode-texture --input <image.ppm> --output <texture.rgb565>
  asset_cli pack-scene --output <scene.e3dscene> --chunk <kind:path> [--chunk <kind:path> ...]
  asset_cli import-bsp --input <level.bsp> --output <level_bsp.rs> [--no-lightmaps]
  asset_cli import-wad --input <doom.wad> --output <level_wad.rs> [--map MAP01]

Quake 1 BSP (version 29) importer.
Emits a Rust source file with pub static arrays and a bsp_world() constructor.

Mesh text format:
  v <x> <y> <z>
  f <i0> <i1> <i2>"
    );
}

fn parse_flag(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone())
}

fn parse_mesh_text(input: &str, scale: f32) -> io::Result<QuantizedMesh> {
    let mut vertices_q = Vec::new();
    let mut faces = Vec::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.first().copied() {
            Some("v") if parts.len() == 4 => {
                let x: f32 = parts[1]
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid vertex x"))?;
                let y: f32 = parts[2]
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid vertex y"))?;
                let z: f32 = parts[3]
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid vertex z"))?;
                vertices_q.push([
                    (x * scale).round() as i16,
                    (y * scale).round() as i16,
                    (z * scale).round() as i16,
                ]);
            }
            Some("f") if parts.len() == 4 => {
                let i0: u16 = parts[1]
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid face i0"))?;
                let i1: u16 = parts[2]
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid face i1"))?;
                let i2: u16 = parts[3]
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid face i2"))?;
                faces.push([i0, i1, i2]);
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported mesh line",
                ));
            }
        }
    }
    Ok(QuantizedMesh {
        vertices_q,
        faces,
        scale,
    })
}

fn write_quantized_mesh(path: &Path, mesh: &QuantizedMesh) -> io::Result<()> {
    let mut out = Vec::new();
    out.extend_from_slice(b"E3DM");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(mesh.vertices_q.len() as u32).to_le_bytes());
    out.extend_from_slice(&(mesh.faces.len() as u32).to_le_bytes());
    out.extend_from_slice(&mesh.scale.to_le_bytes());
    for v in &mesh.vertices_q {
        out.extend_from_slice(&v[0].to_le_bytes());
        out.extend_from_slice(&v[1].to_le_bytes());
        out.extend_from_slice(&v[2].to_le_bytes());
    }
    for f in &mesh.faces {
        out.extend_from_slice(&f[0].to_le_bytes());
        out.extend_from_slice(&f[1].to_le_bytes());
        out.extend_from_slice(&f[2].to_le_bytes());
    }
    fs::write(path, out)
}

fn parse_p6_ppm(data: &[u8]) -> io::Result<(usize, usize, Vec<u8>)> {
    let text = std::str::from_utf8(data)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "ppm is not utf8 header"))?;
    let mut tokens = text.split_whitespace();
    if tokens.next() != Some("P6") {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected P6"));
    }
    let width: usize = tokens
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing width"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid width"))?;
    let height: usize = tokens
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing height"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid height"))?;
    let maxv: usize = tokens
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing max value"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid max value"))?;
    if maxv != 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only max value 255 supported",
        ));
    }
    // PPM header ends at first byte after the third token + whitespace.
    let mut header_end = 0usize;
    let mut seen = 0usize;
    for (idx, b) in data.iter().enumerate() {
        if *b == b' ' || *b == b'\n' || *b == b'\t' || *b == b'\r' {
            continue;
        }
        // walk tokens manually
        let mut j = idx;
        while j < data.len()
            && data[j] != b' '
            && data[j] != b'\n'
            && data[j] != b'\t'
            && data[j] != b'\r'
        {
            j += 1;
        }
        seen += 1;
        if seen == 4 {
            header_end = j;
            while header_end < data.len()
                && (data[header_end] == b' '
                    || data[header_end] == b'\n'
                    || data[header_end] == b'\t'
                    || data[header_end] == b'\r')
            {
                header_end += 1;
            }
            break;
        }
    }
    let expected = width * height * 3;
    let rgb = data
        .get(header_end..header_end + expected)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "ppm payload too small"))?
        .to_vec();
    Ok((width, height, rgb))
}

fn rgb565_bytes(r: u8, g: u8, b: u8) -> [u8; 2] {
    let r5 = (r as u16 >> 3) & 0x1F;
    let g6 = (g as u16 >> 2) & 0x3F;
    let b5 = (b as u16 >> 3) & 0x1F;
    let packed = (r5 << 11) | (g6 << 5) | b5;
    packed.to_le_bytes()
}

fn transcode_ppm_to_rgb565(input_path: &Path, output_path: &Path) -> io::Result<()> {
    let data = fs::read(input_path)?;
    let (width, height, rgb) = parse_p6_ppm(&data)?;
    let mut out = Vec::new();
    out.extend_from_slice(b"E3DT");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(width as u32).to_le_bytes());
    out.extend_from_slice(&(height as u32).to_le_bytes());
    for chunk in rgb.chunks_exact(3) {
        out.extend_from_slice(&rgb565_bytes(chunk[0], chunk[1], chunk[2]));
    }
    fs::write(output_path, out)
}

fn kind_to_u16(kind: &str) -> io::Result<u16> {
    match kind {
        "mesh" => Ok(1),
        "texture" => Ok(2),
        "meta" => Ok(3),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown chunk kind",
        )),
    }
}

fn pack_scene(output: &Path, chunks: &[String]) -> io::Result<()> {
    let mut out = Vec::new();
    out.extend_from_slice(b"E3DS");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(chunks.len() as u16).to_le_bytes());
    for item in chunks {
        let (kind, path) = item.split_once(':').ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "chunk must be kind:path")
        })?;
        let kind_id = kind_to_u16(kind)?;
        let bytes = fs::read(path)?;
        out.extend_from_slice(&kind_id.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
    }
    fs::write(output, out)
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing command",
        ));
    }
    match args[1].as_str() {
        "convert-mesh" => {
            let input = parse_flag(&args, "--input")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --input"))?;
            let output = parse_flag(&args, "--output")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --output"))?;
            let scale = parse_flag(&args, "--scale")
                .unwrap_or_else(|| "1024".to_string())
                .parse::<f32>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid --scale"))?;
            let mesh_text = fs::read_to_string(&input)?;
            let mesh = parse_mesh_text(&mesh_text, scale)?;
            write_quantized_mesh(Path::new(&output), &mesh)?;
        }
        "transcode-texture" => {
            let input = parse_flag(&args, "--input")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --input"))?;
            let output = parse_flag(&args, "--output")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --output"))?;
            transcode_ppm_to_rgb565(Path::new(&input), Path::new(&output))?;
        }
        "pack-scene" => {
            let output = parse_flag(&args, "--output")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --output"))?;
            let mut chunks = Vec::new();
            let mut idx = 2usize;
            while idx + 1 < args.len() {
                if args[idx] == "--chunk" {
                    chunks.push(args[idx + 1].clone());
                    idx += 2;
                } else {
                    idx += 1;
                }
            }
            if chunks.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "pack-scene requires at least one --chunk kind:path",
                ));
            }
            pack_scene(Path::new(&output), &chunks)?;
        }
        "import-bsp" => {
            let input = parse_flag(&args, "--input")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --input"))?;
            let output = parse_flag(&args, "--output")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --output"))?;
            let no_lightmaps = args.iter().any(|a| a == "--no-lightmaps");
            import_bsp(Path::new(&input), Path::new(&output), !no_lightmaps)?;
        }
        "import-wad" => {
            let input = parse_flag(&args, "--input")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --input"))?;
            let output = parse_flag(&args, "--output")
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --output"))?;
            let map = parse_flag(&args, "--map");
            import_wad(Path::new(&input), Path::new(&output), map.as_deref())?;
        }
        _ => {
            usage();
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown command",
            ));
        }
    }
    io::stdout().flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wad_name_trims_nul_padding() {
        let name = *b"MAP01\0\0\0";
        assert_eq!(wad_name_to_string(&name), "MAP01");
    }

    #[test]
    fn wad_sector_effect_maps_known_types() {
        let glow = wad_sector_effect(8).expect("type 8 should map to glow");
        assert_eq!(glow.0, "Glow");
        let random = wad_sector_effect(1).expect("type 1 should map to random");
        assert_eq!(random.0, "Random");
        assert!(wad_sector_effect(999).is_none());
    }
}
