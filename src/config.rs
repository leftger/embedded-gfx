use crate::error::{BudgetKind, RenderError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileCaps {
    pub max_draw_primitives: usize,
    pub max_meshes_per_frame: usize,
    pub max_textures: usize,
    pub max_width: usize,
    pub max_height: usize,
    pub max_triangles_per_mesh: usize,
    pub max_vertices_per_mesh: usize,
}

impl ProfileCaps {
    pub const fn validate_framebuffer(
        &self,
        width: usize,
        height: usize,
    ) -> Result<(), RenderError> {
        if width > self.max_width || height > self.max_height {
            return Err(RenderError::OutOfBudget(BudgetKind::FramebufferDimensions {
                width,
                height,
                max_width: self.max_width,
                max_height: self.max_height,
            }));
        }
        Ok(())
    }
}

pub const PROFILE_M3_MIN: ProfileCaps = ProfileCaps {
    max_draw_primitives: 1_024,
    max_meshes_per_frame: 64,
    max_textures: 4,
    max_width: 240,
    max_height: 240,
    max_triangles_per_mesh: 2_048,
    max_vertices_per_mesh: 2_048,
};

pub const PROFILE_M4_BALANCED: ProfileCaps = ProfileCaps {
    max_draw_primitives: 2_048,
    max_meshes_per_frame: 128,
    max_textures: 8,
    max_width: 320,
    max_height: 240,
    max_triangles_per_mesh: 4_096,
    max_vertices_per_mesh: 4_096,
};

pub const PROFILE_M33_SECURE: ProfileCaps = ProfileCaps {
    max_draw_primitives: 2_048,
    max_meshes_per_frame: 128,
    max_textures: 8,
    max_width: 320,
    max_height: 240,
    max_triangles_per_mesh: 4_096,
    max_vertices_per_mesh: 4_096,
};

pub const PROFILE_M55_PERF: ProfileCaps = ProfileCaps {
    max_draw_primitives: 4_096,
    max_meshes_per_frame: 256,
    max_textures: 16,
    max_width: 480,
    max_height: 320,
    max_triangles_per_mesh: 8_192,
    max_vertices_per_mesh: 8_192,
};
