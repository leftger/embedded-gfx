use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::OriginDimensions;
use embedded_graphics_core::pixelcolor::Rgb565;
use nalgebra::{Matrix4, Point3, Vector3, Vector4};

use crate::camera::Camera;
use crate::command_buffer::CommandBuffer;
use crate::error::RenderError;
use crate::mesh::{K3dMesh, RenderMode};
use crate::primitive::DrawPrimitive;

mod pipeline;
mod recording;
mod transform;

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

    /// Add a dynamic point light. Returns `false` when the 16-light limit is reached.
    #[cfg(feature = "lighting")]
    pub fn add_point_light(&mut self, light: crate::lights::PointLight) -> bool {
        self.point_lights.push(light).is_ok()
    }

    /// Remove all dynamic point lights.
    #[cfg(feature = "lighting")]
    pub fn clear_point_lights(&mut self) {
        self.point_lights.clear();
    }

    #[cfg(feature = "lighting")]
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn light_tint_at(&self, world_pos: Point3<f32>) -> Rgb565 {
        pipeline::light_tint_at(self, world_pos)
    }

    #[cfg(feature = "lighting")]
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn add_tint(base: Rgb565, tint: Rgb565) -> Rgb565 {
        pipeline::add_tint(base, tint)
    }

    #[cfg(feature = "lighting")]
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn sector_shaded_color(
        &self,
        base: Rgb565,
        brightness: u8,
        face_center: Point3<f32>,
    ) -> Rgb565 {
        pipeline::sector_shaded_color(self, base, brightness, face_center)
    }

    #[cfg(feature = "lighting")]
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn face_world_center(
        face: &[usize; 3],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
    ) -> Point3<f32> {
        pipeline::face_world_center(face, vertices, model_matrix)
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

    #[inline]
    pub(crate) fn should_cull_mesh(&self, mesh: &K3dMesh) -> bool {
        transform::should_cull_mesh(&self.camera, mesh)
    }

    #[inline(always)]
    pub fn transform_point(
        &self,
        point: &[f32; 3],
        model_matrix: Matrix4<f32>,
    ) -> Option<Point3<i32>> {
        transform::transform_point(&self.camera, self.width, self.height, point, model_matrix)
    }

    /// Project a 3D world position into 2D screen coordinates using the camera's view-projection matrix.
    pub fn project_point(
        &self,
        point: Point3<f32>,
    ) -> Option<embedded_graphics_core::geometry::Point> {
        let arr = [point.x, point.y, point.z];
        let pt3 = self.transform_point(&arr, self.camera.vp_matrix)?;
        Some(embedded_graphics_core::geometry::Point::new(pt3.x, pt3.y))
    }

    #[inline(always)]
    pub fn transform_points<const N: usize>(
        &self,
        indices: &[usize; N],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
    ) -> Option<[Point3<i32>; N]> {
        transform::transform_points(
            &self.camera,
            self.width,
            self.height,
            indices,
            vertices,
            model_matrix,
        )
    }

    #[inline(always)]
    pub fn transform_points_with_w<const N: usize>(
        &self,
        indices: &[usize; N],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
    ) -> Option<([Point3<i32>; N], [f32; N])> {
        transform::transform_points_with_w(
            &self.camera,
            self.width,
            self.height,
            indices,
            vertices,
            model_matrix,
        )
    }

    #[inline]
    pub(crate) fn is_backface(
        &self,
        face: &[usize; 3],
        vertices: &[[f32; 3]],
        model_matrix: Matrix4<f32>,
        world_normal: &Vector3<f32>,
    ) -> bool {
        transform::is_backface(&self.camera, face, vertices, model_matrix, world_normal)
    }

    pub(crate) fn emit_clipped<F>(&self, clip: [Vector4<f32>; 3], color: Rgb565, callback: &mut F)
    where
        F: FnMut(DrawPrimitive),
    {
        transform::emit_clipped(
            &self.camera,
            self.width,
            self.height,
            self.vertex_snap_bits,
            clip,
            color,
            callback,
        );
    }

    #[allow(dead_code)]
    pub(crate) fn render<'a, MS, F>(&self, meshes: MS, callback: F)
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
        F: FnMut(DrawPrimitive),
    {
        pipeline::render(self, meshes, callback);
    }

    pub fn record<'a, MS, const MAX: usize>(
        &self,
        meshes: MS,
        commands: &mut CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<(), RenderError>
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
    {
        recording::record(self, meshes, commands, telemetry)
    }

    pub fn record_drop_shadow<const MAX: usize>(
        &self,
        mesh: &K3dMesh,
        floor_y: f32,
        shadow_radius: f32,
        max_fade_distance: f32,
        shadow_opacity: u8,
        color: Rgb565,
        commands: &mut CommandBuffer<MAX>,
    ) -> Result<(), RenderError> {
        recording::record_drop_shadow(
            self,
            mesh,
            floor_y,
            shadow_radius,
            max_fade_distance,
            shadow_opacity,
            color,
            commands,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn record_one_mesh<'a, const MAX: usize>(
        &self,
        mesh: &'a K3dMesh<'a>,
        commands: &mut CommandBuffer<MAX>,
        first_error: &mut Option<RenderError>,
        visible_meshes: &mut usize,
        used_texture_ids: &mut heapless::Vec<u32, 64>,
    ) -> Result<(), RenderError> {
        recording::record_one_mesh(
            self,
            mesh,
            commands,
            first_error,
            visible_meshes,
            used_texture_ids,
        )
    }

    pub fn record_with_fallback<'a, MS, FS, const MAX: usize>(
        &self,
        primary: MS,
        fallback: FS,
        commands: &mut CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<BudgetFallbackOutcome, RenderError>
    where
        MS: IntoIterator<Item = &'a K3dMesh<'a>>,
        FS: IntoIterator<Item = &'a K3dMesh<'a>>,
    {
        recording::record_with_fallback(self, primary, fallback, commands, telemetry)
    }

    pub fn record_with_degradation<'a, const MAX: usize>(
        &mut self,
        meshes: &[&'a K3dMesh<'a>],
        commands: &mut CommandBuffer<MAX>,
        policy: crate::config::DegradationPolicy<'_>,
        telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
    ) -> Result<DegradationOutcome, RenderError> {
        recording::record_with_degradation(self, meshes, commands, policy, telemetry)
    }

    pub fn execute<D, const MAX: usize>(
        &self,
        fb: &mut D,
        frame: &mut crate::renderer::FrameCtx<'_>,
        commands: &CommandBuffer<MAX>,
        telemetry: Option<&mut crate::telemetry::ExecuteTelemetry>,
    ) -> Result<Option<crate::renderer::DirtyRegion>, RenderError>
    where
        D: DrawTarget<Color = Rgb565> + OriginDimensions,
        D::Error: Debug,
    {
        recording::execute(self, fb, frame, commands, telemetry)
    }

    #[cfg(feature = "textured")]
    pub fn execute_with_textures<D, const MAX: usize, const N: usize>(
        &self,
        fb: &mut D,
        frame: &mut crate::renderer::FrameCtx<'_>,
        commands: &CommandBuffer<MAX>,
        texture_manager: &crate::texture::TextureManager<N>,
        telemetry: Option<&mut crate::telemetry::ExecuteTelemetry>,
    ) -> Result<Option<crate::renderer::DirtyRegion>, RenderError>
    where
        D: DrawTarget<Color = Rgb565> + OriginDimensions,
        D::Error: Debug,
    {
        recording::execute_with_textures(self, fb, frame, commands, texture_manager, telemetry)
    }

    pub fn execute_tiled<D, const MAX: usize, const BIN_CAP: usize>(
        &self,
        fb: &mut D,
        frame: &mut crate::renderer::FrameCtx<'_>,
        commands: &CommandBuffer<MAX>,
        tile: crate::tilebin::TileConfig,
    ) -> Result<crate::tilebin::TileBinStats, RenderError>
    where
        D: DrawTarget<Color = Rgb565> + OriginDimensions,
        D::Error: Debug,
    {
        recording::execute_tiled::<D, MAX, BIN_CAP>(self, fb, frame, commands, tile)
    }

    #[cfg(feature = "gizmos")]
    pub fn record_aabb_gizmo<const MAX: usize>(
        &self,
        aabb: &crate::bounds::Aabb,
        model_matrix: &Matrix4<f32>,
        color: Rgb565,
        commands: &mut CommandBuffer<MAX>,
    ) -> Result<(), RenderError> {
        recording::record_aabb_gizmo(self, aabb, model_matrix, color, commands)
    }

    #[cfg(feature = "gizmos")]
    pub fn record_frustum_gizmo<const MAX: usize>(
        &self,
        color: Rgb565,
        commands: &mut CommandBuffer<MAX>,
    ) -> Result<(), RenderError> {
        recording::record_frustum_gizmo(self, color, commands)
    }
}
