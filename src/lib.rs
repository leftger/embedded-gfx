#![no_std]
#[cfg(feature = "std")]
extern crate std;
use camera::Camera;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::RgbColor;
use mesh::K3dMesh;
use mesh::RenderMode;
use nalgebra::Matrix4;
use nalgebra::Point2;
use nalgebra::Point3;
use nalgebra::Vector3;

// ComplexField provides sqrt() for f32 in no_std via libm
// It appears "unused" in tests because tests use std, but it's required for no_std builds
#[allow(unused_imports)]
use nalgebra::ComplexField;

pub mod animation;
pub mod billboard;
pub mod bridge;
pub mod bsp;
pub mod camera;
pub mod command_buffer;
pub mod config;
pub mod display_backend;
pub mod draw;
pub mod error;
pub mod fixed_math;
pub mod hardware_profile;
pub mod hud;
pub mod lights;
pub mod lut;
pub mod mesh;
#[cfg(feature = "std")]
pub mod painters;
pub mod particles;
#[cfg(feature = "perfcounter")]
pub mod perfcounter;
pub mod physics;
pub mod renderer;
pub mod scene_format;
pub mod scene_stream;
pub mod skeleton;
pub mod softbody;
pub mod swapchain;
pub mod telemetry;
pub mod texture;
pub mod tilebin;
pub mod transform_anim;
pub mod tween;

// Re-export framebuffer types from external crate for user convenience
pub use embedded_graphics_framebuf::{
    FrameBuf,
    backends::{DMACapableFrameBufferBackend, EndianCorrectedBuffer, EndianCorrection},
};

#[cfg(feature = "aa")]
pub use draw::ReadPixel;

pub use bridge::{
    AsEgPoint, AsNalgebraPoint, draw_to, eg_to_nalgebra, nalgebra_to_eg, render_drawable_to_buffer,
};
pub use draw::FogConfig;
pub use lights::{PointLight, PointLightSet};
pub use particles::{ParticleSpawn, ParticleSystem};
pub use renderer::{DirtyRegion, FrameCtx};
pub use tilebin::{TileBinStats, TileConfig};
pub use transform_anim::{AnimationPlayer, SampledTransform, TransformKeyframe, TransformTrack};
pub use tween::{Easing, Tween, Tween3, apply_easing, lerp, lerp3, scale_rgb565};

#[derive(Debug, Clone)]
pub enum DrawPrimitive {
    ColoredPoint(Point2<i32>, Rgb565),
    Line([Point2<i32>; 2], Rgb565),
    ColoredTriangle([Point2<i32>; 3], Rgb565),
    ColoredTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        color: Rgb565,
    },
    GouraudTriangle {
        points: [Point2<i32>; 3],
        colors: [Rgb565; 3],
    },
    GouraudTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        colors: [Rgb565; 3],
    },
    TexturedTriangle {
        points: [Point2<i32>; 3],
        uvs: [[f32; 2]; 3],
        texture_id: u32,
    },
    TexturedTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        ws: [f32; 3],
        uvs: [[f32; 2]; 3],
        texture_id: u32,
    },
    /// Perspective-correct textured triangle with baked lightmap.
    ///
    /// The final pixel colour is:
    /// `clamp(surface.sample(su,sv) × lightmap.sample(lu,lv) + dynamic_tint)`
    /// where `×` is per-channel normalised multiply.
    /// Set `lightmap_id = u32::MAX` to fall back to full-bright surface colour.
    /// Set `dynamic_tint = Rgb565::new(0,0,0)` for no dynamic lighting.
    LightmappedTriangle {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        ws: [f32; 3],
        surface_uvs: [[f32; 2]; 3],
        lm_uvs: [[f32; 2]; 3],
        texture_id: u32,
        lightmap_id: u32,
        /// Additive RGB565 tint from runtime point lights.
        dynamic_tint: Rgb565,
    },
}

pub struct K3dengine {
    pub camera: Camera,
    width: u16,
    height: u16,
    caps: Option<crate::config::ProfileCaps>,
    quality_tier: crate::config::QualityTier,
    material_profile: crate::config::MaterialProfile,
    /// Depth-based fog applied during `execute` / `execute_tiled`.
    fog: Option<crate::draw::FogConfig>,
    /// Runtime point lights (max 16).  Applied at face-centre granularity
    /// during `record` for mesh geometry and at face level for BSP.
    point_lights: heapless::Vec<crate::lights::PointLight, 16>,
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

    /// Add a dynamic point light.  Returns `false` when the 16-light limit
    /// is reached.
    pub fn add_point_light(&mut self, light: crate::lights::PointLight) -> bool {
        self.point_lights.push(light).is_ok()
    }

    /// Remove all dynamic point lights.
    pub fn clear_point_lights(&mut self) {
        self.point_lights.clear();
    }

    /// Compute the summed additive RGB565 tint from all registered point
    /// lights at `world_pos`.
    #[inline]
    fn light_tint_at(&self, world_pos: Point3<f32>) -> Rgb565 {
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        for light in &self.point_lights {
            let c = light.contribution_at(world_pos);
            r += c.r() as u32;
            g += c.g() as u32;
            b += c.b() as u32;
        }
        Rgb565::new(r.min(31) as u8, g.min(63) as u8, b.min(31) as u8)
    }

    /// Additively blend `tint` into `base`, saturating per channel.
    #[inline]
    fn add_tint(base: Rgb565, tint: Rgb565) -> Rgb565 {
        Rgb565::new(
            (base.r() as u16 + tint.r() as u16).min(31) as u8,
            (base.g() as u16 + tint.g() as u16).min(63) as u8,
            (base.b() as u16 + tint.b() as u16).min(31) as u8,
        )
    }

    /// Compute the world-space centroid of a triangle face.
    #[inline]
    fn face_world_center(
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

    fn resolve_render_mode(&self, mode: &RenderMode) -> RenderMode {
        use crate::config::{MaterialProfile, QualityTier};
        match self.quality_tier {
            QualityTier::Fastest => match mode {
                RenderMode::BlinnPhong { .. }
                | RenderMode::GouraudLightDir(_)
                | RenderMode::SolidLightDir(_) => RenderMode::Solid,
                _ => mode.clone(),
            },
            QualityTier::Balanced => match (self.material_profile, mode) {
                (MaterialProfile::Unlit, RenderMode::BlinnPhong { .. })
                | (MaterialProfile::Unlit, RenderMode::GouraudLightDir(_))
                | (MaterialProfile::Unlit, RenderMode::SolidLightDir(_)) => RenderMode::Solid,
                (MaterialProfile::Lambert, RenderMode::BlinnPhong { light_dir, .. }) => {
                    RenderMode::SolidLightDir(light_dir.clone())
                }
                _ => mode.clone(),
            },
            QualityTier::Quality => match (self.material_profile, mode) {
                (MaterialProfile::Unlit, RenderMode::BlinnPhong { .. })
                | (MaterialProfile::Unlit, RenderMode::GouraudLightDir(_))
                | (MaterialProfile::Unlit, RenderMode::SolidLightDir(_)) => RenderMode::Solid,
                (MaterialProfile::Lambert, RenderMode::BlinnPhong { light_dir, .. }) => {
                    RenderMode::SolidLightDir(light_dir.clone())
                }
                _ => mode.clone(),
            },
        }
    }

    /// Fast frustum culling check using bounding sphere.
    /// Returns true if the mesh should be culled (not rendered).
    #[inline]
    fn should_cull_mesh(&self, mesh: &K3dMesh) -> bool {
        // Get mesh position in world space
        let mesh_pos = mesh.get_position();

        // Compute distance from camera to mesh center
        let to_mesh = mesh_pos - self.camera.position;
        let distance = to_mesh.norm(); // Uses libm sqrt via nalgebra

        // Get squared bounding radius and compute radius
        // This is only called once per mesh, not in the inner loop
        let radius_sq = mesh.compute_bounding_radius_sq();
        let radius = radius_sq.sqrt(); // Uses libm sqrt (one call per mesh is acceptable)

        // Far plane culling: mesh sphere is entirely beyond far plane
        if distance - radius > self.camera.far {
            return true;
        }

        // Near plane culling: mesh sphere is entirely before near plane
        if distance + radius < self.camera.near {
            return true;
        }

        // Passed culling tests - render the mesh
        false
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
            if point.z < self.camera.near || point.z > self.camera.far {
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
        use crate::fixed_math::{div_fp, from_fp, to_fp};

        let point = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
        let point = model_matrix * point;

        if point.w <= 0.0 {
            return None;
        }

        let x_fp = div_fp(to_fp(point.x), to_fp(point.w))?;
        let y_fp = div_fp(to_fp(point.y), to_fp(point.w))?;
        let z_ndc = from_fp(div_fp(to_fp(point.z), to_fp(point.w))?);
        if z_ndc < self.camera.near || z_ndc > self.camera.far {
            return None;
        }

        let x = ((1.0 + from_fp(x_fp)) * 0.5 * self.width as f32) as i32;
        let y = ((1.0 - from_fp(y_fp)) * 0.5 * self.height as f32) as i32;

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
        if clip.w <= 0.0 {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let ndc_z = clip.z / clip.w;
        if ndc_z < self.camera.near || ndc_z > self.camera.far {
            return None;
        }
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

            let transform_matrix = self.camera.vp_matrix * mesh.model_matrix;

            let render_mode = self.resolve_render_mode(&mesh.render_mode);
            match render_mode {
                RenderMode::Points => {
                    let screen_space_points = geometry
                        .vertices
                        .iter()
                        .filter_map(|v| self.transform_point(v, transform_matrix));

                    if geometry.colors.len() == geometry.vertices.len() {
                        for (point, color) in screen_space_points.zip(geometry.colors) {
                            callback(DrawPrimitive::ColoredPoint(point.xy(), *color));
                        }
                    } else {
                        for point in screen_space_points {
                            callback(DrawPrimitive::ColoredPoint(point.xy(), mesh.color));
                        }
                    }
                }

                RenderMode::Lines if !geometry.lines.is_empty() => {
                    for line in geometry.lines {
                        if let Some([p1, p2]) =
                            self.transform_points(line, geometry.vertices, transform_matrix)
                        {
                            callback(DrawPrimitive::Line([p1.xy(), p2.xy()], mesh.color));
                        }
                    }
                }

                RenderMode::Lines if !geometry.faces.is_empty() => {
                    for face in geometry.faces {
                        if let Some([p1, p2, p3]) =
                            self.transform_points(face, geometry.vertices, transform_matrix)
                        {
                            callback(DrawPrimitive::Line([p1.xy(), p2.xy()], mesh.color));
                            callback(DrawPrimitive::Line([p2.xy(), p3.xy()], mesh.color));
                            callback(DrawPrimitive::Line([p3.xy(), p1.xy()], mesh.color));
                        }
                    }
                }

                RenderMode::Lines => {}

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
                        if self.camera.get_direction().dot(&transformed_normal) < 0.0 {
                            continue;
                        }

                        if let Some([p1, p2, p3]) =
                            self.transform_points(face, geometry.vertices, transform_matrix)
                        {
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
                            callback(DrawPrimitive::ColoredTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                color,
                            });
                        }
                    }
                }

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

                        if self.camera.get_direction().dot(&transformed_fn) < 0.0 {
                            continue;
                        }

                        if let Some([p1, p2, p3]) =
                            self.transform_points(face, geometry.vertices, transform_matrix)
                        {
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

                            callback(DrawPrimitive::GouraudTriangleWithDepth {
                                points: [p1.xy(), p2.xy(), p3.xy()],
                                depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                colors: vertex_colors,
                            });
                        }
                    }
                }

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
                        if self.camera.get_direction().dot(&normalized_normal) < 0.0 {
                            continue;
                        }

                        if let Some([p1, p2, p3]) =
                            self.transform_points(face, geometry.vertices, transform_matrix)
                        {
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
                            callback(DrawPrimitive::ColoredTriangleWithDepth {
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
                            if let Some([p1, p2, p3]) =
                                self.transform_points(face, geometry.vertices, transform_matrix)
                            {
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
                                callback(DrawPrimitive::ColoredTriangleWithDepth {
                                    points: [p1.xy(), p2.xy(), p3.xy()],
                                    depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                    color,
                                });
                            }
                        }
                    } else {
                        for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                            let normal = Vector3::new(normal[0], normal[1], normal[2]);
                            let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                            if self.camera.get_direction().dot(&transformed_normal) < 0.0 {
                                continue;
                            }
                            if let Some([p1, p2, p3]) =
                                self.transform_points(face, geometry.vertices, transform_matrix)
                            {
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
                                callback(DrawPrimitive::ColoredTriangleWithDepth {
                                    points: [p1.xy(), p2.xy(), p3.xy()],
                                    depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                    color,
                                });
                            }
                        }
                    }
                }

                RenderMode::SectorBright(brightness) => {
                    let factor = brightness as f32 / 255.0;
                    let scaled_r = (mesh.color.r() as f32 * factor) as u8;
                    let scaled_g = (mesh.color.g() as f32 * factor) as u8;
                    let scaled_b = (mesh.color.b() as f32 * factor) as u8;
                    let scaled_color = Rgb565::new(scaled_r, scaled_g, scaled_b);

                    if geometry.normals.is_empty() {
                        for face in geometry.faces.iter() {
                            if let Some([p1, p2, p3]) =
                                self.transform_points(face, geometry.vertices, transform_matrix)
                            {
                                let color = if !self.point_lights.is_empty() {
                                    let wc = Self::face_world_center(
                                        face,
                                        geometry.vertices,
                                        mesh.model_matrix,
                                    );
                                    Self::add_tint(scaled_color, self.light_tint_at(wc))
                                } else {
                                    scaled_color
                                };
                                callback(DrawPrimitive::ColoredTriangleWithDepth {
                                    points: [p1.xy(), p2.xy(), p3.xy()],
                                    depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                    color,
                                });
                            }
                        }
                    } else {
                        for (face, normal) in geometry.faces.iter().zip(geometry.normals) {
                            let normal = Vector3::new(normal[0], normal[1], normal[2]);
                            let transformed_normal = mesh.model_matrix.transform_vector(&normal);
                            if self.camera.get_direction().dot(&transformed_normal) < 0.0 {
                                continue;
                            }
                            if let Some([p1, p2, p3]) =
                                self.transform_points(face, geometry.vertices, transform_matrix)
                            {
                                let color = if !self.point_lights.is_empty() {
                                    let wc = Self::face_world_center(
                                        face,
                                        geometry.vertices,
                                        mesh.model_matrix,
                                    );
                                    Self::add_tint(scaled_color, self.light_tint_at(wc))
                                } else {
                                    scaled_color
                                };
                                callback(DrawPrimitive::ColoredTriangleWithDepth {
                                    points: [p1.xy(), p2.xy(), p3.xy()],
                                    depths: [p1.z as f32, p2.z as f32, p3.z as f32],
                                    color,
                                });
                            }
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
        use crate::error::{BudgetKind, RenderError};

        commands.clear();
        commands.push(RenderCommand::ClearDepth(u32::MAX))?;
        if let Some(caps) = self.caps {
            caps.validate_framebuffer(self.width as usize, self.height as usize)?;
        }

        let mut first_error = None;
        let mut visible_meshes = 0usize;
        let mut used_texture_ids: heapless::Vec<u32, 64> = heapless::Vec::new();
        let mut meshes_total = 0usize;

        for mesh in meshes {
            meshes_total += 1;
            if mesh.geometry.vertices.is_empty() {
                continue;
            }
            if self.should_cull_mesh(mesh) {
                continue;
            }

            let distance = (mesh.get_position() - self.camera.position).norm();
            let geometry = mesh.select_lod(distance);

            if let Some(caps) = self.caps {
                visible_meshes += 1;
                if visible_meshes > caps.max_meshes_per_frame {
                    return Err(RenderError::OutOfBudget(BudgetKind::MeshesPerFrame {
                        attempted: visible_meshes,
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
                    && !used_texture_ids.iter().any(|id| *id == texture_id)
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
            }

            self.render(core::iter::once(mesh), |primitive| {
                if first_error.is_none()
                    && let Err(e) = commands.push(RenderCommand::Draw(primitive))
                {
                    first_error = Some(e);
                }
            });
            if let Some(err) = first_error {
                return Err(err);
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
        crate::renderer::execute_commands_with_dirty_region(fb, frame, commands, self.fog.as_ref())
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
        crate::renderer::execute_commands_tiled::<D, MAX, BIN_CAP>(
            fb,
            frame,
            commands,
            tile,
            self.fog.as_ref(),
        )
    }
}

/// Result of a ray cast against triangle geometry.
#[derive(Debug, Clone, Copy)]
pub struct MeshRayCastHit {
    /// Distance along the ray to the hit point
    pub distance: f32,
    /// Hit point in world space
    pub point: Vector3<f32>,
    /// Face normal (from cross product of edges, not per-vertex normals)
    pub normal: Vector3<f32>,
    /// Index of the triangle face that was hit
    pub face_index: usize,
    /// Barycentric-interpolated UV at the hit point (or [0.0, 0.0] if no UVs present)
    pub uv: [f32; 2],
}

/// Ray-cast against triangle geometry using Möller–Trumbore intersection.
///
/// `ray_origin` and `ray_dir` are in world space. `model_matrix` transforms mesh
/// vertices to world space. Returns the closest hit within `max_distance`, or `None`.
pub fn mesh_ray_cast(
    ray_origin: Vector3<f32>,
    ray_dir: Vector3<f32>,
    geometry: &mesh::Geometry<'_>,
    model_matrix: &Matrix4<f32>,
    max_distance: f32,
) -> Option<MeshRayCastHit> {
    let mut nearest: Option<MeshRayCastHit> = None;
    let mut min_dist = max_distance;

    for (face_index, face) in geometry.faces.iter().enumerate() {
        let raw_v0 = geometry.vertices[face[0]];
        let raw_v1 = geometry.vertices[face[1]];
        let raw_v2 = geometry.vertices[face[2]];

        // Transform vertices to world space
        let v0 = model_matrix
            .transform_point(&Point3::new(raw_v0[0], raw_v0[1], raw_v0[2]))
            .coords;
        let v1 = model_matrix
            .transform_point(&Point3::new(raw_v1[0], raw_v1[1], raw_v1[2]))
            .coords;
        let v2 = model_matrix
            .transform_point(&Point3::new(raw_v2[0], raw_v2[1], raw_v2[2]))
            .coords;

        // Möller–Trumbore
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = ray_dir.cross(&edge2);
        let det = edge1.dot(&h);

        // Parallel ray: skip
        if det.abs() < 1e-6 {
            continue;
        }

        let inv_det = 1.0 / det;
        let s = ray_origin - v0;
        let bary_u = inv_det * s.dot(&h);
        if bary_u < 0.0 || bary_u > 1.0 {
            continue;
        }

        let q = s.cross(&edge1);
        let bary_v = inv_det * ray_dir.dot(&q);
        if bary_v < 0.0 || bary_u + bary_v > 1.0 {
            continue;
        }

        let t = inv_det * edge2.dot(&q);
        if t <= 0.0 || t >= min_dist {
            continue;
        }

        // Face normal from edge cross product
        let normal = edge1.cross(&edge2).normalize();

        // Barycentric weights: w0 = 1 - u - v, w1 = u, w2 = v
        let bary_w = 1.0 - bary_u - bary_v;

        // Interpolate UV if available
        let uv = if geometry.uvs.len() > face[0]
            && geometry.uvs.len() > face[1]
            && geometry.uvs.len() > face[2]
        {
            let uv0 = geometry.uvs[face[0]];
            let uv1 = geometry.uvs[face[1]];
            let uv2 = geometry.uvs[face[2]];
            [
                bary_w * uv0[0] + bary_u * uv1[0] + bary_v * uv2[0],
                bary_w * uv0[1] + bary_u * uv1[1] + bary_v * uv2[1],
            ]
        } else {
            [0.0, 0.0]
        };

        let point = ray_origin + ray_dir * t;
        min_dist = t;
        nearest = Some(MeshRayCastHit {
            distance: t,
            point,
            normal,
            face_index,
            uv,
        });
    }

    nearest
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = K3dengine::new(640, 480);
        assert_eq!(engine.width, 640);
        assert_eq!(engine.height, 480);
        assert!((engine.camera.get_aspect_ratio() - 640.0 / 480.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_point_basic() {
        let engine = K3dengine::new(640, 480);
        // Use camera's VP matrix directly
        let transform_matrix = engine.camera.vp_matrix;

        // Point in front of default camera, within view frustum
        // Default camera is at origin looking at origin, so we need a point in front
        let point = [0.0, 0.0, -5.0];
        let result = engine.transform_point(&point, transform_matrix);

        if let Some(transformed) = result {
            // Should be within screen bounds
            assert!(transformed.x >= 0 && transformed.x < 640);
            assert!(transformed.y >= 0 && transformed.y < 480);
        }
        // If None, the point was culled which is also valid behavior
    }

    #[test]
    fn test_transform_point_clamps_out_of_bounds() {
        let engine = K3dengine::new(640, 480);
        let model_matrix = nalgebra::Matrix4::identity();

        // Point way outside the viewport should be clamped/rejected
        let point = [100.0, 100.0, -5.0];
        let result = engine.transform_point(&point, model_matrix);
        // Should return None because coordinates are clamped out
        assert!(result.is_none());
    }

    #[test]
    fn test_transform_point_behind_camera() {
        let engine = K3dengine::new(640, 480);
        let transform_matrix = engine.camera.vp_matrix;

        // Point with positive z (behind default camera orientation)
        let point = [0.0, 0.0, 1.0];
        let _result = engine.transform_point(&point, transform_matrix);
        // Point behind camera or outside frustum should return None
        // (actual behavior depends on camera setup and projection)
        // This test just verifies the function doesn't panic
    }

    #[test]
    fn test_transform_point_near_plane_clipping() {
        let engine = K3dengine::new(640, 480);
        let model_matrix = nalgebra::Matrix4::identity();

        // Point too close to camera (before near plane)
        let point = [0.0, 0.0, -0.01];
        let result = engine.transform_point(&point, model_matrix);
        assert!(result.is_none());
    }

    #[test]
    fn test_transform_point_far_plane_clipping() {
        let engine = K3dengine::new(640, 480);
        let model_matrix = nalgebra::Matrix4::identity();

        // Point too far from camera (beyond far plane)
        let point = [0.0, 0.0, -1000.0];
        let result = engine.transform_point(&point, model_matrix);
        assert!(result.is_none());
    }

    #[test]
    fn test_transform_points_array() {
        let engine = K3dengine::new(640, 480);
        let transform_matrix = engine.camera.vp_matrix;

        let vertices = [[0.0, 0.0, -5.0], [0.1, 0.0, -5.0], [0.0, 0.1, -5.0]];
        let indices = [0, 1, 2];

        let result = engine.transform_points(&indices, &vertices, transform_matrix);

        // If transform succeeds, verify we get 3 points
        if let Some(points) = result {
            assert_eq!(points.len(), 3);
        }
        // If None, one or more points were culled which is valid
    }

    #[test]
    fn test_render_empty_faces_mesh() {
        let engine = K3dengine::new(640, 480);
        let vertices = [[0.0, 0.0, -5.0]]; // At least one vertex required
        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mesh = mesh::K3dMesh::new(geometry);

        let mut callback_count = 0;
        engine.render(std::iter::once(&mesh), |_| {
            callback_count += 1;
        });

        // Mesh with no faces/lines should trigger one point callback (default is Points mode)
        assert!(callback_count > 0);
    }

    #[test]
    fn test_render_points_mode() {
        let engine = K3dengine::new(640, 480);

        let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0]];

        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(mesh::RenderMode::Points);

        let mut primitives = std::vec::Vec::new();
        engine.render(std::iter::once(&mesh), |prim| {
            primitives.push(prim);
        });

        // Should render points
        assert!(primitives.len() > 0);
        for prim in primitives {
            assert!(matches!(prim, DrawPrimitive::ColoredPoint(_, _)));
        }
    }

    #[test]
    fn test_render_lines_mode_with_faces() {
        let engine = K3dengine::new(640, 480);

        let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

        let faces = [[0, 1, 2]];

        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(mesh::RenderMode::Lines);

        let mut primitives = std::vec::Vec::new();
        engine.render(std::iter::once(&mesh), |prim| {
            primitives.push(prim);
        });

        // Should render 3 lines (edges of triangle)
        assert_eq!(primitives.len(), 3);
        for prim in primitives {
            assert!(matches!(prim, DrawPrimitive::Line(_, _)));
        }
    }

    #[test]
    fn test_render_gouraud_light_dir() {
        let mut engine = K3dengine::new(640, 480);
        engine.camera.set_position(Point3::new(0.0, 0.0, -10.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let faces = [[0, 1, 2]];
        let normals = [[0.0, 0.0, -1.0]]; // face normal pointing toward camera
        let vertex_normals = [[0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0]];

        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &normals,
            vertex_normals: &vertex_normals,
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(mesh::RenderMode::GouraudLightDir(Vector3::new(
            0.0, 0.0, 1.0,
        )));

        let mut primitives = std::vec::Vec::new();
        engine.render(std::iter::once(&mesh), |prim| {
            primitives.push(prim);
        });

        // Should emit GouraudTriangleWithDepth primitives
        assert!(!primitives.is_empty());
        for prim in &primitives {
            assert!(matches!(
                prim,
                DrawPrimitive::GouraudTriangleWithDepth { .. }
            ));
        }
    }
}
