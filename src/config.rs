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

pub const PROFILE_M3_BALANCED: ProfileCaps = ProfileCaps {
    max_draw_primitives: 1_536,
    max_meshes_per_frame: 96,
    max_textures: 6,
    max_width: 320,
    max_height: 240,
    max_triangles_per_mesh: 3_072,
    max_vertices_per_mesh: 3_072,
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

pub const PROFILE_M33_BALANCED: ProfileCaps = ProfileCaps {
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

pub const DEFAULT_PROFILE_CAPS: ProfileCaps = PROFILE_M33_BALANCED;

/// Resolve the runtime default cap profile.
///
/// Priority:
/// 1. `desktop-unbounded` cargo feature => no caps
/// 2. `EMBEDDED_3DGFX_CAPS=off` => no caps
/// 3. `EMBEDDED_3DGFX_CAPS=m3|m4|m33|m55` => corresponding balanced/perf profile
/// 4. Fallback => `DEFAULT_PROFILE_CAPS` (`PROFILE_M33_BALANCED`)
pub fn default_profile_caps() -> Option<ProfileCaps> {
    if cfg!(feature = "desktop-unbounded") {
        return None;
    }

    #[cfg(feature = "std")]
    {
        if let Ok(raw) = std::env::var("EMBEDDED_3DGFX_CAPS") {
            let value = raw.trim().to_ascii_lowercase();
            return match value.as_str() {
                "off" | "none" | "unbounded" => None,
                "m3" | "m3_balanced" => Some(PROFILE_M3_BALANCED),
                "m4" | "m4_balanced" => Some(PROFILE_M4_BALANCED),
                "m33" | "m33_balanced" => Some(PROFILE_M33_BALANCED),
                "m55" | "m55_perf" => Some(PROFILE_M55_PERF),
                _ => Some(DEFAULT_PROFILE_CAPS),
            };
        }
    }

    Some(DEFAULT_PROFILE_CAPS)
}

pub fn apply_default_caps(engine: &mut crate::K3dengine) {
    if let Some(caps) = default_profile_caps() {
        engine.set_caps(caps);
    } else {
        engine.clear_caps();
    }
}
