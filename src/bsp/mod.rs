//! BSP world rendering — `K3dengine::record_bsp` and `render_bsp_direct`.
//!
//! # API overview
//!
//! ```text
//! // Record/execute split (fits the existing pipeline):
//! engine.record_bsp(&world, &mut scratch, &mut commands, Some(&mut tel))?;
//! engine.execute_bsp_textured(fb, &mut frame, &commands, &tex_mgr, None)?;
//!
//! // One-shot direct render (skips command buffer, slightly lower overhead):
//! engine.render_bsp_direct(&world, &mut scratch, &tex_mgr, fb, &mut frame, Some(&mut tel))?;
//!
//! // M5: no z-buffer (requires front-to-back guarantee from BSP):
//! engine.render_bsp_coverage(&world, &mut scratch, &tex_mgr, fb, &mut cov, Some(&mut tel))?;
//! ```

#[cfg(feature = "std")]
pub mod builder;
pub mod coverage;
pub mod data;
pub mod pvs;
pub mod scratch;
pub mod traverse;

use core::fmt::Debug;

use embedded_graphics_core::{
    draw_target::DrawTarget, pixelcolor::Rgb565, prelude::OriginDimensions,
};
use heapless::Vec as HVec;
use nalgebra::{Point2, Vector4};

use crate::{
    DrawPrimitive, K3dengine,
    command_buffer::{CommandBuffer, RenderCommand},
    error::RenderError,
    lights::PointLight,
    renderer::FrameCtx,
    sector_lights::{SectorLight, light_level_u8_at},
    texture::TextureManager,
};

use coverage::CoverageBuffer;
use data::{BspWorld, Face};
use scratch::BspScratch;
use traverse::{ClipVert, frustum_from_vp, project_to_screen, walk_front_to_back};

// ---------------------------------------------------------------------------
// Telemetry
// ---------------------------------------------------------------------------

/// Per-frame BSP rendering statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BspTelemetry {
    /// Number of BSP leaves visited during the walk.
    pub leaves_visited: usize,
    /// Leaves rejected by PVS.
    pub leaves_culled_pvs: usize,
    /// Leaves rejected by frustum culling.
    pub leaves_culled_frustum: usize,
    /// Total faces in the visible set after de-duplication.
    pub faces_visible: usize,
    /// Faces actually emitted to the command buffer (excludes degenerate/clipped-away).
    pub faces_emitted: usize,
    /// Triangles emitted after near-plane clipping.
    pub triangles_emitted: usize,
}

// ---------------------------------------------------------------------------
// Triangle fan → clip → project → emit
// ---------------------------------------------------------------------------

/// Internal result of projecting one triangle from a BSP face fan.
struct ProjTri {
    points: [Point2<i32>; 3],
    depths: [f32; 3],
    ws: [f32; 3],
    uvs: [[f32; 2]; 3],
    lm_uvs: [[f32; 2]; 3],
}

#[inline]
fn lerp_clip_vert(a: ClipVert, b: ClipVert, t: f32) -> ClipVert {
    ClipVert {
        clip: a.clip + (b.clip - a.clip) * t,
        uv: [
            a.uv[0] + (b.uv[0] - a.uv[0]) * t,
            a.uv[1] + (b.uv[1] - a.uv[1]) * t,
        ],
        lm_uv: [
            a.lm_uv[0] + (b.lm_uv[0] - a.lm_uv[0]) * t,
            a.lm_uv[1] + (b.lm_uv[1] - a.lm_uv[1]) * t,
        ],
    }
}

fn clip_polygon_plane(
    input: &HVec<ClipVert, 10>,
    output: &mut HVec<ClipVert, 10>,
    dist: impl Fn(ClipVert) -> f32,
) {
    output.clear();
    let n = input.len();
    if n == 0 {
        return;
    }
    for i in 0..n {
        let prev = input[(n + i - 1) % n];
        let curr = input[i];
        let d_prev = dist(prev);
        let d_curr = dist(curr);
        if d_curr >= 0.0 {
            if d_prev < 0.0 {
                let denom = d_prev - d_curr;
                if denom.abs() > 1e-6 {
                    let t = d_prev / denom;
                    let _ = output.push(lerp_clip_vert(prev, curr, t));
                }
            }
            let _ = output.push(curr);
        } else if d_prev >= 0.0 {
            let denom = d_prev - d_curr;
            if denom.abs() > 1e-6 {
                let t = d_prev / denom;
                let _ = output.push(lerp_clip_vert(prev, curr, t));
            }
        }
    }
}

/// Clip and project a triangle into 0–8 screen-space triangles.
///
/// Vertices are given in clip space with associated UV and lightmap UV
/// coordinates. This performs full clip-space frustum clipping (near/far and
/// side planes), then fan-triangulates the clipped polygon.
fn clip_and_project(
    v: [ClipVert; 3],
    width: u16,
    height: u16,
    near: f32,
    far: f32,
) -> HVec<ProjTri, 8> {
    let mut result: HVec<ProjTri, 8> = HVec::new();

    // Build a 3-vertex polygon and clip against the canonical clip frustum.
    let mut a: HVec<ClipVert, 10> = HVec::new();
    let mut b: HVec<ClipVert, 10> = HVec::new();
    for vert in &v {
        let _ = a.push(*vert);
    }

    // w >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.w);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // near: z >= -w  => z + w >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.z + p.clip.w);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // far: z <= w  => w - z >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.w - p.clip.z);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // left: x >= -w  => x + w >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.x + p.clip.w);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // right: x <= w  => w - x >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.w - p.clip.x);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // bottom: y >= -w  => y + w >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.y + p.clip.w);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // top: y <= w  => w - y >= 0
    clip_polygon_plane(&a, &mut b, |p| p.clip.w - p.clip.y);
    if b.len() < 3 {
        return result;
    }
    core::mem::swap(&mut a, &mut b);

    // Fan-triangulate projected clipped polygon.
    let n = a.len();
    for i in 1..(n - 1) {
        let pts = [&a[0], &a[i], &a[i + 1]];

        let mut screen = [Point2::new(0i32, 0i32); 3];
        let mut depths = [0.0f32; 3];
        let mut ws = [0.0f32; 3];
        let mut uvs = [[0.0f32; 2]; 3];
        let mut lm_uvs = [[0.0f32; 2]; 3];
        let mut ok = true;

        for (k, &pt) in pts.iter().enumerate() {
            if let Some((sp, depth, w)) = project_to_screen(pt.clip, width, height, near, far) {
                screen[k] = sp;
                depths[k] = depth;
                ws[k] = w;
                uvs[k] = pt.uv;
                lm_uvs[k] = pt.lm_uv;
            } else {
                ok = false;
                break;
            }
        }

        if ok {
            let _ = result.push(ProjTri {
                points: screen,
                depths,
                ws,
                uvs,
                lm_uvs,
            });
        }
    }

    result
}

/// Build the 3 clip-space vertices for triangle `k` of a fan starting at
/// `face.first_vert` in `world`, applying `vp` matrix.
///
/// Returns `None` if vertex indices are out of bounds.
fn face_fan_triangle(
    world: &BspWorld<'_>,
    face: &Face,
    k: usize,
    vp: &nalgebra::Matrix4<f32>,
) -> Option<[ClipVert; 3]> {
    let base = face.first_vert as usize;
    let i0 = base;
    let i1 = base + k + 1;
    let i2 = base + k + 2;

    if i2 >= world.vertices.len() {
        return None;
    }

    let to_clip = |idx: usize| -> Vector4<f32> {
        let v = &world.vertices[idx];
        vp * Vector4::new(v[0], v[1], v[2], 1.0)
    };

    let uv = |idx: usize| -> [f32; 2] { world.uvs.get(idx).copied().unwrap_or([0.0, 0.0]) };

    let lm_uv = |idx: usize| -> [f32; 2] { world.lm_uvs.get(idx).copied().unwrap_or([0.0, 0.0]) };

    Some([
        ClipVert {
            clip: to_clip(i0),
            uv: uv(i0),
            lm_uv: lm_uv(i0),
        },
        ClipVert {
            clip: to_clip(i1),
            uv: uv(i1),
            lm_uv: lm_uv(i1),
        },
        ClipVert {
            clip: to_clip(i2),
            uv: uv(i2),
            lm_uv: lm_uv(i2),
        },
    ])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the world-space centroid of a BSP face and accumulate the additive
/// RGB565 tint from a slice of point lights.
fn face_dynamic_tint(world: &BspWorld<'_>, face: &Face, lights: &[PointLight]) -> Rgb565 {
    use embedded_graphics_core::pixelcolor::RgbColor;
    if lights.is_empty() {
        return Rgb565::new(0, 0, 0);
    }
    let base = face.first_vert as usize;
    let n = face.num_verts as usize;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut cz = 0.0f32;
    for i in 0..n {
        if let Some(v) = world.vertices.get(base + i) {
            cx += v[0];
            cy += v[1];
            cz += v[2];
        }
    }
    if n == 0 {
        return Rgb565::new(0, 0, 0);
    }
    let inv = 1.0 / n as f32;
    let world_pos = nalgebra::Point3::new(cx * inv, cy * inv, cz * inv);
    let mut r = 0u32;
    let mut g = 0u32;
    let mut b = 0u32;
    for light in lights {
        let c = light.contribution_at(world_pos);
        r += c.r() as u32;
        g += c.g() as u32;
        b += c.b() as u32;
    }
    Rgb565::new(r.min(31) as u8, g.min(63) as u8, b.min(31) as u8)
}

// ---------------------------------------------------------------------------
// K3dengine BSP methods
// ---------------------------------------------------------------------------

impl K3dengine {
    /// Record BSP world geometry into a command buffer for later execution.
    ///
    /// Produces [`DrawPrimitive::TexturedTriangleWithDepth`] commands for faces
    /// without lightmaps and [`DrawPrimitive::LightmappedTriangle`] for faces
    /// with `face.lightmap_id != 0xFFFF`.
    ///
    /// Execute the resulting buffer with [`K3dengine::execute_bsp_textured`].
    pub fn record_bsp<const MAX: usize>(
        &self,
        world: &BspWorld<'_>,
        scratch: &mut BspScratch<'_>,
        commands: &mut CommandBuffer<MAX>,
        telemetry: Option<&mut BspTelemetry>,
    ) -> Result<(), RenderError> {
        self.record_bsp_with_sector_lights(world, scratch, commands, None, 0.0, telemetry)
    }

    /// Record BSP geometry while applying optional animated sector lights.
    pub fn record_bsp_with_sector_lights<const MAX: usize>(
        &self,
        world: &BspWorld<'_>,
        scratch: &mut BspScratch<'_>,
        commands: &mut CommandBuffer<MAX>,
        sector_lights: Option<&[SectorLight]>,
        time_seconds: f32,
        telemetry: Option<&mut BspTelemetry>,
    ) -> Result<(), RenderError> {
        commands.clear();
        commands.push(RenderCommand::ClearDepth(crate::Z_MAX_VALUE))?;
        scratch.mark_new_frame();

        let cam = [
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
        ];
        let cam_leaf = world.leaf_for_point(cam);
        let cam_cluster = world.leaves.get(cam_leaf).map(|l| l.cluster).unwrap_or(-1);

        let frustum = frustum_from_vp(&self.camera.vp_matrix);
        let vp = &self.camera.vp_matrix;

        let mut tel = BspTelemetry::default();
        let mut first_err: Option<RenderError> = None;

        walk_front_to_back(world, scratch, cam, cam_cluster, &frustum, |_fi, face| {
            if first_err.is_some() {
                return;
            }
            tel.faces_visible += 1;

            let dynamic_tint = face_dynamic_tint(world, face, &self.point_lights);

            let num_tris = face.num_verts.saturating_sub(2) as usize;
            let mut emitted_any = false;

            for k in 0..num_tris {
                let tri = match face_fan_triangle(world, face, k, vp) {
                    Some(t) => t,
                    None => continue,
                };
                let projected = clip_and_project(
                    tri,
                    self.width,
                    self.height,
                    self.camera.near,
                    self.camera.far,
                );

                for pt in projected.iter() {
                    let brightness = if let Some(lights) = sector_lights {
                        if face.sector_light_id == u16::MAX {
                            255
                        } else {
                            lights
                                .get(face.sector_light_id as usize)
                                .map_or(255, |light| light_level_u8_at(light, time_seconds))
                        }
                    } else {
                        255
                    }
                    // Keep a small floor to avoid complete black-outs on low-light sectors.
                    .max(32);
                    let prim = DrawPrimitive::LightmappedTriangle {
                        points: [pt.points[0], pt.points[1], pt.points[2]],
                        depths: pt.depths,
                        ws: pt.ws,
                        surface_uvs: pt.uvs,
                        lm_uvs: pt.lm_uvs,
                        texture_id: face.texture_id,
                        lightmap_id: if face.lightmap_id == 0xFFFF {
                            u32::MAX
                        } else {
                            face.lightmap_id as u32
                        },
                        brightness,
                        dynamic_tint,
                    };

                    if let Err(e) = commands.push(RenderCommand::Draw(prim)) {
                        first_err = Some(e);
                        return;
                    }
                    tel.triangles_emitted += 1;
                    emitted_any = true;
                }
            }

            if emitted_any {
                tel.faces_emitted += 1;
            }
        });

        if let Some(err) = first_err {
            return Err(err);
        }

        if let Some(t) = telemetry {
            *t = tel;
        }
        Ok(())
    }

    /// Execute a BSP command buffer with texture support.
    ///
    /// Handles both [`DrawPrimitive::TexturedTriangleWithDepth`] and
    /// [`DrawPrimitive::LightmappedTriangle`] using `texture_manager`.
    pub fn execute_bsp_textured<D, const MAX: usize, const N: usize>(
        &self,
        fb: &mut D,
        frame: &mut FrameCtx<'_>,
        commands: &CommandBuffer<MAX>,
        texture_manager: &TextureManager<N>,
        telemetry: Option<&mut crate::telemetry::ExecuteTelemetry>,
    ) -> Result<Option<crate::renderer::DirtyRegion>, RenderError>
    where
        D: DrawTarget<Color = Rgb565> + OriginDimensions,
        D::Error: Debug,
    {
        use crate::draw::{draw_zbuffered_lightmapped_mapped, draw_zbuffered_with_textures_mapped};
        use crate::renderer::DirtyRegion;

        frame.validate()?;
        let mut dirty: Option<(i32, i32, i32, i32)> = None;

        if let Some(t) = telemetry {
            t.commands_total = commands.len();
            t.draw_commands = commands
                .iter()
                .filter(|c| matches!(c, RenderCommand::Draw(_)))
                .count();
        }

        for cmd in commands.iter() {
            match cmd {
                RenderCommand::ClearDepth(v) => crate::clear_zbuffer(frame.zbuffer, *v),
                RenderCommand::ClearColor(color) => {
                    let w = frame.width as i32;
                    let h = frame.height as i32;
                    for y in 0..h {
                        for x in 0..w {
                            fb.draw_iter([embedded_graphics_core::Pixel(
                                embedded_graphics_core::prelude::Point::new(x, y),
                                *color,
                            )])
                            .map_err(|_| RenderError::InvalidInput("clear write failed"))?;
                        }
                    }
                }
                RenderCommand::Draw(prim) => {
                    let (x0, y0, x1, y1) = prim_bounds(prim);
                    match prim.clone() {
                        DrawPrimitive::LightmappedTriangle {
                            points,
                            depths,
                            ws,
                            surface_uvs,
                            lm_uvs,
                            texture_id,
                            lightmap_id,
                            brightness,
                            dynamic_tint,
                        } => {
                            draw_zbuffered_lightmapped_mapped(
                                points,
                                depths,
                                ws,
                                surface_uvs,
                                lm_uvs,
                                texture_id,
                                lightmap_id,
                                brightness,
                                dynamic_tint,
                                self.fog.as_ref(),
                                texture_manager,
                                fb,
                                frame.zbuffer,
                                frame.width,
                                self.texture_mapping,
                                self.stipple_mode,
                                self.screen_tint,
                                self.palette_mode,
                            );
                        }
                        other => {
                            draw_zbuffered_with_textures_mapped(
                                other,
                                fb,
                                frame.zbuffer,
                                frame.width,
                                texture_manager,
                                self.fog.as_ref(),
                                self.dither.as_ref(),
                                self.texture_mapping,
                                self.stipple_mode,
                                self.screen_tint,
                                self.palette_mode,
                            );
                        }
                    }
                    if let Some((cx0, cy0, cx1, cy1)) = dirty {
                        dirty = Some((cx0.min(x0), cy0.min(y0), cx1.max(x1), cy1.max(y1)));
                    } else {
                        dirty = Some((x0, y0, x1, y1));
                    }
                }
            }
        }

        let region = dirty.and_then(|(x0, y0, x1, y1)| {
            if x1 < x0 || y1 < y0 {
                return None;
            }
            Some(DirtyRegion {
                x: x0 as usize,
                y: y0 as usize,
                width: (x1 - x0 + 1) as usize,
                height: (y1 - y0 + 1) as usize,
            })
        });
        Ok(region)
    }

    /// One-shot BSP render: traverse + rasterize without an intermediate command buffer.
    ///
    /// Lower overhead than record+execute for world geometry that must be
    /// re-traversed every frame (camera always moves).
    pub fn render_bsp_direct<D, const N: usize>(
        &self,
        world: &BspWorld<'_>,
        scratch: &mut BspScratch<'_>,
        texture_manager: &TextureManager<N>,
        fb: &mut D,
        frame: &mut FrameCtx<'_>,
        telemetry: Option<&mut BspTelemetry>,
    ) -> Result<(), RenderError>
    where
        D: DrawTarget<Color = Rgb565> + OriginDimensions,
        D::Error: Debug,
    {
        use crate::draw::draw_zbuffered_lightmapped_mapped;

        frame.validate()?;
        crate::clear_zbuffer(frame.zbuffer, crate::Z_MAX_VALUE);
        scratch.mark_new_frame();

        let cam = [
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
        ];
        let cam_leaf = world.leaf_for_point(cam);
        let cam_cluster = world.leaves.get(cam_leaf).map(|l| l.cluster).unwrap_or(-1);
        let frustum = frustum_from_vp(&self.camera.vp_matrix);
        let vp = &self.camera.vp_matrix;

        let mut tel = BspTelemetry::default();

        walk_front_to_back(world, scratch, cam, cam_cluster, &frustum, |_fi, face| {
            tel.faces_visible += 1;

            let dynamic_tint = face_dynamic_tint(world, face, &self.point_lights);

            let num_tris = face.num_verts.saturating_sub(2) as usize;
            let mut emitted_any = false;

            for k in 0..num_tris {
                let tri = match face_fan_triangle(world, face, k, vp) {
                    Some(t) => t,
                    None => continue,
                };
                let projected = clip_and_project(
                    tri,
                    self.width,
                    self.height,
                    self.camera.near,
                    self.camera.far,
                );

                for pt in projected.iter() {
                    draw_zbuffered_lightmapped_mapped(
                        [pt.points[0], pt.points[1], pt.points[2]],
                        pt.depths,
                        pt.ws,
                        pt.uvs,
                        pt.lm_uvs,
                        face.texture_id,
                        if face.lightmap_id == 0xFFFF {
                            u32::MAX
                        } else {
                            face.lightmap_id as u32
                        },
                        255,
                        dynamic_tint,
                        self.fog.as_ref(),
                        texture_manager,
                        fb,
                        frame.zbuffer,
                        frame.width,
                        self.texture_mapping,
                        self.stipple_mode,
                        self.screen_tint,
                        self.palette_mode,
                    );
                    tel.triangles_emitted += 1;
                    emitted_any = true;
                }
            }

            if emitted_any {
                tel.faces_emitted += 1;
            }
        });

        if let Some(t) = telemetry {
            *t = tel;
        }
        Ok(())
    }

    /// M5: One-shot BSP render using a coverage bitmap instead of a z-buffer.
    ///
    /// Requires `frame` was created with a valid zbuffer (the zbuffer is *not*
    /// used for depth testing — the coverage buffer takes its role).  The
    /// zbuffer slice is only needed to satisfy the `FrameCtx` API; pass a
    /// zero-length slice and skip `frame.validate()` if RAM is tight, or
    /// supply a 1-element dummy.
    ///
    /// Front-to-back traversal guarantees correctness without depth testing.
    pub fn render_bsp_coverage<D, const N: usize>(
        &self,
        world: &BspWorld<'_>,
        scratch: &mut BspScratch<'_>,
        texture_manager: &TextureManager<N>,
        fb: &mut D,
        coverage: &mut CoverageBuffer<'_>,
        telemetry: Option<&mut BspTelemetry>,
    ) -> Result<(), RenderError>
    where
        D: DrawTarget<Color = Rgb565> + OriginDimensions,
        D::Error: Debug,
    {
        use crate::draw::draw_bsp_coverage;

        coverage.clear();
        scratch.mark_new_frame();

        let cam = [
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
        ];
        let cam_leaf = world.leaf_for_point(cam);
        let cam_cluster = world.leaves.get(cam_leaf).map(|l| l.cluster).unwrap_or(-1);
        let frustum = frustum_from_vp(&self.camera.vp_matrix);
        let vp = &self.camera.vp_matrix;

        let mut tel = BspTelemetry::default();

        walk_front_to_back(world, scratch, cam, cam_cluster, &frustum, |_fi, face| {
            // Early-out when every pixel is already covered
            if coverage.is_full() {
                return;
            }

            tel.faces_visible += 1;
            let num_tris = face.num_verts.saturating_sub(2) as usize;
            let mut emitted_any = false;

            for k in 0..num_tris {
                let tri = match face_fan_triangle(world, face, k, vp) {
                    Some(t) => t,
                    None => continue,
                };
                let projected = clip_and_project(
                    tri,
                    self.width,
                    self.height,
                    self.camera.near,
                    self.camera.far,
                );

                for pt in projected.iter() {
                    draw_bsp_coverage(
                        [pt.points[0], pt.points[1], pt.points[2]],
                        pt.ws,
                        pt.uvs,
                        face.texture_id,
                        texture_manager,
                        fb,
                        coverage,
                        self.texture_mapping,
                        self.stipple_mode,
                        self.screen_tint,
                        self.palette_mode,
                    );
                    tel.triangles_emitted += 1;
                    emitted_any = true;
                }
            }

            if emitted_any {
                tel.faces_emitted += 1;
            }
        });

        if let Some(t) = telemetry {
            *t = tel;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Primitive bounding box helper (mirrors renderer.rs::primitive_bounds)
// ---------------------------------------------------------------------------

fn prim_bounds(p: &DrawPrimitive) -> (i32, i32, i32, i32) {
    match p {
        DrawPrimitive::ColoredPoint(pt, _) => (pt.x, pt.y, pt.x, pt.y),
        DrawPrimitive::Line([a, b], _) => (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)),
        DrawPrimitive::ColoredTriangle(pts, _)
        | DrawPrimitive::ColoredTriangleWithDepth { points: pts, .. }
        | DrawPrimitive::TranslucentTriangleWithDepth { points: pts, .. }
        | DrawPrimitive::GouraudTriangle { points: pts, .. }
        | DrawPrimitive::GouraudTriangleWithDepth { points: pts, .. }
        | DrawPrimitive::TexturedTriangle { points: pts, .. }
        | DrawPrimitive::TexturedTriangleWithDepth { points: pts, .. }
        | DrawPrimitive::LightmappedTriangle { points: pts, .. } => {
            let min_x = pts.iter().map(|p| p.x).min().unwrap_or(0);
            let min_y = pts.iter().map(|p| p.y).min().unwrap_or(0);
            let max_x = pts.iter().map(|p| p.x).max().unwrap_or(0);
            let max_y = pts.iter().map(|p| p.y).max().unwrap_or(0);
            (min_x, min_y, max_x, max_y)
        }
    }
}

// ---------------------------------------------------------------------------
// Test-only two-room level fixture
// ---------------------------------------------------------------------------

/// A minimal hand-built level for use in tests and integration examples.
///
/// Layout:
/// ```text
///     Room A              Room B
///  (-5,-2,-3)           (0,-2,-3)
///  ┌──────────┐   X=0   ┌──────────┐
///  │          │         │          │
///  │ cluster 0│         │ cluster 1│
///  └──────────┘         └──────────┘
///  (0, 2, 3)            (5, 2, 3)
/// ```
/// Both rooms are fully mutually visible (no zero-run PVS entries).
#[allow(dead_code)]
pub mod test_level {
    use super::data::*;

    pub static PLANES: [Plane; 1] = [Plane {
        normal: [1.0, 0.0, 0.0],
        dist: 0.0,
    }];

    // Node: X=0 split. children[0]=front(d>=0)=leaf1, children[1]=back(d<0)=leaf0
    pub static NODES: [Node; 1] = [Node {
        plane: 0,
        children: [!1i32, !0i32],
        mins: [-5, -2, -3],
        maxs: [5, 2, 3],
        first_face: 0,
        num_faces: 0,
    }];

    pub static LEAVES: [Leaf; 2] = [
        Leaf {
            cluster: 0,
            mins: [-5, -2, -3],
            maxs: [0, 2, 3],
            first_marksurface: 0,
            num_marksurfaces: 2,
        },
        Leaf {
            cluster: 1,
            mins: [0, -2, -3],
            maxs: [5, 2, 3],
            first_marksurface: 2,
            num_marksurfaces: 2,
        },
    ];

    // Room A floor (fan = 2 triangles) + Room B floor (fan = 2 triangles)
    pub static FACES: [Face; 4] = [
        // Room A floor
        Face {
            first_vert: 0,
            num_verts: 4,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
            sector_light_id: u16::MAX,
        },
        // Room A ceiling
        Face {
            first_vert: 4,
            num_verts: 4,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
            sector_light_id: u16::MAX,
        },
        // Room B floor
        Face {
            first_vert: 8,
            num_verts: 4,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
            sector_light_id: u16::MAX,
        },
        // Room B ceiling
        Face {
            first_vert: 12,
            num_verts: 4,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
            sector_light_id: u16::MAX,
        },
    ];

    pub static MARKSURFACES: [u16; 4] = [0, 1, 2, 3];

    // Room A floor quad (y=-2 plane, fan: v0,v1,v2 + v0,v2,v3)
    pub static VERTICES: [[f32; 3]; 16] = [
        // Room A floor (y=-2)
        [-5.0, -2.0, -3.0],
        [0.0, -2.0, -3.0],
        [0.0, -2.0, 3.0],
        [-5.0, -2.0, 3.0],
        // Room A ceiling (y=2)
        [-5.0, 2.0, -3.0],
        [0.0, 2.0, -3.0],
        [0.0, 2.0, 3.0],
        [-5.0, 2.0, 3.0],
        // Room B floor (y=-2)
        [0.0, -2.0, -3.0],
        [5.0, -2.0, -3.0],
        [5.0, -2.0, 3.0],
        [0.0, -2.0, 3.0],
        // Room B ceiling (y=2)
        [0.0, 2.0, -3.0],
        [5.0, 2.0, -3.0],
        [5.0, 2.0, 3.0],
        [0.0, 2.0, 3.0],
    ];

    pub static UVS: [[f32; 2]; 16] = [
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];

    // Full-vis PVS: 2 clusters, each byte = 0b00000011 = both visible
    pub static VIS: [u8; 2] = [0b00000011, 0b00000011];
    pub static VIS_OFFSETS: [u32; 2] = [0, 1];

    pub fn world() -> BspWorld<'static> {
        BspWorld::new(
            &PLANES,
            &NODES,
            &LEAVES,
            &FACES,
            &MARKSURFACES,
            &VERTICES,
            &UVS,
            &[], // no lightmaps in test level
            &VIS,
            &VIS_OFFSETS,
            2,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::bsp::scratch::BspScratch;
    use crate::bsp::test_level;
    use crate::sector_lights::{LightEffectKind, SectorLight};
    use nalgebra::Point3;

    #[test]
    fn test_level_leaf_location() {
        let world = test_level::world();
        let leaf_a = world.leaf_for_point([-2.0, 0.0, 0.0]);
        let leaf_b = world.leaf_for_point([2.0, 0.0, 0.0]);
        assert_eq!(leaf_a, 0);
        assert_eq!(leaf_b, 1);
    }

    #[test]
    fn test_level_cluster_visibility() {
        let world = test_level::world();
        assert!(world.cluster_visible(0, 0));
        assert!(world.cluster_visible(0, 1));
        assert!(world.cluster_visible(1, 0));
        assert!(world.cluster_visible(1, 1));
    }

    #[test]
    fn record_bsp_emits_draw_commands() {
        let world = test_level::world();
        let mut visframe = [0u32; 4];
        let mut scratch = BspScratch::new(&mut visframe);

        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(-2.0, 0.0, 0.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let mut commands: CommandBuffer<512> = CommandBuffer::new();
        let mut tel = BspTelemetry::default();

        engine
            .record_bsp(&world, &mut scratch, &mut commands, Some(&mut tel))
            .unwrap();

        // Should have at least one draw command (floor/ceiling quads)
        let draw_count = commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::Draw(_)))
            .count();
        assert!(draw_count > 0, "expected draw commands, got 0");
        assert!(tel.faces_visible > 0);
        assert!(tel.triangles_emitted > 0);
    }

    #[test]
    fn record_bsp_deduplicates_faces() {
        let world = test_level::world();
        let mut vf1 = [0u32; 4];
        let mut vf2 = [0u32; 4];
        let mut s1 = BspScratch::new(&mut vf1);
        let mut s2 = BspScratch::new(&mut vf2);

        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(-2.0, 0.0, 0.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let mut c1: CommandBuffer<512> = CommandBuffer::new();
        engine.record_bsp(&world, &mut s1, &mut c1, None).unwrap();
        let count1 = c1
            .iter()
            .filter(|c| matches!(c, RenderCommand::Draw(_)))
            .count();

        // Second record (same frame config) should produce identical count
        let mut c2: CommandBuffer<512> = CommandBuffer::new();
        engine.record_bsp(&world, &mut s2, &mut c2, None).unwrap();
        let count2 = c2
            .iter()
            .filter(|c| matches!(c, RenderCommand::Draw(_)))
            .count();

        assert_eq!(count1, count2);
    }

    #[test]
    fn record_bsp_with_sector_lights_keeps_output_when_faces_unmapped() {
        let world = test_level::world();
        let mut vf_a = [0u32; 4];
        let mut vf_b = [0u32; 4];
        let mut scratch_a = BspScratch::new(&mut vf_a);
        let mut scratch_b = BspScratch::new(&mut vf_b);

        let mut engine = K3dengine::new(320, 240);
        engine.camera.set_position(Point3::new(-2.0, 0.0, 0.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let mut commands_base: CommandBuffer<512> = CommandBuffer::new();
        engine
            .record_bsp(&world, &mut scratch_a, &mut commands_base, None)
            .unwrap();
        let base_draw_count = commands_base
            .iter()
            .filter(|c| matches!(c, RenderCommand::Draw(_)))
            .count();

        let lights = [SectorLight {
            base: 255,
            alt: 48,
            speed: 0.5,
            duration: 0.0,
            sync: 0.0,
            effect: Some(LightEffectKind::Glow),
        }];

        let mut commands_with_lights: CommandBuffer<512> = CommandBuffer::new();
        engine
            .record_bsp_with_sector_lights(
                &world,
                &mut scratch_b,
                &mut commands_with_lights,
                Some(&lights),
                1.0,
                None,
            )
            .unwrap();
        let lit_draw_count = commands_with_lights
            .iter()
            .filter(|c| matches!(c, RenderCommand::Draw(_)))
            .count();

        // test_level faces use u16::MAX sector_light_id, so command emission should match.
        assert_eq!(base_draw_count, lit_draw_count);
    }

    #[test]
    fn coverage_buffer_basic() {
        let mut data = [0u8; CoverageBuffer::bytes_for(32, 32)];
        let mut cov = CoverageBuffer::new(&mut data, 32, 32).unwrap();
        cov.clear();
        assert!(!cov.is_full());
        assert!(!cov.is_covered(5, 5));
        cov.mark_covered(5, 5);
        assert!(cov.is_covered(5, 5));
    }

    #[test]
    fn clip_and_project_triangle_inside_frustum_emits_one() {
        let tri = [
            ClipVert {
                clip: nalgebra::Vector4::new(-0.5, -0.5, 0.0, 1.0),
                uv: [0.0, 0.0],
                lm_uv: [0.0, 0.0],
            },
            ClipVert {
                clip: nalgebra::Vector4::new(0.5, -0.5, 0.0, 1.0),
                uv: [1.0, 0.0],
                lm_uv: [1.0, 0.0],
            },
            ClipVert {
                clip: nalgebra::Vector4::new(0.0, 0.5, 0.0, 1.0),
                uv: [0.5, 1.0],
                lm_uv: [0.5, 1.0],
            },
        ];
        let out = clip_and_project(tri, 320, 240, 0.1, 40.0);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn clip_and_project_triangle_crossing_side_plane_is_clipped() {
        let tri = [
            ClipVert {
                clip: nalgebra::Vector4::new(0.0, -0.2, 0.0, 1.0),
                uv: [0.0, 0.0],
                lm_uv: [0.0, 0.0],
            },
            ClipVert {
                clip: nalgebra::Vector4::new(1.6, 0.0, 0.0, 1.0), // outside right plane
                uv: [1.0, 0.5],
                lm_uv: [1.0, 0.5],
            },
            ClipVert {
                clip: nalgebra::Vector4::new(0.0, 0.4, 0.0, 1.0),
                uv: [0.0, 1.0],
                lm_uv: [0.0, 1.0],
            },
        ];
        let out = clip_and_project(tri, 320, 240, 0.1, 40.0);
        assert!(!out.is_empty(), "expected clipped output triangles");
        for tri in out.iter() {
            for p in tri.points {
                assert!(p.x >= 0 && p.x <= 320);
                assert!(p.y >= 0 && p.y <= 240);
            }
        }
    }

    #[test]
    fn clip_and_project_triangle_behind_near_plane_is_discarded() {
        let tri = [
            ClipVert {
                clip: nalgebra::Vector4::new(-0.2, -0.2, -2.0, 1.0),
                uv: [0.0, 0.0],
                lm_uv: [0.0, 0.0],
            },
            ClipVert {
                clip: nalgebra::Vector4::new(0.2, -0.2, -2.2, 1.0),
                uv: [1.0, 0.0],
                lm_uv: [1.0, 0.0],
            },
            ClipVert {
                clip: nalgebra::Vector4::new(0.0, 0.2, -2.1, 1.0),
                uv: [0.5, 1.0],
                lm_uv: [0.5, 1.0],
            },
        ];
        let out = clip_and_project(tri, 320, 240, 0.1, 40.0);
        assert_eq!(out.len(), 0);
    }
}
