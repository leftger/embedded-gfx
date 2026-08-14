use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::Point;
use nalgebra::{ComplexField, Matrix4, Point2, Point3, Vector3, Vector4};

use crate::camera::Camera;
use crate::command_buffer::{CommandBuffer, RenderCommand};
use crate::config::{MaterialProfile, ProfileCaps, QualityTier};
use crate::draw::{DitherConfig, FogConfig};
use crate::error::{BudgetKind, RenderError};
use crate::mesh::{self, K3dMesh, RenderMode};
use crate::primitive::DrawPrimitive;
use crate::renderer::{DirtyRegion, FrameCtx};
use crate::retro::{LightLevels, PaletteMode, ScreenTint, SkyConfig, StippleMode, TextureMapping};
use crate::tilebin::TileConfig;
use crate::{ZDepth, clear_zbuffer, to_zdepth};

pub struct K3dengine {
    pub camera: Camera,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) caps: Option<crate::config::ProfileCaps>,
    pub(crate) quality_tier: crate::config::QualityTier,
    pub(crate) material_profile: crate::config::MaterialProfile,
    /// Depth-based fog applied during `execute` / `execute_tiled`.
    pub(crate) fog: Option<crate::draw::FogConfig>,
    /// Ordered dithering applied during `execute` / `execute_tiled`.
    pub(crate) dither: Option<crate::draw::DitherConfig>,
    /// Optional NDC snap precision for retro-style vertex jitter.
    pub(crate) vertex_snap_bits: u8,
    /// Texture interpolation mode for textured raster paths.
    pub(crate) texture_mapping: crate::retro::TextureMapping,
    /// Sector brightness behavior.
    pub(crate) light_levels: crate::retro::LightLevels,
    /// Optional stipple mode for textured/lightmapped passes.
    pub(crate) stipple_mode: crate::retro::StippleMode,
    /// Optional full-screen tint blended during rasterization.
    pub(crate) screen_tint: Option<crate::retro::ScreenTint>,
    /// Optional palette quantization.
    pub(crate) palette_mode: crate::retro::PaletteMode,
    /// Optional sky background rendered before scene geometry.
    pub(crate) sky: Option<crate::retro::SkyConfig>,
    /// Runtime point lights (max 16).  Applied at face-centre granularity
    /// during `record` for mesh geometry and at face level for BSP.
    #[cfg(feature = "lighting")]
    pub(crate) point_lights: heapless::Vec<crate::lights::PointLight, 16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetFallbackOutcome {
    pub used_fallback: bool,
    pub primary_budget_error: Option<crate::error::BudgetKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DegradationOutcome {
    pub used_degradation: bool,
    pub steps_applied: usize,
    pub dropped_meshes: usize,
    pub final_quality_tier: crate::config::QualityTier,
    pub primary_budget_error: Option<crate::error::BudgetKind>,
}

impl K3dengine {
    pub fn new(width: u16, height: u16) -> K3dengine {
        K3dengine {
            camera: Camera::new(width as f32 / height as f32),
            width,
            height,
            caps: None,
            quality_tier: crate::config::QualityTier::Balanced,
            material_profile: crate::config::MaterialProfile::Lambert,
            fog: None,
            dither: None,
            vertex_snap_bits: 0,
            texture_mapping: crate::retro::TextureMapping::PerspectiveCorrect,
            light_levels: crate::retro::LightLevels::Linear,
            stipple_mode: crate::retro::StippleMode::Off,
            screen_tint: None,
            palette_mode: crate::retro::PaletteMode::Off,
            sky: None,
            #[cfg(feature = "lighting")]
            point_lights: heapless::Vec::new(),
        }
    }

    /// Enable depth-based fog for subsequent [`execute`][Self::execute] calls.
    pub fn set_fog(&mut self, fog: crate::draw::FogConfig) {
        self.fog = Some(fog);
    }

    /// Disable fog (default state).
    pub fn clear_fog(&mut self) {
        self.fog = None;
    }

    /// Enable ordered dithering for subsequent execute passes.
    pub fn set_dither(&mut self, dither: crate::draw::DitherConfig) {
        self.dither = Some(dither);
    }

    /// Disable ordered dithering.
    pub fn clear_dither(&mut self) {
        self.dither = None;
    }

    /// Set NDC vertex snap precision. `0` disables snapping.
    pub fn set_vertex_snap_bits(&mut self, bits: u8) {
        self.vertex_snap_bits = bits.min(16);
    }

    /// Select texture interpolation mode.
    pub fn set_texture_mapping(&mut self, mapping: crate::retro::TextureMapping) {
        self.texture_mapping = mapping;
    }

    /// Select sector light quantization model.
    pub fn set_light_levels(&mut self, levels: crate::retro::LightLevels) {
        self.light_levels = levels;
    }

    /// Set stipple mode used by textured/lightmapped raster paths.
    pub fn set_stipple_mode(&mut self, mode: crate::retro::StippleMode) {
        self.stipple_mode = mode;
    }

    /// Set an optional full-screen tint.
    pub fn set_screen_tint(&mut self, tint: crate::retro::ScreenTint) {
        self.screen_tint = Some(tint);
    }

    /// Disable full-screen tint.
    pub fn clear_screen_tint(&mut self) {
        self.screen_tint = None;
    }

    /// Set output palette quantization mode.
    pub fn set_palette_mode(&mut self, mode: crate::retro::PaletteMode) {
        self.palette_mode = mode;
    }

    /// Set procedural sky rendering parameters.
    pub fn set_sky(&mut self, sky: crate::retro::SkyConfig) {
        self.sky = Some(sky);
    }

    /// Disable procedural sky rendering.
    pub fn clear_sky(&mut self) {
        self.sky = None;
    }

    /// Apply a coarse retro visual preset.
    pub fn apply_retro_style(&mut self, style: crate::retro::RetroStyle) {
        self.fog = style.fog;
        self.dither = style.dither;
        self.set_vertex_snap_bits(style.vertex_snap_bits);
        self.texture_mapping = style.texture_mapping;
        self.light_levels = style.light_levels;
        self.stipple_mode = style.stipple_mode;
        self.screen_tint = style.screen_tint;
        self.palette_mode = style.palette_mode;
        self.sky = style.sky;
    }

    /// Add a dynamic point light.  Returns `false` when the 16-light limit
    /// is reached.
    #[cfg(feature = "lighting")]
    pub fn add_point_light(&mut self, light: crate::lights::PointLight) -> bool {
        self.point_lights.push(light).is_ok()
    }

    /// Remove all dynamic point lights.
    #[cfg(feature = "lighting")]
    pub fn clear_point_lights(&mut self) {
        self.point_lights.clear();
    }

    /// Compute the summed additive RGB565 tint from all registered point
    /// lights at `world_pos`.
    #[cfg(feature = "lighting")]
    #[inline]
    pub(crate) fn light_tint_at(&self, world_pos: Point3<f32>) -> Rgb565 {
        #[cfg(feature = "lighting")]
        {
            let mut acc_r = 0u16;
            let mut acc_g = 0u16;
            let mut acc_b = 0u16;
            for light in &self.point_lights {
                let tint = light.contribution_at(world_pos);
                acc_r += tint.r() as u16;
                acc_g += tint.g() as u16;
                acc_b += tint.b() as u16;
            }
            Rgb565::new(
                (acc_r.min(31)) as u8,
                (acc_g.min(63)) as u8,
                (acc_b.min(31)) as u8,
            )
        }
        #[cfg(not(feature = "lighting"))]
        {
            let _ = world_pos;
            Rgb565::new(0, 0, 0)
        }
    }

    /// Additively blend `tint` into `base`, saturating per channel.
    #[inline]
    pub(crate) fn add_tint(base: Rgb565, tint: Rgb565) -> Rgb565 {
        Rgb565::new(
            (base.r() as u16 + tint.r() as u16).min(31) as u8,
            (base.g() as u16 + tint.g() as u16).min(63) as u8,
            (base.b() as u16 + tint.b() as u16).min(31) as u8,
        )
    }

    /// Non-linear 32-level light ramp for Doom-style sector attenuation.
    #[cfg(feature = "lighting")]
    const DOOM_LIGHT_TABLE: [u8; 32] = [
        8, 12, 16, 20, 24, 28, 34, 40, 48, 56, 64, 72, 82, 92, 102, 112, 124, 136, 148, 160, 172,
        184, 196, 206, 216, 224, 232, 238, 244, 248, 252, 255,
    ];

    #[cfg(feature = "lighting")]
    #[inline]
    pub(crate) fn sector_shaded_color(
        &self,
        base: Rgb565,
        brightness: u8,
        face_center: Point3<f32>,
    ) -> Rgb565 {
        let level_u8 = match self.light_levels {
            crate::retro::LightLevels::Linear => brightness,
            crate::retro::LightLevels::Doom32 => {
                let base_level = (brightness as usize * 31) / 255;
                let distance = (face_center - self.camera.position).norm();
                // 2.0 buckets per world-unit gives coarse banding similar to classic software renderers.
                let distance_drop = (distance * 2.0) as usize;
                let idx = base_level.saturating_sub(distance_drop).min(31);
                Self::DOOM_LIGHT_TABLE[idx]
            }
        };

        let factor = level_u8 as f32 / 255.0;
        Rgb565::new(
            (base.r() as f32 * factor) as u8,
            (base.g() as f32 * factor) as u8,
            (base.b() as f32 * factor) as u8,
        )
    }

    /// Compute the world-space centroid of a triangle face.
    #[cfg(feature = "lighting")]
    #[inline]
    pub(crate) fn face_world_center(
        face: &[usize; 3],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
    ) -> Point3<f32> {
        let v0 = vertices[face[0]];
        let v1 = vertices[face[1]];
        let v2 = vertices[face[2]];
        let cx = (v0[0] + v1[0] + v2[0]) / 3.0;
        let cy = (v0[1] + v1[1] + v2[1]) / 3.0;
        let cz = (v0[2] + v1[2] + v2[2]) / 3.0;
        model_matrix.transform_point(&Point3::new(cx, cy, cz))
    }

    pub fn set_caps(&mut self, caps: crate::config::ProfileCaps) {
        self.caps = Some(caps);
        self.apply_render_defaults(crate::config::render_defaults_for_profile(caps));
    }

    pub fn clear_caps(&mut self) {
        self.caps = None;
    }

    pub fn set_quality_tier(&mut self, tier: crate::config::QualityTier) {
        self.quality_tier = tier;
    }

    pub fn set_material_profile(&mut self, profile: crate::config::MaterialProfile) {
        self.material_profile = profile;
    }

    pub fn apply_render_defaults(&mut self, defaults: crate::config::RenderDefaults) {
        self.quality_tier = defaults.quality_tier;
        self.material_profile = defaults.material_profile;
    }

    pub(crate) fn resolve_render_mode(&self, mode: &RenderMode) -> RenderMode {
        #[cfg(not(feature = "lighting"))]
        {
            let _ = self;
            mode.clone()
        }
        #[cfg(feature = "lighting")]
        {
            use crate::config::{MaterialProfile, QualityTier};
            match self.quality_tier {
                QualityTier::Fastest => match mode {
                    #[cfg(feature = "lighting")]
                    RenderMode::BlinnPhong { .. }
                    | RenderMode::GouraudLightDir(_)
                    | RenderMode::Toon(_, _)
                    | RenderMode::SolidLightDir(_) => RenderMode::Solid,
                    _ => mode.clone(),
                },
                QualityTier::Balanced => match (self.material_profile, mode) {
                    (MaterialProfile::Unlit, RenderMode::BlinnPhong { .. })
                    | (MaterialProfile::Unlit, RenderMode::GouraudLightDir(_))
                    | (MaterialProfile::Unlit, RenderMode::Toon(_, _))
                    | (MaterialProfile::Unlit, RenderMode::SolidLightDir(_)) => RenderMode::Solid,
                    (MaterialProfile::Lambert, RenderMode::BlinnPhong { light_dir, .. }) => {
                        RenderMode::SolidLightDir(*light_dir)
                    }
                    _ => mode.clone(),
                },
                QualityTier::Quality => match (self.material_profile, mode) {
                    (MaterialProfile::Unlit, RenderMode::BlinnPhong { .. })
                    | (MaterialProfile::Unlit, RenderMode::GouraudLightDir(_))
                    | (MaterialProfile::Unlit, RenderMode::Toon(_, _))
                    | (MaterialProfile::Unlit, RenderMode::SolidLightDir(_)) => RenderMode::Solid,
                    (MaterialProfile::Lambert, RenderMode::BlinnPhong { light_dir, .. }) => {
                        RenderMode::SolidLightDir(*light_dir)
                    }
                    _ => mode.clone(),
                },
            }
        }
    }

    /// Frustum culling. Returns true if the mesh should be culled.
    #[inline]
    pub(crate) fn should_cull_mesh(&self, mesh: &K3dMesh) -> bool {
        #[cfg(feature = "render-layers")]
        if !self.camera.layers.intersects(mesh.layers) {
            return true;
        }

        #[cfg(feature = "aabb-cull")]
        {
            let aabb = mesh.model_aabb();
            let world_center = mesh
                .model_matrix
                .transform_point(&nalgebra::Point3::from(aabb.center));
            let scale = mesh.similarity.scaling();
            let radius = aabb.radius() * scale;
            let m = self.camera.vp_matrix;
            let planes = Self::frustum_planes_from_vp(&m);

            for plane in &planes {
                let (a, b, c, d) = (plane[0], plane[1], plane[2], plane[3]);
                let len = (a * a + b * b + c * c).sqrt();
                if len <= 0.0 {
                    continue;
                }
                let dist = (a * world_center.x + b * world_center.y + c * world_center.z + d) / len;
                if dist < -radius {
                    return true;
                }
            }
            for plane in &planes {
                let (a, b, c, d) = (plane[0], plane[1], plane[2], plane[3]);
                let len = (a * a + b * b + c * c).sqrt();
                if len <= 0.0 {
                    continue;
                }
                if aabb.plane_signed_overshoot(a, b, c, d, len, &mesh.model_matrix) < 0.0 {
                    return true;
                }
            }
            return false;
        }

        #[cfg(not(feature = "aabb-cull"))]
        {
            let mesh_pos = mesh.get_position();
            let radius_sq = mesh.compute_bounding_radius_sq();
            let radius = radius_sq.sqrt();
            let planes = Self::frustum_planes_from_vp(&self.camera.vp_matrix);
            for plane in &planes {
                let (a, b, c, d) = (plane[0], plane[1], plane[2], plane[3]);
                let len = (a * a + b * b + c * c).sqrt();
                if len > 0.0 {
                    let dist = (a * mesh_pos.x + b * mesh_pos.y + c * mesh_pos.z + d) / len;
                    if dist < -radius {
                        return true;
                    }
                }
            }
            false
        }
    }

    #[inline]
    fn frustum_planes_from_vp(m: &Matrix4<f32>) -> [[f32; 4]; 6] {
        [
            [
                m[(3, 0)] + m[(0, 0)],
                m[(3, 1)] + m[(0, 1)],
                m[(3, 2)] + m[(0, 2)],
                m[(3, 3)] + m[(0, 3)],
            ],
            [
                m[(3, 0)] - m[(0, 0)],
                m[(3, 1)] - m[(0, 1)],
                m[(3, 2)] - m[(0, 2)],
                m[(3, 3)] - m[(0, 3)],
            ],
            [
                m[(3, 0)] + m[(1, 0)],
                m[(3, 1)] + m[(1, 1)],
                m[(3, 2)] + m[(1, 2)],
                m[(3, 3)] + m[(1, 3)],
            ],
            [
                m[(3, 0)] - m[(1, 0)],
                m[(3, 1)] - m[(1, 1)],
                m[(3, 2)] - m[(1, 2)],
                m[(3, 3)] - m[(1, 3)],
            ],
            [
                m[(3, 0)] + m[(2, 0)],
                m[(3, 1)] + m[(2, 1)],
                m[(3, 2)] + m[(2, 2)],
                m[(3, 3)] + m[(2, 3)],
            ],
            [
                m[(3, 0)] - m[(2, 0)],
                m[(3, 1)] - m[(2, 1)],
                m[(3, 2)] - m[(2, 2)],
                m[(3, 3)] - m[(2, 3)],
            ],
        ]
    }

    #[inline(always)]
    fn transform_point(&self, point: &[f32; 3], model_matrix: Matrix4<f32>) -> Option<Point3<i32>> {
        #[cfg(feature = "fixed-transform")]
        {
            return self.transform_point_fixed(point, model_matrix);
        }
        #[cfg(not(feature = "fixed-transform"))]
        {
            let point = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
            let point = model_matrix * point;

            if point.w < 0.0 {
                return None;
            }
            // `point.w` is the view-space depth (distance along the view
            // direction) for a standard perspective projection, so this is
            // a proper "is the view depth within [near, far]" test. This
            // used to compare pre-divide clip.z instead, which is not
            // linear in view depth -- the "safe" window it accepted started
            // well above `near` itself, silently culling geometry closer to
            // the camera than roughly the midpoint of [near, far]. Matches
            // the fix already applied to `transform_point_with_w`.
            if point.w < self.camera.near || point.w > self.camera.far {
                return None;
            }

            let point = Point3::from_homogeneous(point)?;

            let x = ((1.0 + point.x) * 0.5 * self.width as f32) as i32;
            let y = ((1.0 - point.y) * 0.5 * self.height as f32) as i32;

            if x < 0 || x >= self.width as i32 || y < 0 || y >= self.height as i32 {
                return None;
            }

            Some(Point3::new(
                x,
                y,
                (point.z * (self.camera.far - self.camera.near) + self.camera.near) as i32,
            ))
        }
    }

    #[cfg(feature = "fixed-transform")]
    #[inline(always)]
    fn transform_point_fixed(
        &self,
        point: &[f32; 3],
        model_matrix: Matrix4<f32>,
    ) -> Option<Point3<i32>> {
        use embedded_dsp::fixed_point::{Q16, from_q16, to_q16};

        // Q16.16 division that returns `None` on a zero divisor, matching
        // the early-return-via-`?` control flow below (`div_q16` itself
        // just returns 0 on divide-by-zero).
        #[inline(always)]
        fn div_checked(a: Q16, b: Q16) -> Option<Q16> {
            if b == 0 {
                None
            } else {
                Some(embedded_dsp::fixed_point::div_q16(a, b))
            }
        }

        let point = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
        let point = model_matrix * point;

        if point.w <= 0.0 {
            return None;
        }
        // Same fix as `transform_point`/`transform_point_with_w`: test the
        // view-space depth (`point.w`) against `[near, far]`, not the
        // post-divide NDC z (which is confined to roughly `[-1, 1]` and can
        // never satisfy a "world scale" near/far pair).
        if point.w < self.camera.near || point.w > self.camera.far {
            return None;
        }

        let x_fp = div_checked(to_q16(point.x), to_q16(point.w))?;
        let y_fp = div_checked(to_q16(point.y), to_q16(point.w))?;
        let z_ndc = from_q16(div_checked(to_q16(point.z), to_q16(point.w))?);

        let x = ((1.0 + from_q16(x_fp)) * 0.5 * self.width as f32) as i32;
        let y = ((1.0 - from_q16(y_fp)) * 0.5 * self.height as f32) as i32;

        if x < 0 || x >= self.width as i32 || y < 0 || y >= self.height as i32 {
            return None;
        }

        Some(Point3::new(
            x,
            y,
            (z_ndc * (self.camera.far - self.camera.near) + self.camera.near) as i32,
        ))
    }

    #[inline(always)]
    pub fn transform_points<const N: usize>(
        &self,
        indices: &[usize; N],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
    ) -> Option<[Point3<i32>; N]> {
        let mut ret = [Point3::new(0, 0, 0); N];

        for i in 0..N {
            ret[i] = self.transform_point(&vertices[indices[i]], model_matrix)?;
        }

        Some(ret)
    }

    /// Like `transform_point` but also returns the clip-space W for perspective-correct interpolation.
    /// Returns (screen_point, w_clip). w_clip is the clip-space W before perspective division.
    fn transform_point_with_w(
        &self,
        point: &[f32; 3],
        model_matrix: Matrix4<f32>,
    ) -> Option<(Point3<i32>, f32)> {
        let v = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
        let clip = model_matrix * v;
        // clip.w is the view-space depth (distance along the view direction).
        // Previously this compared ndc_z (range -1..+1) against camera.near/far
        // (world-space values like 0.4 and 20.0), which is a unit mismatch that
        // caused close particles to bleed through unrendered geometry.
        if clip.w < self.camera.near || clip.w > self.camera.far {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let ndc_z = clip.z / clip.w;
        let x = ((1.0 + ndc_x) * 0.5 * self.width as f32) as i32;
        let y = ((1.0 - ndc_y) * 0.5 * self.height as f32) as i32;
        if x < 0 || x >= self.width as i32 || y < 0 || y >= self.height as i32 {
            return None;
        }
        let z = (ndc_z * (self.camera.far - self.camera.near) + self.camera.near) as i32;
        Some((Point3::new(x, y, z), clip.w))
    }

    /// Like `transform_points` but also returns clip-space W values for perspective-correct UV.
    #[inline(always)]
    pub fn transform_points_with_w<const N: usize>(
        &self,
        indices: &[usize; N],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
    ) -> Option<([Point3<i32>; N], [f32; N])> {
        let mut pts = [Point3::new(0, 0, 0); N];
        let mut ws = [1.0f32; N];
        for i in 0..N {
            let (p, w) = self.transform_point_with_w(&vertices[indices[i]], model_matrix)?;
            pts[i] = p;
            ws[i] = w;
        }
        Some((pts, ws))
    }

    /// Position-based backface cull.
    ///
    /// Returns `true` when the face should be skipped (camera is on the
    /// back/outer side of the surface).
    ///
    /// Using the camera *position* rather than *direction* means the result is
    /// independent of where the camera looks — a horizontal floor stays visible
    /// even when the camera pitches up or down.  The direction-based test
    /// (`camera.get_direction() · normal`) incorrectly flips sign as soon as
    /// pitch ≠ 0, culling the floor on upward tilt and the ceiling on downward.
    #[inline]
    pub(crate) fn is_backface(
        &self,
        face: &[usize; 3],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
        world_normal: &Vector3<f32>,
    ) -> bool {
        let v0 = vertices[face[0]];
        let v0_world = if model_matrix == Matrix4::identity() {
            Point3::new(v0[0], v0[1], v0[2])
        } else {
            model_matrix.transform_point(&Point3::new(v0[0], v0[1], v0[2]))
        };
        (self.camera.position - v0_world).dot(world_normal) < 0.0
    }

    // ── Near-plane triangle clipping ──────────────────────────────────────────
    //
    // The engine's vertex-by-vertex `transform_point` returns `None` for any
    // vertex that projects outside the screen, causing the whole triangle to
    // be dropped.  For interior scenes (floor, ceiling, walls close to the
    // camera) this makes large surfaces invisible.
    //
    // The fix is a one-plane Sutherland-Hodgman clip against w = CLIP_NEAR_W
    // before perspective divide.  Clipped triangles are emitted directly after
    // projection; the rasterizer's existing scanline bounds-checks handle any
    // remaining X/Y screen overshoot safely.

    /// Project a single homogeneous clip-space vertex to integer screen coords.
    /// Unlike `transform_point`, this does NOT reject off-screen X/Y — the
    /// rasterizer clips scanlines to screen bounds already.
    /// Screen coordinates are guard-banded to ±8× the framebuffer dimension so
    /// the rasterizer scanline loop is always bounded even for near-plane clips.
    #[inline]
    fn clip_to_screen(&self, c: Vector4<f32>) -> Option<Point3<i32>> {
        if c.w <= 0.0 {
            return None;
        }
        let mut ndc = Point3::from_homogeneous(c)?;
        if self.vertex_snap_bits > 0 {
            let scale = (1u32 << self.vertex_snap_bits) as f32;
            ndc.x = (ndc.x * scale).round() / scale;
            ndc.y = (ndc.y * scale).round() / scale;
        }
        let w = self.width as f32;
        let h = self.height as f32;
        let x = ((1.0 + ndc.x) * 0.5 * w).clamp(-w * 8.0, w * 9.0) as i32;
        let y = ((1.0 - ndc.y) * 0.5 * h).clamp(-h * 8.0, h * 9.0) as i32;
        let depth = (ndc.z * (self.camera.far - self.camera.near) + self.camera.near) as i32;
        Some(Point3::new(x, y, depth))
    }

    /// Project three clip-space vertices and emit one `ColoredTriangleWithDepth`
    /// command if all three project successfully.
    #[inline]
    fn project_and_emit<F>(
        &self,
        c0: Vector4<f32>,
        c1: Vector4<f32>,
        c2: Vector4<f32>,
        color: Rgb565,
        callback: &mut F,
    ) where
        F: FnMut(DrawPrimitive),
    {
        if let (Some(p0), Some(p1), Some(p2)) = (
            self.clip_to_screen(c0),
            self.clip_to_screen(c1),
            self.clip_to_screen(c2),
        ) {
            callback(DrawPrimitive::ColoredTriangleWithDepth {
                points: [p0.xy(), p1.xy(), p2.xy()],
                depths: [p0.z as f32, p1.z as f32, p2.z as f32],
                color,
            });
        }
    }

    /// One Sutherland-Hodgman pass against a single clip-space plane.
    ///
    /// `dist(v) >= 0.0` means the vertex is on the inside of the plane.
    /// Returns the number of vertices written into `output`.
    fn clip_polygon_plane(
        input: &[Vector4<f32>],
        output: &mut [Vector4<f32>; 8],
        dist: impl Fn(Vector4<f32>) -> f32,
    ) -> usize {
        let n = input.len();
        let mut m = 0usize;
        for i in 0..n {
            let prev = input[(n + i - 1) % n];
            let curr = input[i];
            let d_prev = dist(prev);
            let d_curr = dist(curr);
            if d_curr >= 0.0 {
                if d_prev < 0.0 {
                    // Crossing from outside → inside: emit the boundary vertex.
                    let t = d_prev / (d_prev - d_curr);
                    if m < 8 {
                        output[m] = prev + (curr - prev) * t;
                        m += 1;
                    }
                }
                if m < 8 {
                    output[m] = curr;
                    m += 1;
                }
            } else if d_prev >= 0.0 {
                // Crossing from inside → outside: emit the boundary vertex.
                let t = d_prev / (d_prev - d_curr);
                if m < 8 {
                    output[m] = prev + (curr - prev) * t;
                    m += 1;
                }
            }
        }
        m
    }

    /// Clip a triangle against all 5 frustum planes and emit the resulting
    /// fan of screen-space triangles.
    ///
    /// Uses a full Sutherland-Hodgman pass in clip space (near, left, right,
    /// bottom, top).  Clipping against only the near plane left NDC x/y values
    /// like ±13 for near-clipped vertices touching a wall at a grazing angle,
    /// causing the projected triangle to cover only a sliver instead of the
    /// full wall — producing the black triangular holes.
    pub(crate) fn emit_clipped<F>(&self, clip: [Vector4<f32>; 3], color: Rgb565, callback: &mut F)
    where
        F: FnMut(DrawPrimitive),
    {
        let nw = self.camera.near;

        let mut a = [Vector4::zeros(); 8];
        let mut b = [Vector4::zeros(); 8];
        a[0] = clip[0];
        a[1] = clip[1];
        a[2] = clip[2];

        // near:   w >= nw
        let n = Self::clip_polygon_plane(&a[..3], &mut b, |v| v.w - nw);
        if n < 3 {
            return;
        }
        // left:   x >= -w  →  x + w >= 0
        let n = Self::clip_polygon_plane(&b[..n], &mut a, |v| v.x + v.w);
        if n < 3 {
            return;
        }
        // right:  x <=  w  →  w - x >= 0
        let n = Self::clip_polygon_plane(&a[..n], &mut b, |v| v.w - v.x);
        if n < 3 {
            return;
        }
        // bottom: y >= -w  →  y + w >= 0
        let n = Self::clip_polygon_plane(&b[..n], &mut a, |v| v.y + v.w);
        if n < 3 {
            return;
        }
        // top:    y <=  w  →  w - y >= 0
        let n = Self::clip_polygon_plane(&a[..n], &mut b, |v| v.w - v.y);
        if n < 3 {
            return;
        }

        // Triangulate the clipped polygon as a fan from vertex 0.
        for i in 1..n - 1 {
            self.project_and_emit(b[0], b[i], b[i + 1], color, callback);
        }
    }

    fn render<'a, MS, F>(&self, meshes: MS, mut callback: F)
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
        F: FnMut(DrawPrimitive),
    {
        for mesh in meshes {
            if mesh.geometry.vertices.is_empty() {
                continue;
            }

            // Frustum culling: Skip meshes that are completely outside the view frustum
            // This can improve performance by 50-90% by avoiding transformation and rendering
            // of off-screen objects
            if self.should_cull_mesh(mesh) {
                continue;
            }

            // LOD Selection: Choose geometry based on distance from camera
            let mesh_pos = mesh.get_position();
            let distance = (mesh_pos - self.camera.position).norm();
            let geometry = mesh.select_lod(distance);
            #[cfg(feature = "lod-crossfade")]
            let alpha_override = mesh.draw_alpha.get();
            #[cfg(feature = "lod-crossfade")]
            let mut emit = |prim: DrawPrimitive| {
                let prim = match alpha_override {
                    Some(a) => apply_draw_alpha(prim, a),
                    None => prim,
                };
                callback(prim);
            };
            #[cfg(not(feature = "lod-crossfade"))]
            let mut emit = |prim: DrawPrimitive| {
                callback(prim);
            };

            let transform_matrix = self.camera.vp_matrix * mesh.model_matrix;

            #[cfg(feature = "textured")]
            let is_textured = matches!(
                self.resolve_render_mode(&mesh.render_mode),
                RenderMode::Textured | RenderMode::TexturedGouraud(_) | RenderMode::MatCap
            );
            #[cfg(not(feature = "textured"))]
            let is_textured = false;

            let mut v_cache_plain: [Option<Point3<i32>>; 256] = [None; 256];
            #[cfg(feature = "textured")]
            let mut v_cache_w: [Option<(Point3<i32>, f32)>; 256] = [None; 256];

            let cache_limit = geometry.vertices.len().min(256);
            if is_textured {
                #[cfg(feature = "textured")]
                for i in 0..cache_limit {
                    v_cache_w[i] =
                        self.transform_point_with_w(&geometry.vertices[i], transform_matrix);
                }
            } else {
                for i in 0..cache_limit {
                    v_cache_plain[i] =
                        self.transform_point(&geometry.vertices[i], transform_matrix);
                }
            }

            let mut get_pt = |idx: usize| -> Option<Point3<i32>> {
                if idx < 256 {
                    v_cache_plain[idx]
                } else {
                    self.transform_point(&geometry.vertices[idx], transform_matrix)
                }
            };

            #[cfg(feature = "textured")]
            let get_pt_w = |idx: usize| -> Option<(Point3<i32>, f32)> {
                if idx < 256 {
                    v_cache_w[idx]
                } else {
                    self.transform_point_with_w(&geometry.vertices[idx], transform_matrix)
                }
            };

            let tf_face = |face: &[usize; 3]| -> Option<[Point3<i32>; 3]> {
                Some([get_pt(face[0])?, get_pt(face[1])?, get_pt(face[2])?])
            };

            #[cfg(feature = "textured")]
            let tf_face_w = |face: &[usize; 3]| -> Option<([Point3<i32>; 3], [f32; 3])> {
                let (p0, w0) = get_pt_w(face[0])?;
                let (p1, w1) = get_pt_w(face[1])?;
                let (p2, w2) = get_pt_w(face[2])?;
                Some(([p0, p1, p2], [w0, w1, w2]))
            };

            if let Some(out_color) = mesh.outline_color {
                if mesh.outline_width > 0.0 {
                    let has_vertex_normals = !geometry.vertex_normals.is_empty();
                    let has_face_normals = !geometry.normals.is_empty();

                    for (face_idx, face) in geometry.faces.iter().enumerate() {
                        let face_normal = if has_face_normals {
                            Vector3::new(
                                geometry.normals[face_idx][0],
                                geometry.normals[face_idx][1],
                                geometry.normals[face_idx][2],
                            )
                        } else {
                            let v0 = Vector3::new(
                                geometry.vertices[face[0]][0],
                                geometry.vertices[face[0]][1],
                                geometry.vertices[face[0]][2],
                            );
                            let v1 = Vector3::new(
                                geometry.vertices[face[1]][0],
                                geometry.vertices[face[1]][1],
                                geometry.vertices[face[1]][2],
                            );
                            let v2 = Vector3::new(
                                geometry.vertices[face[2]][0],
                                geometry.vertices[face[2]][1],
                                geometry.vertices[face[2]][2],
                            );
                            (v1 - v0).cross(&(v2 - v0)).normalize()
                        };

                        let transformed_normal = mesh.model_matrix.transform_vector(&face_normal);
                        if !self.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }

                        let mut pts = [Point3::origin(); 3];
                        let mut valid = true;
                        for i in 0..3 {
                            let vn = if has_vertex_normals {
                                Vector3::new(
                                    geometry.vertex_normals[face[i]][0],
                                    geometry.vertex_normals[face[i]][1],
                                    geometry.vertex_normals[face[i]][2],
                                )
                            } else {
                                face_normal
                            };
                            let vpos = geometry.vertices[face[i]];
                            let ext_v = [
                                vpos[0] + vn.x * mesh.outline_width,
                                vpos[1] + vn.y * mesh.outline_width,
                                vpos[2] + vn.z * mesh.outline_width,
                            ];
                            if let Some(pt) = self.transform_point(&ext_v, transform_matrix) {
                                pts[i] = pt;
                            } else {
                                valid = false;
                                break;
                            }
                        }

                        if valid {
                            emit(DrawPrimitive::ColoredTriangleWithDepth {
                                points: [pts[0].xy(), pts[1].xy(), pts[2].xy()],
                                depths: [pts[0].z as f32, pts[1].z as f32, pts[2].z as f32],
                                color: out_color,
                            });
                        }
                    }
                }
            }

            let render_mode = self.resolve_render_mode(&mesh.render_mode);
            match render_mode {
                RenderMode::Points => {
                    let screen_space_points = (0..geometry.vertices.len()).filter_map(&mut get_pt);

                    if geometry.colors.len() == geometry.vertices.len() {
                        for (point, color) in screen_space_points.zip(geometry.colors) {
                            emit(DrawPrimitive::ColoredPoint(point.xy(), *color));
                        }
                    } else {
                        for point in screen_space_points {
                            emit(DrawPrimitive::ColoredPoint(point.xy(), mesh.color));
                        }
                    }
                }

                RenderMode::Lines if !geometry.lines.is_empty() => {
                    for line in geometry.lines {
                        if let (Some(p1), Some(p2)) = (get_pt(line[0]), get_pt(line[1])) {
                            emit(DrawPrimitive::Line([p1.xy(), p2.xy()], mesh.color));
                        }
                    }
                }

                RenderMode::Lines if !geometry.faces.is_empty() => {
                    for face in geometry.faces {
                        if let Some([p1, p2, p3]) = tf_face(face) {
                            emit(DrawPrimitive::Line([p1.xy(), p2.xy()], mesh.color));
                            emit(DrawPrimitive::Line([p2.xy(), p3.xy()], mesh.color));
                            emit(DrawPrimitive::Line([p3.xy(), p1.xy()], mesh.color));
                        }
                    }
                }

                RenderMode::Lines => {}

                #[cfg(feature = "lighting")]
                RenderMode::SolidLightDir(direction) => {
                    let color_as_float = Vector3::new(
                        mesh.color.r() as f32 / 32.0,
                        mesh.color.g() as f32 / 64.0,
                        mesh.color.b() as f32 / 32.0,
                    );
                    let ambient_color = color_as_float * 0.1;
                    let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);

                    for (face, normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                        let normal = Vector3::new(normal[0], normal[1], normal[2]);
                        let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                        if self.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }

                        if let Some([p1, p2, p3]) = tf_face(face) {
                            let intensity = transformed_normal.dot(&adjusted_dir).max(0.0);
                            let final_color = color_as_float * intensity + ambient_color;
                            let final_color = Vector3::new(
                                final_color.x.clamp(0.0, 1.0),
                                final_color.y.clamp(0.0, 1.0),
                                final_color.z.clamp(0.0, 1.0),
                            );
                            let mut color = Rgb565::new(
                                (final_color.x * 31.0) as u8,
                                (final_color.y * 63.0) as u8,
                                (final_color.z * 31.0) as u8,
                            );
                            if !self.point_lights.is_empty() {
                                let wc = Self::face_world_center(
                                    face,
                                    geometry.vertices,
                                    mesh.model_matrix,
                                );
                                color = Self::add_tint(color, self.light_tint_at(wc));
                            }
                            emit(DrawPrimitive::ColoredTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                color,
                            });
                        }
                    }
                }

                #[cfg(feature = "lighting")]
                RenderMode::GouraudLightDir(direction) => {
                    let color_as_float = Vector3::new(
                        mesh.color.r() as f32 / 32.0,
                        mesh.color.g() as f32 / 64.0,
                        mesh.color.b() as f32 / 32.0,
                    );
                    let ambient_color = color_as_float * 0.1;
                    let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);

                    for (face, face_normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                        let fn_vec = Vector3::new(face_normal[0], face_normal[1], face_normal[2]);
                        let transformed_fn = mesh.model_matrix.transform_vector(&fn_vec);

                        if self.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_fn,
                        ) {
                            continue;
                        }

                        if let Some([p1, p2, p3]) = tf_face(face) {
                            let vertex_colors: [Rgb565; 3] = core::array::from_fn(|k| {
                                let vn = if !geometry.vertex_normals.is_empty() {
                                    let vn_arr = geometry.vertex_normals[face[k]];
                                    let vn_vec = Vector3::new(vn_arr[0], vn_arr[1], vn_arr[2]);
                                    mesh.model_matrix.transform_vector(&vn_vec)
                                } else {
                                    transformed_fn
                                };

                                let intensity = vn.dot(&adjusted_dir).max(0.0);
                                let c = color_as_float * intensity + ambient_color;
                                let mut vc = Rgb565::new(
                                    (c.x.clamp(0.0, 1.0) * 31.0) as u8,
                                    (c.y.clamp(0.0, 1.0) * 63.0) as u8,
                                    (c.z.clamp(0.0, 1.0) * 31.0) as u8,
                                );
                                if !self.point_lights.is_empty() {
                                    let vpos = geometry.vertices[face[k]];
                                    let wp = mesh
                                        .model_matrix
                                        .transform_point(&Point3::new(vpos[0], vpos[1], vpos[2]));
                                    vc = Self::add_tint(vc, self.light_tint_at(wp));
                                }
                                vc
                            });

                            emit(DrawPrimitive::GouraudTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                colors: vertex_colors,
                            });
                        }
                    }
                }

                #[cfg(feature = "lighting")]
                RenderMode::Toon(direction, bands) => {
                    let color_as_float = Vector3::new(
                        mesh.color.r() as f32 / 32.0,
                        mesh.color.g() as f32 / 64.0,
                        mesh.color.b() as f32 / 32.0,
                    );
                    let ambient_color = color_as_float * 0.15;
                    let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);
                    let bands_f = bands.max(1) as f32;

                    for (face, normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                        let normal = Vector3::new(normal[0], normal[1], normal[2]);
                        let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                        if self.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }

                        if let Some([p1, p2, p3]) = tf_face(face) {
                            let raw_intensity = transformed_normal.dot(&adjusted_dir).max(0.0);
                            let intensity =
                                ((raw_intensity * bands_f).round() / bands_f).clamp(0.0, 1.0);

                            let final_color = color_as_float * intensity + ambient_color;
                            let final_color = Vector3::new(
                                final_color.x.clamp(0.0, 1.0),
                                final_color.y.clamp(0.0, 1.0),
                                final_color.z.clamp(0.0, 1.0),
                            );
                            let mut color = Rgb565::new(
                                (final_color.x * 31.0) as u8,
                                (final_color.y * 63.0) as u8,
                                (final_color.z * 31.0) as u8,
                            );
                            if !self.point_lights.is_empty() {
                                let wc = Self::face_world_center(
                                    face,
                                    geometry.vertices,
                                    mesh.model_matrix,
                                );
                                color = Self::add_tint(color, self.light_tint_at(wc));
                            }
                            emit(DrawPrimitive::ColoredTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                color,
                            });
                        }
                    }
                }

                #[cfg(feature = "lighting")]
                RenderMode::BlinnPhong {
                    light_dir,
                    specular_intensity,
                    shininess,
                } => {
                    // Pre-compute lighting constants (once per mesh, not per face)
                    let color_as_float = Vector3::new(
                        mesh.color.r() as f32 / 32.0,
                        mesh.color.g() as f32 / 64.0,
                        mesh.color.b() as f32 / 32.0,
                    );

                    // Pre-compute ambient lighting term
                    let ambient_color = color_as_float * 0.1;

                    // Pre-compute adjusted light direction
                    // Negate only Z component of direction to fix front/back while keeping left/right
                    let adjusted_light_dir = Vector3::new(light_dir.x, light_dir.y, -light_dir.z);

                    // Normalize light direction
                    let light_dir_normalized = adjusted_light_dir.normalize();

                    for (face, normal) in geometry.faces.iter().zip(geometry.normals.iter()) {
                        //Backface culling
                        let normal = Vector3::new(normal[0], normal[1], normal[2]);
                        let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                        let normalized_normal = transformed_normal.normalize();

                        // Backface culling: cull faces pointing away from camera
                        if self.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &normalized_normal,
                        ) {
                            continue;
                        }

                        if let Some([p1, p2, p3]) = tf_face(face) {
                            // Calculate face center in world space for view direction
                            let v0 = geometry.vertices[face[0]];
                            let v1 = geometry.vertices[face[1]];
                            let v2 = geometry.vertices[face[2]];
                            let face_center = Point3::new(
                                (v0[0] + v1[0] + v2[0]) / 3.0,
                                (v0[1] + v1[1] + v2[1]) / 3.0,
                                (v0[2] + v1[2] + v2[2]) / 3.0,
                            );
                            let face_center_world = mesh.model_matrix.transform_point(&face_center);

                            // View direction: from face to camera
                            let view_dir = (self.camera.position - face_center_world).normalize();

                            // Blinn-Phong half vector: H = normalize(L + V)
                            let half_vector = (light_dir_normalized + view_dir).normalize();

                            // Diffuse term: N·L
                            let diffuse_intensity =
                                normalized_normal.dot(&light_dir_normalized).max(0.0);

                            // Specular term: (N·H)^shininess
                            let specular_term =
                                normalized_normal.dot(&half_vector).max(0.0).powf(shininess);

                            // Compute final color: ambient + diffuse + specular
                            let diffuse_color = color_as_float * diffuse_intensity;
                            let specular_color =
                                Vector3::new(1.0, 1.0, 1.0) * specular_term * specular_intensity;
                            let final_color = ambient_color + diffuse_color + specular_color;

                            let final_color = Vector3::new(
                                final_color.x.clamp(0.0, 1.0),
                                final_color.y.clamp(0.0, 1.0),
                                final_color.z.clamp(0.0, 1.0),
                            );

                            let mut color = Rgb565::new(
                                (final_color.x * 31.0) as u8,
                                (final_color.y * 63.0) as u8,
                                (final_color.z * 31.0) as u8,
                            );
                            if !self.point_lights.is_empty() {
                                color =
                                    Self::add_tint(color, self.light_tint_at(face_center_world));
                            }
                            emit(DrawPrimitive::ColoredTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                color,
                            });
                        }
                    }
                }

                RenderMode::Solid => {
                    if geometry.normals.is_empty() {
                        for face in geometry.faces.iter() {
                            #[cfg(feature = "lighting")]
                            let color = if !self.point_lights.is_empty() {
                                let wc = Self::face_world_center(
                                    face,
                                    geometry.vertices,
                                    mesh.model_matrix,
                                );
                                Self::add_tint(mesh.color, self.light_tint_at(wc))
                            } else {
                                mesh.color
                            };
                            #[cfg(not(feature = "lighting"))]
                            let color = mesh.color;
                            let v = &geometry.vertices;
                            let clip = [
                                transform_matrix
                                    * Vector4::new(
                                        v[face[0]][0],
                                        v[face[0]][1],
                                        v[face[0]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[1]][0],
                                        v[face[1]][1],
                                        v[face[1]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[2]][0],
                                        v[face[2]][1],
                                        v[face[2]][2],
                                        1.0,
                                    ),
                            ];
                            self.emit_clipped(clip, color, &mut callback);
                        }
                    } else {
                        for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                            let normal = Vector3::new(normal[0], normal[1], normal[2]);
                            let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                            if self.is_backface(
                                face,
                                geometry.vertices,
                                mesh.model_matrix,
                                &transformed_normal,
                            ) {
                                continue;
                            }
                            #[cfg(feature = "lighting")]
                            let color = if !self.point_lights.is_empty() {
                                let wc = Self::face_world_center(
                                    face,
                                    geometry.vertices,
                                    mesh.model_matrix,
                                );
                                Self::add_tint(mesh.color, self.light_tint_at(wc))
                            } else {
                                mesh.color
                            };
                            #[cfg(not(feature = "lighting"))]
                            let color = mesh.color;
                            let v = &geometry.vertices;
                            let clip = [
                                transform_matrix
                                    * Vector4::new(
                                        v[face[0]][0],
                                        v[face[0]][1],
                                        v[face[0]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[1]][0],
                                        v[face[1]][1],
                                        v[face[1]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[2]][0],
                                        v[face[2]][1],
                                        v[face[2]][2],
                                        1.0,
                                    ),
                            ];
                            self.emit_clipped(clip, color, &mut callback);
                        }
                    }
                }

                #[cfg(feature = "lighting")]
                RenderMode::SectorBright(brightness) => {
                    if geometry.normals.is_empty() {
                        for face in geometry.faces.iter() {
                            let wc =
                                Self::face_world_center(face, geometry.vertices, mesh.model_matrix);
                            let mut color = self.sector_shaded_color(mesh.color, brightness, wc);
                            if !self.point_lights.is_empty() {
                                color = Self::add_tint(color, self.light_tint_at(wc));
                            }
                            let v = &geometry.vertices;
                            let clip = [
                                transform_matrix
                                    * Vector4::new(
                                        v[face[0]][0],
                                        v[face[0]][1],
                                        v[face[0]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[1]][0],
                                        v[face[1]][1],
                                        v[face[1]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[2]][0],
                                        v[face[2]][1],
                                        v[face[2]][2],
                                        1.0,
                                    ),
                            ];
                            self.emit_clipped(clip, color, &mut callback);
                        }
                    } else {
                        for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                            let normal = Vector3::new(normal[0], normal[1], normal[2]);
                            let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                            if self.is_backface(
                                face,
                                geometry.vertices,
                                mesh.model_matrix,
                                &transformed_normal,
                            ) {
                                continue;
                            }
                            let wc =
                                Self::face_world_center(face, geometry.vertices, mesh.model_matrix);
                            let mut color = self.sector_shaded_color(mesh.color, brightness, wc);
                            if !self.point_lights.is_empty() {
                                color = Self::add_tint(color, self.light_tint_at(wc));
                            }
                            let v = &geometry.vertices;
                            let clip = [
                                transform_matrix
                                    * Vector4::new(
                                        v[face[0]][0],
                                        v[face[0]][1],
                                        v[face[0]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[1]][0],
                                        v[face[1]][1],
                                        v[face[1]][2],
                                        1.0,
                                    ),
                                transform_matrix
                                    * Vector4::new(
                                        v[face[2]][0],
                                        v[face[2]][1],
                                        v[face[2]][2],
                                        1.0,
                                    ),
                            ];
                            self.emit_clipped(clip, color, &mut callback);
                        }
                    }
                }

                #[cfg(feature = "textured")]
                RenderMode::Textured => {
                    // Requires both a texture and per-vertex UVs; silently
                    // skip the mesh (move to the next one) if either is
                    // missing, same graceful-degradation style as `Lines`
                    // with no `lines`/`faces` data.
                    let Some(texture_id) = geometry.texture_id else {
                        continue;
                    };
                    if geometry.uvs.is_empty() {
                        continue;
                    }

                    // Unlike `Solid`, this doesn't route through
                    // `emit_clipped` (which only carries a flat `Rgb565`,
                    // not per-vertex UVs) -- a face with any vertex behind
                    // the near plane or outside the frustum is dropped
                    // whole, same as `GouraudLightDir`/`BlinnPhong`.
                    if geometry.normals.is_empty() {
                        for face in geometry.faces.iter() {
                            if let Some((points, ws)) = tf_face_w(face) {
                                emit(DrawPrimitive::TexturedTriangleWithDepth {
                                    points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                    depths: [
                                        points[0].z as f32,
                                        points[1].z as f32,
                                        points[2].z as f32,
                                    ],
                                    ws,
                                    uvs: [
                                        geometry.uvs[face[0]],
                                        geometry.uvs[face[1]],
                                        geometry.uvs[face[2]],
                                    ],
                                    texture_id,
                                });
                            }
                        }
                    } else {
                        for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                            let normal = Vector3::new(normal[0], normal[1], normal[2]);
                            let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                            if self.is_backface(
                                face,
                                geometry.vertices,
                                mesh.model_matrix,
                                &transformed_normal,
                            ) {
                                continue;
                            }
                            if let Some((points, ws)) = tf_face_w(face) {
                                emit(DrawPrimitive::TexturedTriangleWithDepth {
                                    points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                    depths: [
                                        points[0].z as f32,
                                        points[1].z as f32,
                                        points[2].z as f32,
                                    ],
                                    ws,
                                    uvs: [
                                        geometry.uvs[face[0]],
                                        geometry.uvs[face[1]],
                                        geometry.uvs[face[2]],
                                    ],
                                    texture_id,
                                });
                            }
                        }
                    }
                }
                #[cfg(feature = "textured")]
                RenderMode::TexturedGouraud(direction) => {
                    let Some(texture_id) = geometry.texture_id else {
                        continue;
                    };
                    if geometry.uvs.is_empty() {
                        continue;
                    }
                    let color_as_float = Vector3::new(
                        mesh.color.r() as f32 / 32.0,
                        mesh.color.g() as f32 / 64.0,
                        mesh.color.b() as f32 / 32.0,
                    );
                    let ambient_color = color_as_float * 0.1;
                    let adjusted_dir = Vector3::new(direction.x, direction.y, -direction.z);

                    if geometry.normals.is_empty() {
                        for face in geometry.faces.iter() {
                            if let Some((points, ws)) = tf_face_w(face) {
                                let vertex_colors = [mesh.color, mesh.color, mesh.color];
                                emit(DrawPrimitive::TexturedGouraudTriangleWithDepth {
                                    points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                    depths: [
                                        points[0].z as f32,
                                        points[1].z as f32,
                                        points[2].z as f32,
                                    ],
                                    ws,
                                    uvs: [
                                        geometry.uvs[face[0]],
                                        geometry.uvs[face[1]],
                                        geometry.uvs[face[2]],
                                    ],
                                    colors: vertex_colors,
                                    texture_id,
                                });
                            }
                        }
                    } else {
                        for (face, face_normal) in
                            geometry.faces.iter().zip(geometry.normals.iter())
                        {
                            let fn_vec =
                                Vector3::new(face_normal[0], face_normal[1], face_normal[2]);
                            let transformed_fn = mesh.model_matrix.transform_vector(&fn_vec);

                            if self.is_backface(
                                face,
                                geometry.vertices,
                                mesh.model_matrix,
                                &transformed_fn,
                            ) {
                                continue;
                            }

                            if let Some((points, ws)) = tf_face_w(face) {
                                let vertex_colors: [Rgb565; 3] = core::array::from_fn(|k| {
                                    let vn = if !geometry.vertex_normals.is_empty() {
                                        let vn_arr = geometry.vertex_normals[face[k]];
                                        let vn_vec = Vector3::new(vn_arr[0], vn_arr[1], vn_arr[2]);
                                        mesh.model_matrix.transform_vector(&vn_vec)
                                    } else {
                                        transformed_fn
                                    };

                                    let intensity = vn.dot(&adjusted_dir).max(0.0);
                                    let c = color_as_float * intensity + ambient_color;
                                    let mut vc = Rgb565::new(
                                        (c.x.clamp(0.0, 1.0) * 31.0) as u8,
                                        (c.y.clamp(0.0, 1.0) * 63.0) as u8,
                                        (c.z.clamp(0.0, 1.0) * 31.0) as u8,
                                    );
                                    if !self.point_lights.is_empty() {
                                        let vpos = geometry.vertices[face[k]];
                                        let wp = mesh.model_matrix.transform_point(&Point3::new(
                                            vpos[0], vpos[1], vpos[2],
                                        ));
                                        vc = Self::add_tint(vc, self.light_tint_at(wp));
                                    }
                                    vc
                                });

                                emit(DrawPrimitive::TexturedGouraudTriangleWithDepth {
                                    points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                    depths: [
                                        points[0].z as f32,
                                        points[1].z as f32,
                                        points[2].z as f32,
                                    ],
                                    ws,
                                    uvs: [
                                        geometry.uvs[face[0]],
                                        geometry.uvs[face[1]],
                                        geometry.uvs[face[2]],
                                    ],
                                    colors: vertex_colors,
                                    texture_id,
                                });
                            }
                        }
                    }
                }
                #[cfg(feature = "textured")]
                RenderMode::MatCap => {
                    let Some(texture_id) = geometry.texture_id else {
                        continue;
                    };
                    let has_vertex_normals = !geometry.vertex_normals.is_empty();
                    let has_face_normals = !geometry.normals.is_empty();
                    if !has_vertex_normals && !has_face_normals {
                        continue;
                    }

                    let mv = self.camera.view_matrix * mesh.model_matrix;
                    let normal_matrix = nalgebra::Matrix3::new(
                        mv[(0, 0)],
                        mv[(0, 1)],
                        mv[(0, 2)],
                        mv[(1, 0)],
                        mv[(1, 1)],
                        mv[(1, 2)],
                        mv[(2, 0)],
                        mv[(2, 1)],
                        mv[(2, 2)],
                    );

                    for (face_idx, face) in geometry.faces.iter().enumerate() {
                        let face_normal = if has_face_normals {
                            Vector3::new(
                                geometry.normals[face_idx][0],
                                geometry.normals[face_idx][1],
                                geometry.normals[face_idx][2],
                            )
                        } else {
                            let v0 = Vector3::new(
                                geometry.vertices[face[0]][0],
                                geometry.vertices[face[0]][1],
                                geometry.vertices[face[0]][2],
                            );
                            let v1 = Vector3::new(
                                geometry.vertices[face[1]][0],
                                geometry.vertices[face[1]][1],
                                geometry.vertices[face[1]][2],
                            );
                            let v2 = Vector3::new(
                                geometry.vertices[face[2]][0],
                                geometry.vertices[face[2]][1],
                                geometry.vertices[face[2]][2],
                            );
                            (v1 - v0).cross(&(v2 - v0)).normalize()
                        };

                        let transformed_normal = mesh.model_matrix.transform_vector(&face_normal);
                        if self.is_backface(
                            face,
                            geometry.vertices,
                            mesh.model_matrix,
                            &transformed_normal,
                        ) {
                            continue;
                        }

                        if let Some((points, ws)) = tf_face_w(face) {
                            let mut uvs = [[0.0f32; 2]; 3];
                            for i in 0..3 {
                                let vertex_normal = if has_vertex_normals {
                                    Vector3::new(
                                        geometry.vertex_normals[face[i]][0],
                                        geometry.vertex_normals[face[i]][1],
                                        geometry.vertex_normals[face[i]][2],
                                    )
                                } else {
                                    face_normal
                                };
                                let view_normal = (normal_matrix * vertex_normal).normalize();
                                let u = view_normal.x * 0.5 + 0.5;
                                let v = -view_normal.y * 0.5 + 0.5;
                                uvs[i] = [u, v];
                            }

                            emit(DrawPrimitive::TexturedTriangleWithDepth {
                                points: [points[0].xy(), points[1].xy(), points[2].xy()],
                                depths: [
                                    points[0].z as f32,
                                    points[1].z as f32,
                                    points[2].z as f32,
                                ],
                                ws,
                                uvs,
                                texture_id,
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn record<'a, MS, const MAX: usize>(
        &self,
        meshes: MS,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<(), crate::error::RenderError>
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
    {
        self.record_impl(meshes, commands, telemetry)
    }

    /// Record a projected drop shadow decal for a mesh grounded to a specific floor height.
    /// Uses an 8-sided flat polygon projected via camera view-projection matrix to form 8 translucent triangles.
    pub fn record_drop_shadow<const MAX: usize>(
        &self,
        mesh: &K3dMesh,
        floor_y: f32,
        shadow_radius: f32,
        max_fade_distance: f32,
        shadow_opacity: u8,
        color: Rgb565,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
    ) -> Result<(), crate::error::RenderError> {
        let pos = mesh.get_position();
        let height = pos.y - floor_y;
        if height < 0.0 || height >= max_fade_distance {
            return Ok(());
        }

        let fade = 1.0 - (height / max_fade_distance).clamp(0.0, 1.0);
        let radius = shadow_radius * fade;
        let opacity = (shadow_opacity as f32 * fade) as u8;
        if opacity == 0 {
            return Ok(());
        }

        let y_pos = floor_y + 0.01;
        let center_world = [pos.x, y_pos, pos.z];
        let center_proj = self.transform_point_with_w(&center_world, self.camera.vp_matrix);
        let Some((c_pt, _c_w)) = center_proj else {
            return Ok(());
        };

        let mut outer_proj: [Option<(Point3<i32>, f32)>; 8] = [None; 8];
        for i in 0..8 {
            let angle = (i as f32) * (core::f32::consts::PI / 4.0);
            let px = pos.x + radius * micromath::F32Ext::cos(angle);
            let pz = pos.z + radius * micromath::F32Ext::sin(angle);
            outer_proj[i] = self.transform_point_with_w(&[px, y_pos, pz], self.camera.vp_matrix);
        }

        for i in 0..8 {
            let next_idx = (i + 1) % 8;
            if let (Some((p1, _w1)), Some((p2, _w2))) = (outer_proj[i], outer_proj[next_idx]) {
                commands.push(crate::command_buffer::RenderCommand::Draw(
                    DrawPrimitive::TranslucentTriangleWithDepth {
                        points: [c_pt.xy(), p1.xy(), p2.xy()],
                        depths: [c_pt.z as f32, p1.z as f32, p2.z as f32],
                        color,
                        alpha: opacity,
                    },
                ))?;
            }
        }

        Ok(())
    }

    fn record_impl<'a, MS, const MAX: usize>(
        &self,
        meshes: MS,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<(), crate::error::RenderError>
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
    {
        use crate::command_buffer::RenderCommand;

        commands.clear();
        commands.push(RenderCommand::ClearDepth(crate::Z_MAX_VALUE))?;
        if let Some(caps) = self.caps {
            caps.validate_framebuffer(self.width as usize, self.height as usize)?;
        }

        let mut first_error = None;
        let mut visible_meshes = 0usize;
        let mut used_texture_ids: heapless::Vec<u32, 64> = heapless::Vec::new();
        let mut meshes_total = 0usize;

        #[cfg(feature = "record-sort")]
        {
            let mut sorted: heapless::Vec<(u8, i32, &K3dMesh<'a>), 256> = heapless::Vec::new();
            for mesh in meshes {
                meshes_total += 1;
                if mesh.geometry.vertices.is_empty() {
                    continue;
                }
                if self.should_cull_mesh(mesh) {
                    continue;
                }
                let distance = (mesh.get_position() - self.camera.position).norm();
                let dist_key = (distance * 1000.0) as i32;
                if sorted.push((mesh.priority, dist_key, mesh)).is_err() {
                    break;
                }
            }
            sorted.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

            for &(_, _, mesh) in sorted.iter() {
                Self::record_one_mesh(
                    self,
                    mesh,
                    commands,
                    &mut first_error,
                    &mut visible_meshes,
                    &mut used_texture_ids,
                )?;
                if let Some(err) = first_error.take() {
                    return Err(err);
                }
            }
        }

        #[cfg(not(feature = "record-sort"))]
        {
            for mesh in meshes {
                meshes_total += 1;
                if mesh.geometry.vertices.is_empty() {
                    continue;
                }
                if self.should_cull_mesh(mesh) {
                    continue;
                }
                Self::record_one_mesh(
                    self,
                    mesh,
                    commands,
                    &mut first_error,
                    &mut visible_meshes,
                    &mut used_texture_ids,
                )?;
                if let Some(err) = first_error.take() {
                    return Err(err);
                }
            }
        }

        if let Some(t) = telemetry {
            t.meshes_total = meshes_total;
            t.meshes_visible = visible_meshes;
            t.unique_textures = used_texture_ids.len();
            t.draw_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, RenderCommand::Draw(_)))
                .count();
            t.fallback_used = false;
            t.degradation_steps_applied = 0;
            t.dropped_meshes = 0;
        }

        Ok(())
    }

    fn record_one_mesh<'a, const MAX: usize>(
        &self,
        mesh: &'a K3dMesh<'a>,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
        first_error: &mut Option<crate::error::RenderError>,
        visible_meshes: &mut usize,
        used_texture_ids: &mut heapless::Vec<u32, 64>,
    ) -> Result<(), crate::error::RenderError> {
        use crate::command_buffer::RenderCommand;
        use crate::error::{BudgetKind, RenderError};

        let distance = (mesh.get_position() - self.camera.position).norm();
        let geometry = mesh.select_lod(distance);

        if let Some(caps) = self.caps {
            *visible_meshes += 1;
            if *visible_meshes > caps.max_meshes_per_frame {
                return Err(RenderError::OutOfBudget(BudgetKind::MeshesPerFrame {
                    attempted: *visible_meshes,
                    max: caps.max_meshes_per_frame,
                }));
            }

            if geometry.vertices.len() > caps.max_vertices_per_mesh {
                return Err(RenderError::OutOfBudget(BudgetKind::VerticesPerMesh {
                    attempted: geometry.vertices.len(),
                    max: caps.max_vertices_per_mesh,
                }));
            }

            if geometry.faces.len() > caps.max_triangles_per_mesh {
                return Err(RenderError::OutOfBudget(BudgetKind::TrianglesPerMesh {
                    attempted: geometry.faces.len(),
                    max: caps.max_triangles_per_mesh,
                }));
            }

            if let Some(texture_id) = geometry.texture_id
                && !used_texture_ids.contains(&texture_id)
            {
                let attempted = used_texture_ids.len() + 1;
                if attempted > caps.max_textures {
                    return Err(RenderError::OutOfBudget(BudgetKind::Textures {
                        attempted,
                        max: caps.max_textures,
                    }));
                }

                if used_texture_ids.push(texture_id).is_err() {
                    return Err(RenderError::OutOfBudget(BudgetKind::Textures {
                        attempted,
                        max: caps.max_textures,
                    }));
                }
            }
        } else {
            *visible_meshes += 1;
        }

        let mut push_draw = |primitive: DrawPrimitive, first_error: &mut Option<RenderError>| {
            if first_error.is_none()
                && let Err(e) = commands.push(RenderCommand::Draw(primitive))
            {
                *first_error = Some(e);
            }
        };

        #[cfg(feature = "lod-crossfade")]
        {
            match mesh.select_lod_pick(distance) {
                mesh::LodPick::Single(_) => {
                    mesh.lod_force.set(None);
                    mesh.draw_alpha.set(None);
                    self.render(core::iter::once(mesh), |primitive| {
                        push_draw(primitive, first_error);
                    });
                }
                mesh::LodPick::Crossfade { near, far, t } => {
                    let near_lvl = mesh.lod_level_of(near);
                    let far_lvl = mesh.lod_level_of(far);
                    if t < 0.5 {
                        mesh.lod_force.set(Some(near_lvl));
                        mesh.draw_alpha.set(None);
                        self.render(core::iter::once(mesh), |primitive| {
                            push_draw(primitive, first_error);
                        });
                        let a = (t * 255.0) as u8;
                        if a > 16 {
                            mesh.lod_force.set(Some(far_lvl));
                            mesh.draw_alpha.set(Some(a));
                            self.render(core::iter::once(mesh), |primitive| {
                                push_draw(primitive, first_error);
                            });
                        }
                    } else {
                        mesh.lod_force.set(Some(far_lvl));
                        mesh.draw_alpha.set(None);
                        self.render(core::iter::once(mesh), |primitive| {
                            push_draw(primitive, first_error);
                        });
                        let a = ((1.0 - t) * 255.0) as u8;
                        if a > 16 {
                            mesh.lod_force.set(Some(near_lvl));
                            mesh.draw_alpha.set(Some(a));
                            self.render(core::iter::once(mesh), |primitive| {
                                push_draw(primitive, first_error);
                            });
                        }
                    }
                    mesh.lod_force.set(None);
                    mesh.draw_alpha.set(None);
                }
            }
        }
        #[cfg(not(feature = "lod-crossfade"))]
        {
            self.render(core::iter::once(mesh), |primitive| {
                push_draw(primitive, first_error);
            });
        }
        Ok(())
    }

    pub fn record_with_fallback<'a, MS, FS, const MAX: usize>(
        &self,
        primary: MS,
        fallback: FS,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<BudgetFallbackOutcome, crate::error::RenderError>
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
        FS: IntoIterator<Item = &'a K3dMesh<'a>>,
    {
        use crate::error::RenderError;

        let mut local_telemetry = crate::telemetry::RecordTelemetry::default();
        match self.record_impl(primary, commands, Some(&mut local_telemetry)) {
            Ok(()) => {
                if let Some(t) = telemetry {
                    *t = local_telemetry;
                    t.fallback_used = false;
                }
                Ok(BudgetFallbackOutcome {
                    used_fallback: false,
                    primary_budget_error: None,
                })
            }
            Err(RenderError::OutOfBudget(kind)) => {
                let mut fallback_telemetry = crate::telemetry::RecordTelemetry::default();
                self.record_impl(fallback, commands, Some(&mut fallback_telemetry))?;
                if let Some(t) = telemetry {
                    *t = fallback_telemetry;
                    t.fallback_used = true;
                }
                Ok(BudgetFallbackOutcome {
                    used_fallback: true,
                    primary_budget_error: Some(kind),
                })
            }
            Err(e) => Err(e),
        }
    }

    fn downgraded_quality_tier(tier: crate::config::QualityTier) -> crate::config::QualityTier {
        use crate::config::QualityTier;
        match tier {
            QualityTier::Quality => QualityTier::Balanced,
            QualityTier::Balanced => QualityTier::Fastest,
            QualityTier::Fastest => QualityTier::Fastest,
        }
    }

    pub fn record_with_degradation<'a, const MAX: usize>(
        &mut self,
        meshes: &[&'a K3dMesh<'a>],
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
        policy: crate::config::DegradationPolicy<'_>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<DegradationOutcome, crate::error::RenderError> {
        use crate::config::DegradationStep;
        use crate::error::RenderError;

        let original_quality = self.quality_tier;
        let mut active_quality = self.quality_tier;

        let mut outcome = DegradationOutcome {
            used_degradation: false,
            steps_applied: 0,
            dropped_meshes: 0,
            final_quality_tier: active_quality,
            primary_budget_error: None,
        };

        let mut local_telemetry = crate::telemetry::RecordTelemetry::default();
        match self.record_impl(meshes.iter().copied(), commands, Some(&mut local_telemetry)) {
            Ok(()) => {
                if let Some(t) = telemetry {
                    *t = local_telemetry;
                }
                return Ok(outcome);
            }
            Err(RenderError::OutOfBudget(kind)) => {
                outcome.primary_budget_error = Some(kind);
            }
            Err(e) => return Err(e),
        }

        for step in policy.steps {
            outcome.used_degradation = true;
            outcome.steps_applied += 1;

            let mut selected: heapless::Vec<&K3dMesh<'_>, 512> = heapless::Vec::new();
            match *step {
                DegradationStep::RaisePriorityFloor(min_priority) => {
                    for mesh in meshes {
                        if mesh.priority >= min_priority {
                            let _ = selected.push(*mesh);
                        } else {
                            outcome.dropped_meshes += 1;
                        }
                    }
                }
                DegradationStep::MeshDecimationStride(stride) => {
                    if stride == 0 {
                        self.quality_tier = original_quality;
                        return Err(RenderError::InvalidInput(
                            "mesh decimation stride must be >= 1",
                        ));
                    }
                    for (idx, mesh) in meshes.iter().enumerate() {
                        if idx % stride == 0 {
                            let _ = selected.push(*mesh);
                        } else {
                            outcome.dropped_meshes += 1;
                        }
                    }
                }
                DegradationStep::DowngradeQuality => {
                    active_quality = Self::downgraded_quality_tier(active_quality);
                    self.quality_tier = active_quality;
                    for mesh in meshes {
                        let _ = selected.push(*mesh);
                    }
                }
            }

            if selected.is_empty() {
                continue;
            }

            let mut step_telemetry = crate::telemetry::RecordTelemetry::default();
            let attempt = self.record_impl(
                selected.iter().copied(),
                commands,
                Some(&mut step_telemetry),
            );

            if let Ok(()) = attempt {
                outcome.final_quality_tier = self.quality_tier;
                if let Some(t) = telemetry {
                    *t = step_telemetry;
                    t.fallback_used = true;
                    t.degradation_steps_applied = outcome.steps_applied;
                    t.dropped_meshes = outcome.dropped_meshes;
                }
                self.quality_tier = original_quality;
                return Ok(outcome);
            }
        }

        self.quality_tier = original_quality;
        Err(crate::error::RenderError::Recoverable {
            fault: crate::error::RuntimeFaultKind::Budget(outcome.primary_budget_error.unwrap_or(
                crate::error::BudgetKind::DrawPrimitives {
                    attempted: commands.len(),
                    max: MAX,
                },
            )),
            action: crate::error::RecoveryAction::SkipFrame,
        })
    }

    pub fn execute<D, const MAX: usize>(
        &self,
        fb: &mut D,
        frame: &mut crate::renderer::FrameCtx<'_>,
        commands: &crate::command_buffer::CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::ExecuteTelemetry>,
    ) -> Result<Option<crate::renderer::DirtyRegion>, crate::error::RenderError>
    where
        D: embedded_graphics_core::draw_target::DrawTarget<Color = Rgb565>
            + embedded_graphics_core::prelude::OriginDimensions,
        <D as embedded_graphics_core::draw_target::DrawTarget>::Error: core::fmt::Debug,
    {
        if let Some(t) = telemetry {
            t.commands_total = commands.len();
            t.draw_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, crate::command_buffer::RenderCommand::Draw(_)))
                .count();
            t.clear_color_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, crate::command_buffer::RenderCommand::ClearColor(_)))
                .count();
            t.clear_depth_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, crate::command_buffer::RenderCommand::ClearDepth(_)))
                .count();
        }
        let camera_dir = self.camera.get_direction();
        crate::renderer::execute_commands_with_dirty_region_effects(
            fb,
            frame,
            commands,
            self.fog.as_ref(),
            self.dither.as_ref(),
            self.screen_tint,
            self.stipple_mode,
            self.palette_mode,
            self.sky,
            [camera_dir.x, camera_dir.y, camera_dir.z],
        )
    }

    /// Like [`Self::execute`], but resolves [`RenderMode::Textured`] meshes'
    /// `DrawPrimitive::TexturedTriangleWithDepth`/`LightmappedTriangle`
    /// primitives via `texture_manager` instead of silently dropping them.
    ///
    /// `record()` doesn't need a texture manager (it only transforms
    /// geometry into primitives, it doesn't sample pixels), so a scene
    /// mixing textured and flat-colored/lit meshes still goes through one
    /// `record()` call -- only `execute()` needs to change to
    /// `execute_with_textures()` once any mesh in the batch uses
    /// `RenderMode::Textured`.
    #[cfg(feature = "textured")]
    pub fn execute_with_textures<D, const MAX: usize, const N: usize>(
        &self,
        fb: &mut D,
        frame: &mut crate::renderer::FrameCtx<'_>,
        commands: &crate::command_buffer::CommandBuffer<MAX>,
        texture_manager: &crate::texture::TextureManager<N>,
        telemetry: Option<&mut crate::telemetry::ExecuteTelemetry>,
    ) -> Result<Option<crate::renderer::DirtyRegion>, crate::error::RenderError>
    where
        D: embedded_graphics_core::draw_target::DrawTarget<Color = Rgb565>
            + embedded_graphics_core::prelude::OriginDimensions,
        <D as embedded_graphics_core::draw_target::DrawTarget>::Error: core::fmt::Debug,
    {
        if let Some(t) = telemetry {
            t.commands_total = commands.len();
            t.draw_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, crate::command_buffer::RenderCommand::Draw(_)))
                .count();
            t.clear_color_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, crate::command_buffer::RenderCommand::ClearColor(_)))
                .count();
            t.clear_depth_commands = commands
                .iter()
                .filter(|cmd| matches!(cmd, crate::command_buffer::RenderCommand::ClearDepth(_)))
                .count();
        }
        let camera_dir = self.camera.get_direction();
        crate::renderer::execute_commands_with_dirty_region_effects_textured(
            fb,
            frame,
            commands,
            texture_manager,
            self.fog.as_ref(),
            self.dither.as_ref(),
            self.screen_tint,
            self.stipple_mode,
            self.palette_mode,
            self.sky,
            [camera_dir.x, camera_dir.y, camera_dir.z],
        )
    }

    pub fn execute_tiled<D, const MAX: usize, const BIN_CAP: usize>(
        &self,
        fb: &mut D,
        frame: &mut crate::renderer::FrameCtx<'_>,
        commands: &crate::command_buffer::CommandBuffer<MAX>,
        tile: crate::tilebin::TileConfig,
    ) -> Result<crate::tilebin::TileBinStats, crate::error::RenderError>
    where
        D: embedded_graphics_core::draw_target::DrawTarget<Color = Rgb565>
            + embedded_graphics_core::prelude::OriginDimensions,
        <D as embedded_graphics_core::draw_target::DrawTarget>::Error: core::fmt::Debug,
    {
        let camera_dir = self.camera.get_direction();
        crate::renderer::execute_commands_tiled_effects::<D, MAX, BIN_CAP>(
            fb,
            frame,
            commands,
            tile,
            self.fog.as_ref(),
            self.dither.as_ref(),
            self.screen_tint,
            self.stipple_mode,
            self.palette_mode,
            self.sky,
            [camera_dir.x, camera_dir.y, camera_dir.z],
        )
    }

    /// Emit a model-space AABB wireframe into a command buffer (debug).
    #[cfg(feature = "gizmos")]
    pub fn record_aabb_gizmo<const MAX: usize>(
        &self,
        aabb: &crate::bounds::Aabb,
        model_matrix: &Matrix4<f32>,
        color: Rgb565,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
    ) -> Result<(), crate::error::RenderError> {
        let mut err = None;
        crate::gizmos::emit_aabb_wireframe_projected(
            aabb,
            model_matrix,
            |p| self.transform_point(&p, self.camera.vp_matrix),
            color,
            |prim| {
                if err.is_none()
                    && let Err(e) = commands.push(crate::command_buffer::RenderCommand::Draw(prim))
                {
                    err = Some(e);
                }
            },
        );
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Emit the camera frustum wireframe into a command buffer (debug).
    #[cfg(feature = "gizmos")]
    pub fn record_frustum_gizmo<const MAX: usize>(
        &self,
        color: Rgb565,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
    ) -> Result<(), crate::error::RenderError> {
        let mut err = None;
        crate::gizmos::emit_frustum_wireframe(
            &self.camera,
            |p| self.transform_point(&p, self.camera.vp_matrix),
            color,
            |prim| {
                if err.is_none()
                    && let Err(e) = commands.push(crate::command_buffer::RenderCommand::Draw(prim))
                {
                    err = Some(e);
                }
            },
        );
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(feature = "lod-crossfade")]
fn apply_draw_alpha(prim: DrawPrimitive, alpha: u8) -> DrawPrimitive {
    match prim {
        DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color,
        } => DrawPrimitive::TranslucentTriangleWithDepth {
            points,
            depths,
            color,
            alpha,
        },
        DrawPrimitive::TranslucentTriangleWithDepth {
            points,
            depths,
            color,
            alpha: prev,
        } => DrawPrimitive::TranslucentTriangleWithDepth {
            points,
            depths,
            color,
            alpha: ((prev as u16 * alpha as u16) / 255) as u8,
        },
        other => other,
    }
}
