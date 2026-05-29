use crate::error::{RecoveryAction, RenderError, RuntimeFaultKind, StallKind};
use crate::scene_format::{ChunkCursor, ChunkKind, SceneChunkRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadStatus {
    Uploaded,
    Busy,
}

pub trait ResourceUploader {
    fn upload_mesh(&mut self, payload: &[u8]) -> Result<UploadStatus, RenderError>;
    fn upload_texture(&mut self, payload: &[u8]) -> Result<UploadStatus, RenderError>;
    fn upload_meta(&mut self, payload: &[u8]) -> Result<UploadStatus, RenderError>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamProgress {
    pub chunks_total: usize,
    pub chunks_uploaded: usize,
    pub chunks_deferred: usize,
}

pub struct CooperativeChunkLoader<'a, U: ResourceUploader> {
    cursor: ChunkCursor<'a>,
    uploader: U,
    max_chunks_per_poll: usize,
    deferred: Option<SceneChunkRef<'a>>,
    progress: StreamProgress,
}

impl<'a, U: ResourceUploader> CooperativeChunkLoader<'a, U> {
    pub fn new(
        bytes: &'a [u8],
        uploader: U,
        max_chunks_per_poll: usize,
    ) -> Result<Self, RenderError> {
        if max_chunks_per_poll == 0 {
            return Err(RenderError::InvalidInput(
                "max_chunks_per_poll must be >= 1",
            ));
        }
        let cursor = ChunkCursor::new(bytes)?;
        Ok(Self {
            cursor,
            uploader,
            max_chunks_per_poll,
            deferred: None,
            progress: StreamProgress::default(),
        })
    }

    fn upload_chunk(&mut self, chunk: SceneChunkRef<'a>) -> Result<UploadStatus, RenderError> {
        match chunk.kind {
            ChunkKind::Mesh => self.uploader.upload_mesh(chunk.payload),
            ChunkKind::Texture => self.uploader.upload_texture(chunk.payload),
            ChunkKind::Meta => self.uploader.upload_meta(chunk.payload),
        }
    }

    pub fn poll(&mut self) -> Result<bool, RenderError> {
        let mut processed = 0usize;
        while processed < self.max_chunks_per_poll {
            let chunk = if let Some(ch) = self.deferred.take() {
                ch
            } else {
                match self.cursor.next_chunk()? {
                    Some(ch) => ch,
                    None => return Ok(true),
                }
            };
            self.progress.chunks_total += 1;
            match self.upload_chunk(chunk)? {
                UploadStatus::Uploaded => {
                    self.progress.chunks_uploaded += 1;
                }
                UploadStatus::Busy => {
                    self.progress.chunks_deferred += 1;
                    self.deferred = Some(chunk);
                    return Err(RenderError::Recoverable {
                        fault: RuntimeFaultKind::Stall(StallKind::PresentStage),
                        action: RecoveryAction::Retry,
                    });
                }
            }
            processed += 1;
        }
        Ok(false)
    }

    pub fn progress(&self) -> StreamProgress {
        self.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_format::{ChunkKind, EncodedChunk, encode_scene};

    struct TestUploader {
        busy_once: bool,
        meshes: usize,
    }

    impl ResourceUploader for TestUploader {
        fn upload_mesh(&mut self, _payload: &[u8]) -> Result<UploadStatus, RenderError> {
            if self.busy_once {
                self.busy_once = false;
                return Ok(UploadStatus::Busy);
            }
            self.meshes += 1;
            Ok(UploadStatus::Uploaded)
        }
        fn upload_texture(&mut self, _payload: &[u8]) -> Result<UploadStatus, RenderError> {
            Ok(UploadStatus::Uploaded)
        }
        fn upload_meta(&mut self, _payload: &[u8]) -> Result<UploadStatus, RenderError> {
            Ok(UploadStatus::Uploaded)
        }
    }

    #[test]
    fn cooperative_loader_retries_busy_upload() {
        let chunks = [
            EncodedChunk {
                kind: ChunkKind::Mesh,
                payload: &[1, 2],
            },
            EncodedChunk {
                kind: ChunkKind::Texture,
                payload: &[3, 4],
            },
        ];
        let bytes = encode_scene::<128>(&chunks).unwrap();
        let uploader = TestUploader {
            busy_once: true,
            meshes: 0,
        };
        let mut loader = CooperativeChunkLoader::new(&bytes, uploader, 4).unwrap();
        let first = loader.poll();
        assert!(matches!(
            first,
            Err(RenderError::Recoverable {
                action: RecoveryAction::Retry,
                ..
            })
        ));
        let done = loader.poll().unwrap();
        assert!(done);
        assert!(loader.progress().chunks_uploaded >= 2);
    }
}
