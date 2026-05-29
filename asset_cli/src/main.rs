use std::fs;
use std::io::{self, Write};
use std::path::Path;

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
