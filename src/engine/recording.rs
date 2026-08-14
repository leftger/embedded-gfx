use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::OriginDimensions;
use embedded_graphics_core::pixelcolor::Rgb565;
use nalgebra::Point3;

use super::K3dengine;
use super::pipeline::render;
use super::transform::{should_cull_mesh, transform_point_with_w};
use crate::command_buffer::{CommandBuffer, RenderCommand};
use crate::config::{DegradationPolicy, DegradationStep, QualityTier};
use crate::error::{BudgetKind, RecoveryAction, RenderError, RuntimeFaultKind};
use crate::mesh::K3dMesh;
use crate::primitive::DrawPrimitive;

pub(crate) fn record<'a, MS, const MAX: usize>(
    engine: &K3dengine,
    meshes: MS,
    commands: &mut CommandBuffer<MAX>,
    telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
) -> Result<(), RenderError>
where
    MS: IntoIterator<Item = &'a K3dMesh<'a>>,
{
    record_impl(engine, meshes, commands, telemetry)
}

pub(crate) fn record_drop_shadow<const MAX: usize>(
    engine: &K3dengine,
    mesh: &K3dMesh,
    floor_y: f32,
    shadow_radius: f32,
    max_fade_distance: f32,
    shadow_opacity: u8,
    color: Rgb565,
    commands: &mut CommandBuffer<MAX>,
) -> Result<(), RenderError> {
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
    let center_proj = transform_point_with_w(
        &engine.camera,
        engine.width,
        engine.height,
        &center_world,
        engine.camera.vp_matrix,
    );
    let Some((c_pt, _c_w)) = center_proj else {
        return Ok(());
    };

    let mut outer_proj: [Option<(Point3<i32>, f32)>; 8] = [None; 8];
    for i in 0..8 {
        let angle = (i as f32) * (core::f32::consts::PI / 4.0);
        let px = pos.x + radius * micromath::F32Ext::cos(angle);
        let pz = pos.z + radius * micromath::F32Ext::sin(angle);
        outer_proj[i] = transform_point_with_w(
            &engine.camera,
            engine.width,
            engine.height,
            &[px, y_pos, pz],
            engine.camera.vp_matrix,
        );
    }

    for i in 0..8 {
        let next_idx = (i + 1) % 8;
        if let (Some((p1, _w1)), Some((p2, _w2))) = (outer_proj[i], outer_proj[next_idx]) {
            commands.push(RenderCommand::Draw(
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

pub(crate) fn record_impl<'a, MS, const MAX: usize>(
    engine: &K3dengine,
    meshes: MS,
    commands: &mut CommandBuffer<MAX>,
    telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
) -> Result<(), RenderError>
where
    MS: IntoIterator<Item = &'a K3dMesh<'a>>,
{
    commands.clear();
    commands.push(RenderCommand::ClearDepth(crate::Z_MAX_VALUE))?;
    if let Some(caps) = engine.caps {
        caps.validate_framebuffer(engine.width as usize, engine.height as usize)?;
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
            if should_cull_mesh(&engine.camera, mesh) {
                continue;
            }
            let distance = (mesh.get_position() - engine.camera.position).norm();
            let dist_key = (distance * 1000.0) as i32;
            if sorted.push((mesh.priority, dist_key, mesh)).is_err() {
                break;
            }
        }
        sorted.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        for &(_, _, mesh) in sorted.iter() {
            record_one_mesh(
                engine,
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
            if should_cull_mesh(&engine.camera, mesh) {
                continue;
            }
            record_one_mesh(
                engine,
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

pub(crate) fn record_one_mesh<'a, const MAX: usize>(
    engine: &K3dengine,
    mesh: &'a K3dMesh<'a>,
    commands: &mut CommandBuffer<MAX>,
    first_error: &mut Option<RenderError>,
    visible_meshes: &mut usize,
    used_texture_ids: &mut heapless::Vec<u32, 64>,
) -> Result<(), RenderError> {
    let distance = (mesh.get_position() - engine.camera.position).norm();
    let geometry = mesh.select_lod(distance);

    if let Some(caps) = engine.caps {
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
    }

    let mut push_draw = |primitive: DrawPrimitive, err: &mut Option<RenderError>| {
        if err.is_none()
            && let Err(e) = commands.push(RenderCommand::Draw(primitive))
        {
            *err = Some(e);
        }
    };

    #[cfg(feature = "lod-crossfade")]
    {
        match mesh.select_lod_pick(distance) {
            crate::mesh::LodPick::Single(_) => {
                mesh.lod_force.set(None);
                mesh.draw_alpha.set(None);
                render(engine, core::iter::once(mesh), |primitive| {
                    push_draw(primitive, first_error);
                });
            }
            crate::mesh::LodPick::Crossfade { near, far, t } => {
                let near_lvl = mesh.lod_level_of(near);
                let far_lvl = mesh.lod_level_of(far);
                if t < 0.5 {
                    mesh.lod_force.set(Some(near_lvl));
                    mesh.draw_alpha.set(None);
                    render(engine, core::iter::once(mesh), |primitive| {
                        push_draw(primitive, first_error);
                    });
                    let a = (t * 255.0) as u8;
                    if a > 16 {
                        mesh.lod_force.set(Some(far_lvl));
                        mesh.draw_alpha.set(Some(a));
                        render(engine, core::iter::once(mesh), |primitive| {
                            push_draw(primitive, first_error);
                        });
                    }
                } else {
                    mesh.lod_force.set(Some(far_lvl));
                    mesh.draw_alpha.set(None);
                    render(engine, core::iter::once(mesh), |primitive| {
                        push_draw(primitive, first_error);
                    });
                    let a = ((1.0 - t) * 255.0) as u8;
                    if a > 16 {
                        mesh.lod_force.set(Some(near_lvl));
                        mesh.draw_alpha.set(Some(a));
                        render(engine, core::iter::once(mesh), |primitive| {
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
        render(engine, core::iter::once(mesh), |primitive| {
            push_draw(primitive, first_error);
        });
    }
    Ok(())
}

pub(crate) fn record_with_fallback<'a, MS, FS, const MAX: usize>(
    engine: &K3dengine,
    primary: MS,
    fallback: FS,
    commands: &mut CommandBuffer<MAX>,
    telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
) -> Result<super::BudgetFallbackOutcome, RenderError>
where
    MS: IntoIterator<Item = &'a K3dMesh<'a>>,
    FS: IntoIterator<Item = &'a K3dMesh<'a>>,
{
    let mut local_telemetry = crate::telemetry::RecordTelemetry::default();
    match record_impl(engine, primary, commands, Some(&mut local_telemetry)) {
        Ok(()) => {
            if let Some(t) = telemetry {
                *t = local_telemetry;
                t.fallback_used = false;
            }
            Ok(super::BudgetFallbackOutcome {
                used_fallback: false,
                primary_budget_error: None,
            })
        }
        Err(RenderError::OutOfBudget(kind)) => {
            let mut fallback_telemetry = crate::telemetry::RecordTelemetry::default();
            record_impl(engine, fallback, commands, Some(&mut fallback_telemetry))?;
            if let Some(t) = telemetry {
                *t = fallback_telemetry;
                t.fallback_used = true;
            }
            Ok(super::BudgetFallbackOutcome {
                used_fallback: true,
                primary_budget_error: Some(kind),
            })
        }
        Err(e) => Err(e),
    }
}

fn downgraded_quality_tier(tier: QualityTier) -> QualityTier {
    match tier {
        QualityTier::Quality => QualityTier::Balanced,
        QualityTier::Balanced => QualityTier::Fastest,
        QualityTier::Fastest => QualityTier::Fastest,
    }
}

pub(crate) fn record_with_degradation<'a, const MAX: usize>(
    engine: &mut K3dengine,
    meshes: &[&'a K3dMesh<'a>],
    commands: &mut CommandBuffer<MAX>,
    policy: DegradationPolicy<'_>,
    telemetry: Option<&mut crate::telemetry::RecordTelemetry>,
) -> Result<super::DegradationOutcome, RenderError> {
    let original_quality = engine.quality_tier;
    let mut active_quality = engine.quality_tier;

    let mut outcome = super::DegradationOutcome {
        used_degradation: false,
        steps_applied: 0,
        dropped_meshes: 0,
        final_quality_tier: active_quality,
        primary_budget_error: None,
    };

    let mut local_telemetry = crate::telemetry::RecordTelemetry::default();
    match record_impl(
        engine,
        meshes.iter().copied(),
        commands,
        Some(&mut local_telemetry),
    ) {
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
                    engine.quality_tier = original_quality;
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
                active_quality = downgraded_quality_tier(active_quality);
                engine.quality_tier = active_quality;
                for mesh in meshes {
                    let _ = selected.push(*mesh);
                }
            }
        }

        if selected.is_empty() {
            continue;
        }

        let mut step_telemetry = crate::telemetry::RecordTelemetry::default();
        let attempt = record_impl(
            engine,
            selected.iter().copied(),
            commands,
            Some(&mut step_telemetry),
        );

        if let Ok(()) = attempt {
            outcome.final_quality_tier = engine.quality_tier;
            if let Some(t) = telemetry {
                *t = step_telemetry;
                t.fallback_used = true;
                t.degradation_steps_applied = outcome.steps_applied;
                t.dropped_meshes = outcome.dropped_meshes;
            }
            engine.quality_tier = original_quality;
            return Ok(outcome);
        }
    }

    engine.quality_tier = original_quality;
    Err(RenderError::Recoverable {
        fault: RuntimeFaultKind::Budget(outcome.primary_budget_error.unwrap_or(
            BudgetKind::DrawPrimitives {
                attempted: commands.len(),
                max: MAX,
            },
        )),
        action: RecoveryAction::SkipFrame,
    })
}

pub(crate) fn execute<D, const MAX: usize>(
    engine: &K3dengine,
    fb: &mut D,
    frame: &mut crate::renderer::FrameCtx<'_>,
    commands: &CommandBuffer<MAX>,
    telemetry: Option<&mut crate::telemetry::ExecuteTelemetry>,
) -> Result<Option<crate::renderer::DirtyRegion>, RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    if let Some(t) = telemetry {
        t.commands_total = commands.len();
        t.draw_commands = commands
            .iter()
            .filter(|cmd| matches!(cmd, RenderCommand::Draw(_)))
            .count();
        t.clear_color_commands = commands
            .iter()
            .filter(|cmd| matches!(cmd, RenderCommand::ClearColor(_)))
            .count();
        t.clear_depth_commands = commands
            .iter()
            .filter(|cmd| matches!(cmd, RenderCommand::ClearDepth(_)))
            .count();
    }
    let camera_dir = engine.camera.get_direction();
    crate::renderer::execute_commands_with_dirty_region_effects(
        fb,
        frame,
        commands,
        engine.fog.as_ref(),
        engine.dither.as_ref(),
        engine.screen_tint,
        engine.stipple_mode,
        engine.palette_mode,
        engine.sky,
        [camera_dir.x, camera_dir.y, camera_dir.z],
    )
}

#[cfg(feature = "textured")]
pub(crate) fn execute_with_textures<D, const MAX: usize, const N: usize>(
    engine: &K3dengine,
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
    if let Some(t) = telemetry {
        t.commands_total = commands.len();
        t.draw_commands = commands
            .iter()
            .filter(|cmd| matches!(cmd, RenderCommand::Draw(_)))
            .count();
        t.clear_color_commands = commands
            .iter()
            .filter(|cmd| matches!(cmd, RenderCommand::ClearColor(_)))
            .count();
        t.clear_depth_commands = commands
            .iter()
            .filter(|cmd| matches!(cmd, RenderCommand::ClearDepth(_)))
            .count();
    }
    let camera_dir = engine.camera.get_direction();
    crate::renderer::execute_commands_with_dirty_region_effects_textured(
        fb,
        frame,
        commands,
        texture_manager,
        engine.fog.as_ref(),
        engine.dither.as_ref(),
        engine.screen_tint,
        engine.stipple_mode,
        engine.palette_mode,
        engine.sky,
        [camera_dir.x, camera_dir.y, camera_dir.z],
    )
}

pub(crate) fn execute_tiled<D, const MAX: usize, const BIN_CAP: usize>(
    engine: &K3dengine,
    fb: &mut D,
    frame: &mut crate::renderer::FrameCtx<'_>,
    commands: &CommandBuffer<MAX>,
    tile: crate::tilebin::TileConfig,
) -> Result<crate::tilebin::TileBinStats, RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    let camera_dir = engine.camera.get_direction();
    crate::renderer::execute_commands_tiled_effects::<D, MAX, BIN_CAP>(
        fb,
        frame,
        commands,
        tile,
        engine.fog.as_ref(),
        engine.dither.as_ref(),
        engine.screen_tint,
        engine.stipple_mode,
        engine.palette_mode,
        engine.sky,
        [camera_dir.x, camera_dir.y, camera_dir.z],
    )
}

#[cfg(feature = "gizmos")]
pub(crate) fn record_aabb_gizmo<const MAX: usize>(
    engine: &K3dengine,
    aabb: &crate::bounds::Aabb,
    model_matrix: &nalgebra::Matrix4<f32>,
    color: Rgb565,
    commands: &mut CommandBuffer<MAX>,
) -> Result<(), RenderError> {
    let mut err = None;
    crate::gizmos::emit_aabb_wireframe_projected(
        aabb,
        model_matrix,
        |p| {
            crate::engine::transform::transform_point(
                &engine.camera,
                engine.width,
                engine.height,
                &p,
                engine.camera.vp_matrix,
            )
        },
        color,
        |prim| {
            if err.is_none()
                && let Err(e) = commands.push(RenderCommand::Draw(prim))
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

#[cfg(feature = "gizmos")]
pub(crate) fn record_frustum_gizmo<const MAX: usize>(
    engine: &K3dengine,
    color: Rgb565,
    commands: &mut CommandBuffer<MAX>,
) -> Result<(), RenderError> {
    let mut err = None;
    crate::gizmos::emit_frustum_wireframe(
        &engine.camera,
        |p| {
            crate::engine::transform::transform_point(
                &engine.camera,
                engine.width,
                engine.height,
                &p,
                engine.camera.vp_matrix,
            )
        },
        color,
        |prim| {
            if err.is_none()
                && let Err(e) = commands.push(RenderCommand::Draw(prim))
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
