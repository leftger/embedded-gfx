use crate::error::RenderError;

pub const SCENE_MAGIC: [u8; 4] = *b"E3DS";
pub const SCENE_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Mesh = 1,
    Texture = 2,
    Meta = 3,
}

impl ChunkKind {
    fn from_u16(v: u16) -> Option<Self> {
        match v {
            1 => Some(Self::Mesh),
            2 => Some(Self::Texture),
            3 => Some(Self::Meta),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EncodedChunk<'a> {
    pub kind: ChunkKind,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct SceneChunkRef<'a> {
    pub kind: ChunkKind,
    pub payload: &'a [u8],
}

pub fn encode_scene<const N: usize>(
    chunks: &[EncodedChunk<'_>],
) -> Result<heapless::Vec<u8, N>, RenderError> {
    let mut out = heapless::Vec::<u8, N>::new();
    for b in SCENE_MAGIC {
        out.push(b)
            .map_err(|_| RenderError::InvalidInput("scene buffer too small"))?;
    }
    for b in SCENE_VERSION.to_le_bytes() {
        out.push(b)
            .map_err(|_| RenderError::InvalidInput("scene buffer too small"))?;
    }
    for b in (chunks.len() as u16).to_le_bytes() {
        out.push(b)
            .map_err(|_| RenderError::InvalidInput("scene buffer too small"))?;
    }
    for chunk in chunks {
        for b in (chunk.kind as u16).to_le_bytes() {
            out.push(b)
                .map_err(|_| RenderError::InvalidInput("scene buffer too small"))?;
        }
        for b in (chunk.payload.len() as u32).to_le_bytes() {
            out.push(b)
                .map_err(|_| RenderError::InvalidInput("scene buffer too small"))?;
        }
        for b in chunk.payload {
            out.push(*b)
                .map_err(|_| RenderError::InvalidInput("scene buffer too small"))?;
        }
    }
    Ok(out)
}

pub struct ChunkCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining_chunks: u16,
}

impl<'a> ChunkCursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, RenderError> {
        if bytes.len() < 8 {
            return Err(RenderError::InvalidInput("scene too small"));
        }
        if bytes[0..4] != SCENE_MAGIC {
            return Err(RenderError::InvalidInput("invalid scene magic"));
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != SCENE_VERSION {
            return Err(RenderError::InvalidInput("unsupported scene version"));
        }
        let chunk_count = u16::from_le_bytes([bytes[6], bytes[7]]);
        Ok(Self {
            bytes,
            offset: 8,
            remaining_chunks: chunk_count,
        })
    }

    pub fn next_chunk(&mut self) -> Result<Option<SceneChunkRef<'a>>, RenderError> {
        if self.remaining_chunks == 0 {
            return Ok(None);
        }
        if self.offset + 6 > self.bytes.len() {
            return Err(RenderError::InvalidInput("truncated scene chunk header"));
        }
        let kind_raw = u16::from_le_bytes([self.bytes[self.offset], self.bytes[self.offset + 1]]);
        let len = u32::from_le_bytes([
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
            self.bytes[self.offset + 4],
            self.bytes[self.offset + 5],
        ]) as usize;
        self.offset += 6;
        if self.offset + len > self.bytes.len() {
            return Err(RenderError::InvalidInput("truncated scene chunk payload"));
        }
        let payload = &self.bytes[self.offset..self.offset + len];
        self.offset += len;
        self.remaining_chunks -= 1;
        let kind =
            ChunkKind::from_u16(kind_raw).ok_or(RenderError::InvalidInput("unknown chunk kind"))?;
        Ok(Some(SceneChunkRef { kind, payload }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_scene_roundtrip() {
        let chunks = [
            EncodedChunk {
                kind: ChunkKind::Mesh,
                payload: &[1, 2, 3],
            },
            EncodedChunk {
                kind: ChunkKind::Texture,
                payload: &[4, 5],
            },
        ];
        let bytes = encode_scene::<128>(&chunks).unwrap();
        let mut cursor = ChunkCursor::new(&bytes).unwrap();
        let c1 = cursor.next_chunk().unwrap().unwrap();
        assert_eq!(c1.kind, ChunkKind::Mesh);
        assert_eq!(c1.payload, &[1, 2, 3]);
        let c2 = cursor.next_chunk().unwrap().unwrap();
        assert_eq!(c2.kind, ChunkKind::Texture);
        assert_eq!(c2.payload, &[4, 5]);
        assert!(cursor.next_chunk().unwrap().is_none());
    }
}
