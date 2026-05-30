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
    i32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}
fn read_f32_le(data: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

struct LumpDir {
    offset: usize,
    length: usize,
}

fn parse_lump_dirs(data: &[u8]) -> io::Result<Vec<LumpDir>> {
    if data.len() < 4 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "bsp too small")); }
    let ver = read_u32_le(data, 0);
    if ver != BSP_VERSION {
        return Err(io::Error::new(io::ErrorKind::InvalidData,
            format!("unsupported BSP version {ver}, expected {BSP_VERSION}")));
    }
    let mut lumps = Vec::with_capacity(15);
    for i in 0..15usize {
        let base = 4 + i * 8;
        if base + 8 > data.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "lump dir truncated"));
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
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
            format!("lump {idx} out of range")));
    }
    Ok(&data[l.offset..l.offset + l.length])
}

// Quake BSP plane: normal[3] f32, dist f32, type i32 (20 bytes)
struct QPlane { normal: [f32; 3], dist: f32 }
fn parse_planes(raw: &[u8]) -> Vec<QPlane> {
    raw.chunks_exact(20).map(|c| QPlane {
        normal: [read_f32_le(c, 0), read_f32_le(c, 4), read_f32_le(c, 8)],
        dist:    read_f32_le(c, 12),
    }).collect()
}

// Quake BSP node: planenum i32, children[2] i16, mins/maxs[6] i16, firstface u16, numfaces u16 (24 bytes)
struct QNode { plane: u32, children: [i16; 2], mins: [i16; 3], maxs: [i16; 3], first_face: u16, num_faces: u16 }
fn parse_nodes(raw: &[u8]) -> Vec<QNode> {
    raw.chunks_exact(24).map(|c| QNode {
        plane:      read_u32_le(c, 0),
        children:   [read_i16_le(c, 4), read_i16_le(c, 6)],
        mins:       [read_i16_le(c, 8), read_i16_le(c, 10), read_i16_le(c, 12)],
        maxs:       [read_i16_le(c, 14), read_i16_le(c, 16), read_i16_le(c, 18)],
        first_face: read_u16_le(c, 20),
        num_faces:  read_u16_le(c, 22),
    }).collect()
}

// Quake BSP leaf: contents i32, visofs i32, mins/maxs[6] i16, firstmark u16, nummark u16, ambient[4] u8 (28 bytes)
struct QLeaf { contents: i32, visofs: i32, mins: [i16; 3], maxs: [i16; 3], first_mark: u16, num_mark: u16 }
fn parse_leaves(raw: &[u8]) -> Vec<QLeaf> {
    raw.chunks_exact(28).map(|c| QLeaf {
        contents:   read_i32_le(c, 0),
        visofs:     read_i32_le(c, 4),
        mins:       [read_i16_le(c, 8), read_i16_le(c, 10), read_i16_le(c, 12)],
        maxs:       [read_i16_le(c, 14), read_i16_le(c, 16), read_i16_le(c, 18)],
        first_mark: read_u16_le(c, 20),
        num_mark:   read_u16_le(c, 22),
    }).collect()
}

// Quake BSP texinfo: vecs[2][4] f32, miptex i32, flags i32 (40 bytes)
struct QTexinfo { vecs: [[f32; 4]; 2], miptex: i32 }
fn parse_texinfos(raw: &[u8]) -> Vec<QTexinfo> {
    raw.chunks_exact(40).map(|c| QTexinfo {
        vecs: [
            [read_f32_le(c, 0), read_f32_le(c, 4), read_f32_le(c, 8), read_f32_le(c, 12)],
            [read_f32_le(c, 16), read_f32_le(c, 20), read_f32_le(c, 24), read_f32_le(c, 28)],
        ],
        miptex: read_i32_le(c, 32),
    }).collect()
}

// Quake BSP face: planenum u16, side i16, firstedge i32, numedges i16, texinfo u16, styles[4] u8, lightofs i32 (20 bytes)
struct QFace { planenum: u16, side: i16, firstedge: i32, numedges: i16, texinfo: u16, lightofs: i32 }
fn parse_faces(raw: &[u8]) -> Vec<QFace> {
    raw.chunks_exact(20).map(|c| QFace {
        planenum:  read_u16_le(c, 0),
        side:      read_i16_le(c, 2),
        firstedge: read_i32_le(c, 4),
        numedges:  read_i16_le(c, 8),
        texinfo:   read_u16_le(c, 10),
        // styles[4] at offset 12
        lightofs:  read_i32_le(c, 16),
    }).collect()
}

fn parse_vertices(raw: &[u8]) -> Vec<[f32; 3]> {
    raw.chunks_exact(12).map(|c| [read_f32_le(c, 0), read_f32_le(c, 4), read_f32_le(c, 8)]).collect()
}
fn parse_edges(raw: &[u8]) -> Vec<[u16; 2]> {
    raw.chunks_exact(4).map(|c| [read_u16_le(c, 0), read_u16_le(c, 2)]).collect()
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
    if x == 0 { return 1; }
    let mut p = 1usize;
    while p < x { p <<= 1; }
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
    width: usize,  // texels
    height: usize,
    pixels: Vec<[u8; 2]>,  // RGB565 little-endian pairs
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
    if face.lightofs < 0 { return None; }

    let mut s_coords: Vec<f32> = Vec::new();
    let mut t_coords: Vec<f32> = Vec::new();
    for k in 0..face.numedges as usize {
        let se = surfedges[face.firstedge as usize + k];
        let vi = surfedge_vert(se, edges);
        let v = &q_verts[vi];
        let s = v[0]*texinfo.vecs[0][0] + v[1]*texinfo.vecs[0][1] + v[2]*texinfo.vecs[0][2] + texinfo.vecs[0][3];
        let t = v[0]*texinfo.vecs[1][0] + v[1]*texinfo.vecs[1][1] + v[2]*texinfo.vecs[1][2] + texinfo.vecs[1][3];
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
    if lo + n_texels > lighting.len() { return None; }

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
    pixels: Vec<u16>,     // RGB565 as u16
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
        if self.shelf_y + lm.height > self.height { return None; }

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
        if lm.height > self.shelf_h { self.shelf_h = lm.height; }
        Some((ax, ay))
    }

    fn into_rgb565_bytes(self) -> Vec<u8> {
        self.pixels.iter().flat_map(|&p| p.to_le_bytes()).collect()
    }
}

pub fn import_bsp(bsp_path: &Path, out_path: &Path, with_lightmaps: bool) -> io::Result<()> {
    let data = fs::read(bsp_path)?;
    let lumps = parse_lump_dirs(&data)?;

    let q_planes      = parse_planes(lump_slice(&data, &lumps, LUMP_PLANES)?);
    let q_vertices    = parse_vertices(lump_slice(&data, &lumps, LUMP_VERTICES)?);
    let vis_raw       = lump_slice(&data, &lumps, LUMP_VIS)?;
    let q_nodes       = parse_nodes(lump_slice(&data, &lumps, LUMP_NODES)?);
    let q_texinfos    = parse_texinfos(lump_slice(&data, &lumps, LUMP_TEXINFO)?);
    let q_faces       = parse_faces(lump_slice(&data, &lumps, LUMP_FACES)?);
    let lighting_raw  = lump_slice(&data, &lumps, LUMP_LIGHTING)?;
    let q_leaves      = parse_leaves(lump_slice(&data, &lumps, LUMP_LEAVES)?);
    let q_marksurfs   = parse_marksurfaces(lump_slice(&data, &lumps, LUMP_MARKSURFACES)?);
    let q_edges       = parse_edges(lump_slice(&data, &lumps, LUMP_EDGES)?);
    let q_surfedges   = parse_surfedges(lump_slice(&data, &lumps, LUMP_SURFEDGES)?);

    // ----- Build output vertices, UVs, and faces -----
    // For each Quake face, we build a vertex fan and append to flat arrays.
    let mut out_vertices: Vec<[f32; 3]> = Vec::new();
    let mut out_uvs:      Vec<[f32; 2]> = Vec::new();
    let mut out_lm_uvs:   Vec<[f32; 2]> = Vec::new();
    let mut out_faces:    Vec<(u32, u16, u16, u8, u16, u16)> = Vec::new();
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
        let texinfo = if te_idx < q_texinfos.len() { &q_texinfos[te_idx] } else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "texinfo out of range"));
        };

        // Surface texture ID (unique per miptex index)
        let tex_id = *tex_id_map.entry(texinfo.miptex).or_insert_with(|| {
            let id = next_tex_id;
            next_tex_id += 1;
            id
        });

        let first_vert = out_vertices.len() as u32;
        let n = face.numedges as usize;
        if n < 3 { continue; }

        // Build surface UVs — we need texel-space UV normalised to [0,1]
        // We don't have texture dimensions here, so use raw texel coords and
        // normalise by an assumed 64×64 (users can adjust by scaling UVs).
        // A more complete implementation would embed texture sizes from the miptex lump.
        let assumed_tex_size = 64.0f32;

        // Compute lightmap for this face
        let lm_data = if with_lightmaps {
            compute_face_lightmap(face, &q_vertices, &q_surfedges, &q_edges, texinfo, lighting_raw)
        } else { None };

        let lm_atlas_pos = if let Some(ref lm) = lm_data {
            atlas.pack(lm)
        } else { None };

        for k in 0..n {
            let se = q_surfedges[(face.firstedge as usize) + k];
            let vi = surfedge_vert(se, &q_edges);
            let v = q_vertices[vi];
            let s = v[0]*texinfo.vecs[0][0] + v[1]*texinfo.vecs[0][1] + v[2]*texinfo.vecs[0][2] + texinfo.vecs[0][3];
            let t = v[0]*texinfo.vecs[1][0] + v[1]*texinfo.vecs[1][1] + v[2]*texinfo.vecs[1][2] + texinfo.vecs[1][3];
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
            if lm_texture_id.is_none() { lm_texture_id = Some(next_tex_id); next_tex_id += 1; }
            lm_texture_id.unwrap() as u16
        } else { 0xFFFF };

        out_faces.push((first_vert, n as u16, face.planenum, face.side as u8, tex_id as u16, lightmap_id));
    }

    // ----- Marksurfaces: indices into q_faces which map 1:1 to out_faces -----
    let out_marksurfs: Vec<u16> = q_marksurfs.iter().cloned().collect();

    // ----- Nodes -----
    // q_nodes: children are i16 (negative = leaf via -(n+1))
    // Quake leaf 0 is the outside solid leaf; actual content leaves start at 1.
    // We keep indices as-is and translate children.
    let out_nodes: Vec<(u16, i32, i32, i16, i16, i16, i16, i16, i16, u16, u16)> = q_nodes.iter().map(|n| {
        (n.plane as u16,
         quake_child_to_i32(n.children[0]),
         quake_child_to_i32(n.children[1]),
         n.mins[0], n.mins[1], n.mins[2],
         n.maxs[0], n.maxs[1], n.maxs[2],
         n.first_face, n.num_faces)
    }).collect();

    // ----- Leaves -----
    // cluster = leaf_index (Quake 1: one cluster per leaf, leaf 0 = solid)
    // We map leaf_index directly. vis_offsets[leaf_i] = leaf.visofs (or u32::MAX if -1).
    let mut vis_offsets: Vec<u32> = Vec::with_capacity(q_leaves.len());
    let out_leaves: Vec<(i16, i16, i16, i16, i16, i16, i16, u16, u16)> = q_leaves.iter().enumerate().map(|(li, l)| {
        let cluster = if l.contents < 0 && l.visofs >= 0 { li as i16 } else { -1i16 };
        vis_offsets.push(if l.visofs >= 0 { l.visofs as u32 } else { u32::MAX });
        (cluster,
         l.mins[0], l.mins[1], l.mins[2],
         l.maxs[0], l.maxs[1], l.maxs[2],
         l.first_mark, l.num_mark)
    }).collect();

    let num_clusters = q_leaves.len() as u16;

    // ----- Write Rust source -----
    let mut out = String::new();
    out.push_str("// Auto-generated by asset_cli import-bsp — DO NOT EDIT\n");
    out.push_str("// Source: ");
    out.push_str(&bsp_path.to_string_lossy());
    out.push('\n');
    out.push_str("use embedded_3dgfx::bsp::data::*;\n\n");

    // Planes
    out.push_str(&format!("pub static BSP_PLANES: [Plane; {}] = [\n", q_planes.len()));
    for p in &q_planes {
        out.push_str(&format!("    Plane {{ normal: [{:.6}, {:.6}, {:.6}], dist: {:.6} }},\n",
            p.normal[0], p.normal[1], p.normal[2], p.dist));
    }
    out.push_str("];\n\n");

    // Nodes
    out.push_str(&format!("pub static BSP_NODES: [Node; {}] = [\n", out_nodes.len()));
    for (plane, c0, c1, mn0, mn1, mn2, mx0, mx1, mx2, ff, nf) in &out_nodes {
        out.push_str(&format!(
            "    Node {{ plane: {plane}, children: [{c0}, {c1}], mins: [{mn0}, {mn1}, {mn2}], maxs: [{mx0}, {mx1}, {mx2}], first_face: {ff}, num_faces: {nf} }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Leaves
    out.push_str(&format!("pub static BSP_LEAVES: [Leaf; {}] = [\n", out_leaves.len()));
    for (cl, mn0, mn1, mn2, mx0, mx1, mx2, fm, nm) in &out_leaves {
        out.push_str(&format!(
            "    Leaf {{ cluster: {cl}, mins: [{mn0}, {mn1}, {mn2}], maxs: [{mx0}, {mx1}, {mx2}], first_marksurface: {fm}, num_marksurfaces: {nm} }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Faces
    out.push_str(&format!("pub static BSP_FACES: [Face; {}] = [\n", out_faces.len()));
    for (fv, nv, plane, side, tex, lm) in &out_faces {
        out.push_str(&format!(
            "    Face {{ first_vert: {fv}, num_verts: {nv}, texture_id: {tex}, lightmap_id: {lm}, plane: {plane}, side: {side} }},\n"
        ));
    }
    out.push_str("];\n\n");

    // Marksurfaces
    out.push_str(&format!("pub static BSP_MARKSURFACES: [u16; {}] = [", out_marksurfs.len()));
    for (i, ms) in out_marksurfs.iter().enumerate() {
        if i % 16 == 0 { out.push_str("\n    "); }
        out.push_str(&format!("{ms}, "));
    }
    out.push_str("\n];\n\n");

    // Vertices
    out.push_str(&format!("pub static BSP_VERTICES: [[f32; 3]; {}] = [\n", out_vertices.len()));
    for v in &out_vertices {
        out.push_str(&format!("    [{:.6}, {:.6}, {:.6}],\n", v[0], v[1], v[2]));
    }
    out.push_str("];\n\n");

    // UVs
    out.push_str(&format!("pub static BSP_UVS: [[f32; 2]; {}] = [\n", out_uvs.len()));
    for uv in &out_uvs {
        out.push_str(&format!("    [{:.6}, {:.6}],\n", uv[0], uv[1]));
    }
    out.push_str("];\n\n");

    // Lightmap UVs
    if with_lightmaps {
        out.push_str(&format!("pub static BSP_LM_UVS: [[f32; 2]; {}] = [\n", out_lm_uvs.len()));
        for uv in &out_lm_uvs {
            out.push_str(&format!("    [{:.6}, {:.6}],\n", uv[0], uv[1]));
        }
        out.push_str("];\n\n");
    }

    // VIS blob
    out.push_str(&format!("pub static BSP_VIS: [u8; {}] = [\n    ", vis_raw.len()));
    for (i, b) in vis_raw.iter().enumerate() {
        out.push_str(&format!("0x{b:02X}, "));
        if i % 16 == 15 { out.push_str("\n    "); }
    }
    out.push_str("\n];\n\n");

    // VIS offsets
    out.push_str(&format!("pub static BSP_VIS_OFFSETS: [u32; {}] = [", vis_offsets.len()));
    for (i, v) in vis_offsets.iter().enumerate() {
        if i % 16 == 0 { out.push_str("\n    "); }
        out.push_str(&format!("{v}, "));
    }
    out.push_str("\n];\n\n");

    out.push_str(&format!("pub const BSP_NUM_CLUSTERS: u16 = {num_clusters};\n\n"));

    // Lightmap atlas texture data
    if with_lightmaps {
        let atlas_w = atlas.width;
        let atlas_h = atlas.height;
        let atlas_bytes = atlas.into_rgb565_bytes();
        out.push_str(&format!("pub static BSP_LIGHTMAP_ATLAS: [u8; {}] = [\n    ", atlas_bytes.len()));
        for (i, b) in atlas_bytes.iter().enumerate() {
            out.push_str(&format!("0x{b:02X}, "));
            if i % 16 == 15 { out.push_str("\n    "); }
        }
        out.push_str("\n];\n\n");
        out.push_str(&format!("pub const BSP_LIGHTMAP_ATLAS_WIDTH: u32 = {atlas_w};\n"));
        out.push_str(&format!("pub const BSP_LIGHTMAP_ATLAS_HEIGHT: u32 = {atlas_h};\n\n"));
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
        q_planes.len(), out_nodes.len(), out_leaves.len(), out_faces.len(), out_vertices.len()
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
