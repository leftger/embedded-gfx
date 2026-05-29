// Row width configuration - features are prioritized if multiple are enabled
#[cfg(feature = "row_width_320")]
const MAX_ROW_WIDTH: usize = 320;
#[cfg(all(feature = "row_width_240", not(feature = "row_width_320")))]
const MAX_ROW_WIDTH: usize = 240;
#[cfg(all(
    feature = "row_width_160",
    not(feature = "row_width_240"),
    not(feature = "row_width_320"),
    not(feature = "row_width_96")
))]
const MAX_ROW_WIDTH: usize = 160;
#[cfg(all(
    feature = "row_width_96",
    not(feature = "row_width_160"),
    not(feature = "row_width_240"),
    not(feature = "row_width_320")
))]
const MAX_ROW_WIDTH: usize = 96;
#[cfg(not(any(
    feature = "row_width_320",
    feature = "row_width_240",
    feature = "row_width_160",
    feature = "row_width_96"
)))]
const MAX_ROW_WIDTH: usize = 100;

use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
#[cfg(feature = "aa")]
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::RgbColor;
use embedded_graphics_core::prelude::Point;
use heapless::Vec;

use crate::DrawPrimitive;

/// Framebuffer that supports reading back pixel values.
///
/// Required by the analytical-AA rasterizers (`draw_zbuffered_aa`,
/// `draw_line_aa`) which blend partial-coverage triangle edges and
/// anti-aliased line endpoints with the existing framebuffer contents.
/// Implementers should return the most recently written color at `point`,
/// or `Rgb565::BLACK` for out-of-bounds reads.
#[cfg(feature = "aa")]
pub trait ReadPixel {
    fn read_pixel(&self, point: Point) -> Rgb565;
}

/// Component-wise blend in 8-bit fixed-point coverage.
/// `coverage_q8` ∈ [0, 256]; 256 = full triangle color, 0 = full background.
#[cfg(feature = "aa")]
#[inline(always)]
fn blend_q8(bg: Rgb565, fg: Rgb565, coverage_q8: u32) -> Rgb565 {
    let inv = 256 - coverage_q8;
    let r = (bg.r() as u32 * inv + fg.r() as u32 * coverage_q8) >> 8;
    let g = (bg.g() as u32 * inv + fg.g() as u32 * coverage_q8) >> 8;
    let b = (bg.b() as u32 * inv + fg.b() as u32 * coverage_q8) >> 8;
    Rgb565::new(r as u8, g as u8, b as u8)
}

/// Z-test + coverage blend + write a single AA pixel.
///
/// Coverage handling has three cases:
/// - Full coverage (`coverage_q8 >= 256`): fast path, write color directly.
/// - Partial coverage on a virgin pixel (z-buffer at `u32::MAX`): true
///   silhouette against the background — blend `bg * (1-c) + color * c`.
/// - Partial coverage on a pixel another triangle has already painted:
///   treat as a shared interior edge and write full color. This avoids the
///   classic double-blend seam artifact at shared edges in closed meshes.
///   Tradeoff: thin protrusions whose silhouette overlaps another triangle
///   lose AA on that overlap. Acceptable for typical closed geometry.
#[cfg(feature = "aa-heuristic")]
#[inline(always)]
fn aa_pixel<D>(
    fb: &mut D,
    x: i32,
    y: i32,
    color: Rgb565,
    z: u32,
    zbuffer: &mut [u32],
    width: usize,
    coverage_q8: u32,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    if x < 0 || y < 0 || coverage_q8 == 0 {
        return;
    }
    let idx = y as usize * width + x as usize;
    if idx >= zbuffer.len() {
        return;
    }
    if z >= zbuffer[idx].saturating_add(DEPTH_EPSILON) {
        return;
    }

    let pixel_was_virgin = zbuffer[idx] == u32::MAX;
    let final_color = if coverage_q8 >= 256 || !pixel_was_virgin {
        color
    } else {
        let bg = fb.read_pixel(Point::new(x, y));
        blend_q8(bg, color, coverage_q8)
    };
    zbuffer[idx] = z;
    fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
        .unwrap();
}

/// Depth epsilon for Z-buffer comparison to prevent Z-fighting
///
/// When triangles are nearly coplanar or edge-on to the camera, floating-point
/// precision errors can cause depth values to be extremely close, leading to
/// flickering (Z-fighting). This epsilon provides a small bias that helps
/// new pixels pass the depth test when they are very close to existing ones.
///
/// Value: 128 in 16.16 fixed-point format = 0.00195 in floating-point
/// Tuned for typical embedded graphics scenarios. Increase if Z-fighting persists,
/// decrease if you notice incorrect depth ordering on distant objects.
///
/// **Tuning guide:**
/// - More Z-fighting? Increase this value (256, 512, etc.)
/// - Incorrect depth ordering? Decrease this value (64, 32, etc.)
/// - Adjust camera near/far planes for better depth precision distribution
const DEPTH_EPSILON: u32 = 128;

/// Configuration for depth-based fog effect
#[derive(Debug, Clone, Copy)]
pub struct FogConfig {
    /// Fog color to blend towards
    pub color: embedded_graphics_core::pixelcolor::Rgb565,
    /// Near plane distance (fixed-point 16.16 format)
    pub near: u32,
    /// Far plane distance (fixed-point 16.16 format)
    pub far: u32,
}

impl FogConfig {
    /// Create a new fog configuration
    ///
    /// # Arguments
    /// * `color` - The fog color
    /// * `near` - Near distance (depth values closer than this have no fog)
    /// * `far` - Far distance (depth values farther than this are fully fogged)
    pub fn new(color: embedded_graphics_core::pixelcolor::Rgb565, near: f32, far: f32) -> Self {
        Self {
            color,
            near: (near * 65536.0) as u32,
            far: (far * 65536.0) as u32,
        }
    }

    /// Apply fog effect to a color based on depth
    #[inline]
    pub fn apply(
        &self,
        base_color: embedded_graphics_core::pixelcolor::Rgb565,
        depth: u32,
    ) -> embedded_graphics_core::pixelcolor::Rgb565 {
        // Calculate fog factor: 0.0 at near plane, 1.0 at far plane
        let fog_factor = if depth <= self.near {
            0u32
        } else if depth >= self.far {
            65536u32 // 1.0 in fixed-point
        } else {
            // Linear interpolation: (depth - near) / (far - near)
            let numerator = (depth - self.near) as u64;
            let denominator = (self.far - self.near) as u64;
            ((numerator * 65536) / denominator) as u32
        };

        // Blend base color with fog color
        // final_color = base_color * (1 - fog_factor) + fog_color * fog_factor
        let base_r = base_color.r() as u32;
        let base_g = base_color.g() as u32;
        let base_b = base_color.b() as u32;

        let fog_r = self.color.r() as u32;
        let fog_g = self.color.g() as u32;
        let fog_b = self.color.b() as u32;

        // fog_factor is in 16.16 fixed-point format
        let r = ((base_r * (65536 - fog_factor) + fog_r * fog_factor) / 65536) as u8;
        let g = ((base_g * (65536 - fog_factor) + fog_g * fog_factor) / 65536) as u8;
        let b = ((base_b * (65536 - fog_factor) + fog_b * fog_factor) / 65536) as u8;

        embedded_graphics_core::pixelcolor::Rgb565::new(r, g, b)
    }
}

/// Configuration for ordered dithering effect
#[derive(Debug, Clone, Copy)]
pub struct DitherConfig {
    /// Dithering intensity (0-255, where 0 is no dithering)
    pub intensity: u8,
}

impl DitherConfig {
    /// 4x4 Bayer matrix for ordered dithering
    /// Values are in range [0, 15] and will be scaled by intensity
    const BAYER_MATRIX: [[u8; 4]; 4] =
        [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

    /// Create a new dither configuration
    pub fn new(intensity: u8) -> Self {
        Self { intensity }
    }

    /// Apply dithering effect to a color based on screen position
    #[inline]
    pub fn apply(
        &self,
        color: embedded_graphics_core::pixelcolor::Rgb565,
        x: i32,
        y: i32,
    ) -> embedded_graphics_core::pixelcolor::Rgb565 {
        if self.intensity == 0 {
            return color;
        }

        // Get threshold from Bayer matrix (tiles every 4x4 pixels)
        let matrix_x = (x & 3) as usize;
        let matrix_y = (y & 3) as usize;
        let threshold = Self::BAYER_MATRIX[matrix_y][matrix_x];

        // Scale threshold by intensity
        // threshold is 0-15, intensity is 0-255
        // Combined threshold is in range 0-255
        let scaled_threshold = ((threshold as u16 * self.intensity as u16) / 15) as u8;

        // Apply threshold to each color channel
        let r = color.r();
        let g = color.g();
        let b = color.b();

        // Add dithering noise (can increase or decrease based on threshold)
        let r = if r > scaled_threshold {
            r.saturating_sub(scaled_threshold / 2)
        } else {
            r.saturating_add(scaled_threshold / 2)
        };

        let g = if g > scaled_threshold {
            g.saturating_sub(scaled_threshold / 2)
        } else {
            g.saturating_add(scaled_threshold / 2)
        };

        let b = if b > scaled_threshold {
            b.saturating_sub(scaled_threshold / 2)
        } else {
            b.saturating_add(scaled_threshold / 2)
        };

        embedded_graphics_core::pixelcolor::Rgb565::new(r, g, b)
    }
}

struct Interpolator {
    x: i32,
    dx: i32,
    dy: i32,
    error: i32,
}

impl Interpolator {
    fn new(p_start: Point, p_end: Point) -> Self {
        Self {
            x: p_start.x,
            dx: p_end.x - p_start.x,
            dy: p_end.y - p_start.y,
            error: 0,
        }
    }

    fn next(&mut self) -> i32 {
        self.x += self.dx / self.dy;
        self.error += self.dx % self.dy;
        if self.error >= self.dy {
            self.x += 1;
            self.error -= self.dy;
        }
        self.x
    }
}

// Fixed-point 16.16 edge stepper — integer-only replacement for f32 invslope.
const FP_SHIFT: i64 = 16;

#[inline(always)]
fn fixed_to_i32(value: i64) -> i32 {
    if value >= 0 {
        (value >> FP_SHIFT) as i32
    } else {
        -((-value) >> FP_SHIFT) as i32
    }
}

struct EdgeStepper {
    x: i64,
    step: i64,
}

impl EdgeStepper {
    fn new(start: Point, end: Point, y: i32) -> Self {
        let dy = (end.y - start.y) as i64;
        let (step, x) = if dy != 0 {
            let s = (((end.x - start.x) as i64) << FP_SHIFT) / dy;
            let x = ((start.x as i64) << FP_SHIFT) + s * (y - start.y) as i64;
            (s, x)
        } else {
            (0, (start.x as i64) << FP_SHIFT)
        };
        Self { x, step }
    }

    #[inline(always)]
    fn current_x(&self) -> i32 {
        fixed_to_i32(self.x)
    }

    #[inline(always)]
    fn advance(&mut self) {
        self.x += self.step;
    }
}

#[inline(always)]
fn fill_triangle<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let area = (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x);
    if area == 0 {
        // Degenerate triangle (all points collinear)
        return;
    }

    let bounds = fb.bounding_box();
    let min_x = bounds.top_left.x;
    let max_x = bounds.bottom_right().unwrap().x;

    let mut pixel_row: [embedded_graphics_core::Pixel<embedded_graphics_core::pixelcolor::Rgb565>;
        MAX_ROW_WIDTH] = [embedded_graphics_core::Pixel(
        Point::new(0, 0),
        embedded_graphics_core::pixelcolor::RgbColor::BLACK,
    ); MAX_ROW_WIDTH];

    // Top part (p1 to p2)
    if p2.y - p1.y > 0 {
        let mut a = Interpolator::new(p1, p2);
        let mut b = Interpolator::new(p1, p3);

        for y in p1.y..p2.y {
            let ax = a.next();
            let bx = b.next();
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let start_x = start_x.clamp(min_x, max_x);
            let end_x = end_x.clamp(min_x, max_x);

            let mut i = 0;
            for x in start_x..=end_x {
                pixel_row[i] = embedded_graphics_core::Pixel(Point::new(x, y), color);
                i += 1;
            }

            fb.draw_iter(pixel_row[..(end_x - start_x + 1) as usize].iter().copied())
                .unwrap();
        }
    }

    // Bottom part (p2 to p3)
    if p3.y - p2.y > 0 {
        let mut a = Interpolator::new(p2, p3);
        let mut b = Interpolator::new(p1, p3);

        for y in p2.y..=p3.y {
            let ax = a.next();
            let bx = b.next();
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let start_x = start_x.clamp(min_x, max_x);
            let end_x = end_x.clamp(min_x, max_x);

            let mut i = 0;
            for x in start_x..=end_x {
                pixel_row[i] = embedded_graphics_core::Pixel(Point::new(x, y), color);
                i += 1;
            }

            fb.draw_iter(pixel_row[..(end_x - start_x + 1) as usize].iter().copied())
                .unwrap();
        }
    }
}

fn fill_bottom_flat_triangle<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let mut edge1 = EdgeStepper::new(p1, p2, p1.y);
    let mut edge2 = EdgeStepper::new(p1, p3, p1.y);

    for scanline_y in p1.y..=p2.y {
        draw_horizontal_line(
            Point::new(edge1.current_x(), scanline_y),
            Point::new(edge2.current_x(), scanline_y),
            color,
            fb,
        );
        edge1.advance();
        edge2.advance();
    }
}

fn fill_top_flat_triangle<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    // p1.y == p2.y (top flat), p3 is the bottom vertex; iterate top-down.
    let mut edge1 = EdgeStepper::new(p1, p3, p1.y);
    let mut edge2 = EdgeStepper::new(p2, p3, p1.y);

    for scanline_y in p1.y..=p3.y {
        draw_horizontal_line(
            Point::new(edge1.current_x(), scanline_y),
            Point::new(edge2.current_x(), scanline_y),
            color,
            fb,
        );
        edge1.advance();
        edge2.advance();
    }
}

fn draw_horizontal_line<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let start = p1.x.min(p2.x);
    let end = p1.x.max(p2.x);

    for x in start..=end {
        fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, p1.y), color)])
            .unwrap();
    }
}

#[inline]
pub fn draw<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    primitive: DrawPrimitive,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::Line([p1, p2], color) => {
            fb.draw_iter(
                line_drawing::Bresenham::new((p1.x, p1.y), (p2.x, p2.y))
                    .map(|(x, y)| embedded_graphics_core::Pixel(Point::new(x, y), color)),
            )
            .unwrap();
        }
        DrawPrimitive::ColoredPoint(p, c) => {
            let p = embedded_graphics_core::geometry::Point::new(p.x, p.y);

            fb.draw_iter([embedded_graphics_core::Pixel(p, c)]).unwrap();
        }
        DrawPrimitive::ColoredTriangle(mut vertices, color) => {
            // sort vertices by y using sort_unstable_by
            vertices
                .as_mut_slice()
                .sort_unstable_by(|a, b| a.y.cmp(&b.y));

            let [p1, p2, p3] = [
                Point::new(vertices[0].x, vertices[0].y),
                Point::new(vertices[1].x, vertices[1].y),
                Point::new(vertices[2].x, vertices[2].y),
            ];

            // Off-screen culling: skip if all vertices are beyond the same edge.
            let bounds = fb.bounding_box();
            let scr_w = bounds.size.width as i32;
            let scr_h = bounds.size.height as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            fill_triangle(p1, p2, p3, color, fb);
        }
        DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths: _,
            color,
        } => {
            // This variant should use draw_zbuffered() instead
            // For compatibility, render without Z-buffering (ignoring depths)
            let mut vertices = points;
            if vertices[0].y > vertices[1].y {
                vertices.swap(0, 1);
            }
            if vertices[0].y > vertices[2].y {
                vertices.swap(0, 2);
            }
            if vertices[1].y > vertices[2].y {
                vertices.swap(1, 2);
            }

            let mut buf: Vec<_, 3> = Vec::new();
            for p in vertices.iter() {
                buf.push(embedded_graphics_core::geometry::Point::new(p.x, p.y))
                    .unwrap();
            }
            let [p1, p2, p3] = buf.into_array().unwrap();

            // Off-screen culling.
            let bounds = fb.bounding_box();
            let scr_w = bounds.size.width as i32;
            let scr_h = bounds.size.height as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            if p2.y == p3.y {
                fill_bottom_flat_triangle(p1, p2, p3, color, fb);
            } else if p1.y == p2.y {
                fill_top_flat_triangle(p1, p2, p3, color, fb);
            } else {
                let p4 = Point::new(
                    (p1.x as f32
                        + ((p2.y - p1.y) as f32 / (p3.y - p1.y) as f32) * (p3.x - p1.x) as f32)
                        as i32,
                    p2.y,
                );

                fill_bottom_flat_triangle(p1, p2, p4, color, fb);
                fill_top_flat_triangle(p2, p4, p3, color, fb);
            }
        }
        DrawPrimitive::GouraudTriangle {
            mut points,
            mut colors,
        } => {
            // Sort vertices by y coordinate (and corresponding colors)
            if points[0].y > points[1].y {
                points.swap(0, 1);
                colors.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                colors.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                colors.swap(1, 2);
            }

            let mut buf: Vec<_, 3> = Vec::new();
            for p in points.iter() {
                buf.push(embedded_graphics_core::geometry::Point::new(p.x, p.y))
                    .unwrap();
            }
            let [p1, p2, p3] = buf.into_array().unwrap();
            let [c1, c2, c3] = colors;

            // Off-screen culling.
            let bounds = fb.bounding_box();
            let scr_w = bounds.size.width as i32;
            let scr_h = bounds.size.height as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            if p2.y == p3.y {
                fill_bottom_flat_gouraud(p1, p2, p3, c1, c2, c3, fb);
            } else if p1.y == p2.y {
                fill_top_flat_gouraud(p1, p2, p3, c1, c2, c3, fb);
            } else {
                // Split triangle into two flat triangles
                let t = (p2.y - p1.y) as f32 / (p3.y - p1.y) as f32;
                let p4 = Point::new((p1.x as f32 + t * (p3.x - p1.x) as f32) as i32, p2.y);
                // Interpolate color at split point
                let c4 = interpolate_color(c1, c3, t);

                fill_bottom_flat_gouraud(p1, p2, p4, c1, c2, c4, fb);
                fill_top_flat_gouraud(p2, p4, p3, c2, c4, c3, fb);
            }
        }
        DrawPrimitive::GouraudTriangleWithDepth {
            points,
            depths: _,
            colors,
        } => {
            // This variant should use draw_zbuffered() instead
            // For compatibility, render without Z-buffering (ignoring depths)
            let prim = DrawPrimitive::GouraudTriangle { points, colors };
            draw(prim, fb);
        }
        DrawPrimitive::TexturedTriangle { .. }
        | DrawPrimitive::TexturedTriangleWithDepth { .. } => {
            // Textured triangles require a texture manager
            // Use draw_zbuffered_with_textures() instead
            // Cannot render without texture manager, so this is a no-op
        }
    }
}

// Interpolate between two colors
#[inline]
fn interpolate_color(
    c1: embedded_graphics_core::pixelcolor::Rgb565,
    c2: embedded_graphics_core::pixelcolor::Rgb565,
    t: f32,
) -> embedded_graphics_core::pixelcolor::Rgb565 {
    let r1 = c1.r() as f32;
    let g1 = c1.g() as f32;
    let b1 = c1.b() as f32;

    let r2 = c2.r() as f32;
    let g2 = c2.g() as f32;
    let b2 = c2.b() as f32;

    let r = (r1 + t * (r2 - r1)) as u8;
    let g = (g1 + t * (g2 - g1)) as u8;
    let b = (b1 + t * (b2 - b1)) as u8;

    embedded_graphics_core::pixelcolor::Rgb565::new(r, g, b)
}

// Gouraud shading - bottom flat triangle with color interpolation
fn fill_bottom_flat_gouraud<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    c1: embedded_graphics_core::pixelcolor::Rgb565,
    c2: embedded_graphics_core::pixelcolor::Rgb565,
    c3: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = (p2.y - p1.y) as f32;
    if height == 0.0 {
        return;
    }

    let mut edge1 = EdgeStepper::new(p1, p2, p1.y);
    let mut edge2 = EdgeStepper::new(p1, p3, p1.y);

    for scanline_y in p1.y..=p2.y {
        let t = (scanline_y - p1.y) as f32 / height;
        let color_left = interpolate_color(c1, c2, t);
        let color_right = interpolate_color(c1, c3, t);

        draw_horizontal_line_gouraud(
            Point::new(edge1.current_x(), scanline_y),
            Point::new(edge2.current_x(), scanline_y),
            color_left,
            color_right,
            fb,
        );

        edge1.advance();
        edge2.advance();
    }
}

// Gouraud shading - top flat triangle with color interpolation
fn fill_top_flat_gouraud<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    c1: embedded_graphics_core::pixelcolor::Rgb565,
    c2: embedded_graphics_core::pixelcolor::Rgb565,
    c3: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    // p1.y == p2.y (top flat), p3 is the bottom vertex; iterate top-down.
    let height = (p3.y - p1.y) as f32;
    if height == 0.0 {
        return;
    }

    let mut edge1 = EdgeStepper::new(p1, p3, p1.y);
    let mut edge2 = EdgeStepper::new(p2, p3, p1.y);

    for scanline_y in p1.y..=p3.y {
        let t = (scanline_y - p1.y) as f32 / height;
        let color_left = interpolate_color(c1, c3, t);
        let color_right = interpolate_color(c2, c3, t);

        draw_horizontal_line_gouraud(
            Point::new(edge1.current_x(), scanline_y),
            Point::new(edge2.current_x(), scanline_y),
            color_left,
            color_right,
            fb,
        );

        edge1.advance();
        edge2.advance();
    }
}

// Draw a horizontal line with color interpolation (Gouraud)
fn draw_horizontal_line_gouraud<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    color1: embedded_graphics_core::pixelcolor::Rgb565,
    color2: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let start = p1.x.min(p2.x);
    let end = p1.x.max(p2.x);
    let width = (end - start) as f32;

    if width == 0.0 {
        fb.draw_iter([embedded_graphics_core::Pixel(
            Point::new(start, p1.y),
            color1,
        )])
        .unwrap();
        return;
    }

    for x in start..=end {
        let t = (x - start) as f32 / width;
        let color = interpolate_color(color1, color2, t);
        fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, p1.y), color)])
            .unwrap();
    }
}

// Z-buffered drawing function
// Using u32 for Z-buffer is much faster on embedded systems without FPU
#[inline]
pub fn draw_zbuffered<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    // Call with no effects for backward compatibility
    draw_zbuffered_with_effects(primitive, fb, zbuffer, width, None, None);
}

/// Z-buffered drawing with analytical edge anti-aliasing.
///
/// Renders triangles with sub-pixel-accurate left/right edge coverage by
/// blending the boundary pixels with the existing framebuffer contents.
/// Inner pixels of each scanline use the same fast path as `draw_zbuffered`.
/// Lines use Wu's algorithm.
///
/// Requires `ReadPixel` on the framebuffer for the boundary blends.
#[cfg(feature = "aa-heuristic")]
#[inline]
pub fn draw_zbuffered_aa<D>(primitive: DrawPrimitive, fb: &mut D, zbuffer: &mut [u32], width: usize)
where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::ColoredTriangleWithDepth {
            mut points,
            mut depths,
            color,
        } => {
            if points[0].y > points[1].y {
                points.swap(0, 1);
                depths.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                depths.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                depths.swap(1, 2);
            }
            let [p1, p2, p3] = points;
            let [z1, z2, z3] = depths;

            // Off-screen culling.
            let scr_w = width as i32;
            let scr_h = (zbuffer.len() / width) as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            fill_triangle_zbuffered_aa(p1, p2, p3, z1, z2, z3, color, fb, zbuffer, width);
        }
        DrawPrimitive::Line([p1, p2], color) => {
            draw_line_aa(p1.x, p1.y, p2.x, p2.y, color, fb);
        }
        // Anything else (Gouraud, textured, points) — fall back to the
        // non-AA path. AA variants for those can be added as needed.
        other => draw_zbuffered(other, fb, zbuffer, width),
    }
}

#[cfg(feature = "aa-heuristic")]
#[inline(always)]
fn fill_triangle_zbuffered_aa<D>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_aa(
            p1_eg, p2_eg, p3_eg, z1_int, z2_int, z3_int, color, fb, zbuffer, width,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_aa(
            p1_eg, p2_eg, p3_eg, z1_int, z2_int, z3_int, color, fb, zbuffer, width,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        fill_bottom_flat_aa(
            p1_eg, p2_eg, p4, z1_int, z2_int, z4_int, color, fb, zbuffer, width,
        );
        fill_top_flat_aa(
            p2_eg, p4, p3_eg, z2_int, z4_int, z3_int, color, fb, zbuffer, width,
        );
    }
}

#[cfg(feature = "aa-heuristic")]
#[inline(always)]
fn fill_bottom_flat_aa<D>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let z_left = (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;
        let z_right = (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;

        aa_scanline(
            curx1, curx2, scanline_y, z_left, z_right, color, fb, zbuffer, width,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[cfg(feature = "aa-heuristic")]
#[inline(always)]
fn fill_top_flat_aa<D>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let z_left = (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;
        let z_right = (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32;

        aa_scanline(
            curx1, curx2, scanline_y, z_left, z_right, color, fb, zbuffer, width,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

/// Render one scanline with analytical left/right edge coverage.
///
/// `cx1` / `cx2` are 16.16 fixed-point edge positions. The fractional parts
/// give us per-edge sub-pixel coverage; the integer span between them is
/// rendered with the existing fully-opaque fast path.
#[cfg(feature = "aa-heuristic")]
#[inline(always)]
fn aa_scanline<D>(
    cx1: i32,
    cx2: i32,
    y: i32,
    z_left: u32,
    z_right: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    // Normalize: left should be the smaller fixed-point x.
    let (left_fx, right_fx, z_l, z_r) = if cx1 <= cx2 {
        (cx1, cx2, z_left, z_right)
    } else {
        (cx2, cx1, z_right, z_left)
    };

    let l_int = left_fx >> 16;
    let r_int = right_fx >> 16;
    let l_frac_q16 = (left_fx & 0xFFFF) as u32;
    let r_frac_q16 = (right_fx & 0xFFFF) as u32;

    // Linear z interpolation across the inner span.
    let span = r_int - l_int;

    if l_int == r_int {
        // Sub-pixel span: triangle width < 1px on this row. Coverage equals
        // the float-difference of the edge positions.
        let cov_q16 = r_frac_q16.saturating_sub(l_frac_q16);
        aa_pixel(fb, l_int, y, color, z_l, zbuffer, width, cov_q16 >> 8);
        return;
    }

    // Left boundary: covered fraction is (1 - l_frac).
    let left_cov_q8 = 256 - (l_frac_q16 >> 8);
    aa_pixel(fb, l_int, y, color, z_l, zbuffer, width, left_cov_q8);

    // Inner pixels: full coverage. Reuse the existing scanline fast path
    // semantics inline (z-test + write, no read-blend).
    if span > 1 {
        for x in (l_int + 1)..r_int {
            if x < 0 {
                continue;
            }
            let idx = y as usize * width + x as usize;
            if idx >= zbuffer.len() {
                continue;
            }
            // Linear interp z across the inner pixels
            let t_num = (x - l_int) as i64;
            let t_den = span as i64;
            let z = (z_l as i64 + ((z_r as i64 - z_l as i64) * t_num / t_den)) as u32;
            if z < zbuffer[idx].saturating_add(DEPTH_EPSILON) {
                zbuffer[idx] = z;
                fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), color)])
                    .unwrap();
            }
        }
    }

    // Right boundary: covered fraction is r_frac itself.
    if r_frac_q16 > 0 {
        let right_cov_q8 = r_frac_q16 >> 8;
        aa_pixel(fb, r_int, y, color, z_r, zbuffer, width, right_cov_q8);
    }
}

/// Z-buffered drawing with coverage-tracked analytical edge AA.
///
/// Differs from `draw_zbuffered_aa` by maintaining a per-pixel coverage
/// buffer (`u8`, 0..255) so that multiple triangles meeting at a shared
/// edge composite additively rather than overwriting each other. This
/// eliminates the residual color-step seam at coplanar shared edges that
/// the simpler `draw_zbuffered_aa` heuristic leaves behind.
///
/// **Caller protocol per frame:**
/// 1. Clear `zbuffer` to `u32::MAX` and `coverage` to `0`.
/// 2. Render all primitives via this function.
/// 3. Call [`composite_aa_background`] with the desired background color.
///    This blends `bg` into pixels that aren't fully covered.
///
/// **Cost:** an additional `width * height` byte buffer (~6 KiB at 96×64).
/// Inner-pixel rasterization is the same fast path as `draw_zbuffered`.
#[cfg(feature = "aa-coverage")]
#[inline]
pub fn draw_zbuffered_aa_coverage<D>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [u32],
    coverage: &mut [u8],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::ColoredTriangleWithDepth {
            mut points,
            mut depths,
            color,
        } => {
            if points[0].y > points[1].y {
                points.swap(0, 1);
                depths.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                depths.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                depths.swap(1, 2);
            }
            let [p1, p2, p3] = points;
            let [z1, z2, z3] = depths;
            fill_triangle_zbuffered_aa_cov(
                p1, p2, p3, z1, z2, z3, color, fb, zbuffer, coverage, width,
            );
        }
        DrawPrimitive::Line([p1, p2], color) => {
            // Use the coverage-aware Wu's variant so the bg composite at
            // end-of-frame doesn't overwrite line pixels.
            draw_line_aa_coverage(p1.x, p1.y, p2.x, p2.y, color, fb, coverage, width);
        }
        other => draw_zbuffered(other, fb, zbuffer, width),
    }
}

/// Wu's anti-aliased line that updates a coverage buffer alongside the
/// framebuffer write. Use with `draw_zbuffered_aa_coverage` so the bg
/// composite at end-of-frame respects line pixels.
#[cfg(feature = "aa-coverage")]
pub fn draw_line_aa_coverage<D>(
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgb565,
    fb: &mut D,
    coverage: &mut [u8],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steep = dy > dx;
    let (x0, y0, x1, y1) = if steep {
        (y0, x0, y1, x1)
    } else {
        (x0, y0, x1, y1)
    };
    let (x0, y0, x1, y1) = if x0 > x1 {
        (x1, y1, x0, y0)
    } else {
        (x0, y0, x1, y1)
    };
    let dx = x1 - x0;
    let dy = y1 - y0;
    if dx == 0 {
        let (px, py) = if steep { (y0, x0) } else { (x0, y0) };
        plot_aa_cov(fb, px, py, color, coverage, width, 256);
        return;
    }
    let gradient: i32 = ((dy as i64) << 16) as i32 / dx;
    let mut intery: i32 = y0 << 16;
    for x in x0..=x1 {
        let y_int = intery >> 16;
        let frac_q16 = (intery & 0xFFFF) as u32;
        let cov_top = 256 - (frac_q16 >> 8);
        let cov_bot = frac_q16 >> 8;
        if steep {
            plot_aa_cov(fb, y_int, x, color, coverage, width, cov_top);
            plot_aa_cov(fb, y_int + 1, x, color, coverage, width, cov_bot);
        } else {
            plot_aa_cov(fb, x, y_int, color, coverage, width, cov_top);
            plot_aa_cov(fb, x, y_int + 1, color, coverage, width, cov_bot);
        }
        intery += gradient;
    }
}

/// Coverage-aware single-pixel plot for Wu's lines. Mirrors the four-case
/// logic of `aa_pixel_cov` but without the z-test (lines don't carry depth).
#[cfg(feature = "aa-coverage")]
#[inline(always)]
fn plot_aa_cov<D>(
    fb: &mut D,
    x: i32,
    y: i32,
    color: Rgb565,
    coverage: &mut [u8],
    width: usize,
    coverage_q8: u32,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    if x < 0 || y < 0 || coverage_q8 == 0 {
        return;
    }
    let idx = y as usize * width + x as usize;
    if idx >= coverage.len() {
        return;
    }
    let p = Point::new(x, y);

    if coverage_q8 >= 256 {
        coverage[idx] = 255;
        fb.draw_iter([embedded_graphics_core::Pixel(p, color)])
            .unwrap();
        return;
    }

    let prev_cov = coverage[idx] as u32;

    if prev_cov == 0 {
        let claim_255 = (coverage_q8 * 255) >> 8;
        coverage[idx] = claim_255 as u8;
        fb.draw_iter([embedded_graphics_core::Pixel(p, color)])
            .unwrap();
        return;
    }

    if prev_cov >= 255 {
        let existing = fb.read_pixel(p);
        let result = blend_q8(existing, color, coverage_q8);
        fb.draw_iter([embedded_graphics_core::Pixel(p, result)])
            .unwrap();
        return;
    }

    let remaining = 255 - prev_cov;
    let claim_255 = ((coverage_q8 * 255) >> 8).min(remaining);
    if claim_255 == 0 {
        return;
    }
    let new_total = prev_cov + claim_255;
    let existing = fb.read_pixel(p);
    let blend_factor = (claim_255 * 256) / new_total;
    let result = blend_q8(existing, color, blend_factor);
    coverage[idx] = new_total as u8;
    fb.draw_iter([embedded_graphics_core::Pixel(p, result)])
        .unwrap();
}

/// Composite background color into pixels that weren't fully covered by
/// the AA rasterizer. Run once per frame after all primitives have been
/// drawn via `draw_zbuffered_aa_coverage`.
#[cfg(feature = "aa-coverage")]
pub fn composite_aa_background<D>(
    fb: &mut D,
    coverage: &[u8],
    bg: Rgb565,
    width: usize,
    height: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let cov = coverage[idx];
            if cov == 255 {
                continue; // pixel fully owned by triangles, nothing to do
            }
            let p = Point::new(x as i32, y as i32);
            let final_color = if cov == 0 {
                bg
            } else {
                // Pixel holds the (already pre-composited) accumulated
                // triangle color, weighted by `cov / 255`. Composite the
                // remaining `(255 - cov) / 255` with the bg.
                let tri_color = fb.read_pixel(p);
                // Convert 0..255 to 0..256 q8 coverage for blend_q8.
                let cov_q8 = ((cov as u32) * 256) / 255;
                blend_q8(bg, tri_color, cov_q8)
            };
            fb.draw_iter([embedded_graphics_core::Pixel(p, final_color)])
                .unwrap();
        }
    }
}

/// Z-test + coverage-tracked write of a single AA pixel.
///
/// Four cases, branched on `coverage_q8` (this triangle's pixel coverage)
/// and `coverage[idx]` (sum of prior triangles' coverage at this pixel):
///
/// 1. Full coverage (`coverage_q8 >= 256`): triangle covers the pixel
///    completely. Overwrite. Coverage saturates to 255.
/// 2. Virgin pixel + partial: store pure triangle color; bg gets composited
///    by `composite_aa_background` at end-of-frame using the claimed cov.
/// 3. Already-fully-covered pixel + partial closer triangle: blend the new
///    color over the existing (existing acts as the local "background"
///    since it's the visible scene behind the new triangle).
/// 4. Partially-claimed pixel + partial new: weighted-average accumulation
///    of pure triangle colors. This is the shared-coplanar-edge case.
#[cfg(feature = "aa-coverage")]
#[inline(always)]
fn aa_pixel_cov<D>(
    fb: &mut D,
    x: i32,
    y: i32,
    color: Rgb565,
    z: u32,
    zbuffer: &mut [u32],
    coverage: &mut [u8],
    width: usize,
    coverage_q8: u32,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    if x < 0 || y < 0 || coverage_q8 == 0 {
        return;
    }
    let idx = y as usize * width + x as usize;
    if idx >= zbuffer.len() {
        return;
    }
    if z >= zbuffer[idx].saturating_add(DEPTH_EPSILON) {
        return;
    }

    let p = Point::new(x, y);

    // Case 1: full coverage — overwrite unconditionally.
    if coverage_q8 >= 256 {
        coverage[idx] = 255;
        zbuffer[idx] = z;
        fb.draw_iter([embedded_graphics_core::Pixel(p, color)])
            .unwrap();
        return;
    }

    let prev_cov = coverage[idx] as u32;

    if prev_cov == 0 {
        // Case 2: virgin pixel + partial coverage. Pure triangle color;
        // bg composite at end of frame fills the unclaimed remainder.
        let claim_255 = (coverage_q8 * 255) >> 8;
        coverage[idx] = claim_255 as u8;
        zbuffer[idx] = z;
        fb.draw_iter([embedded_graphics_core::Pixel(p, color)])
            .unwrap();
        return;
    }

    if prev_cov >= 255 {
        // Case 3: pixel was fully claimed by farther geometry. We're closer
        // (z-test passed). Anti-alias the new triangle's edge against the
        // existing pixel as if it were the local background. Total coverage
        // stays at 255 — no bg composite needed.
        let existing = fb.read_pixel(p);
        let result = blend_q8(existing, color, coverage_q8);
        zbuffer[idx] = z;
        fb.draw_iter([embedded_graphics_core::Pixel(p, result)])
            .unwrap();
        return;
    }

    // Case 4: partially-claimed pixel + partial new triangle. Weighted-
    // average accumulation. This is the coplanar-shared-edge case.
    let remaining = 255 - prev_cov;
    let claim_255 = ((coverage_q8 * 255) >> 8).min(remaining);
    if claim_255 == 0 {
        return;
    }
    let new_total = prev_cov + claim_255;
    let existing = fb.read_pixel(p);
    let blend_factor = (claim_255 * 256) / new_total;
    let result = blend_q8(existing, color, blend_factor);
    coverage[idx] = new_total as u8;
    zbuffer[idx] = z;
    fb.draw_iter([embedded_graphics_core::Pixel(p, result)])
        .unwrap();
}

#[cfg(feature = "aa-coverage")]
#[inline(always)]
fn fill_triangle_zbuffered_aa_cov<D>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    coverage: &mut [u8],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_aa_cov(
            p1_eg, p2_eg, p3_eg, z1_int, z2_int, z3_int, color, fb, zbuffer, coverage, width,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_aa_cov(
            p1_eg, p2_eg, p3_eg, z1_int, z2_int, z3_int, color, fb, zbuffer, coverage, width,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        fill_bottom_flat_aa_cov(
            p1_eg, p2_eg, p4, z1_int, z2_int, z4_int, color, fb, zbuffer, coverage, width,
        );
        fill_top_flat_aa_cov(
            p2_eg, p4, p3_eg, z2_int, z4_int, z3_int, color, fb, zbuffer, coverage, width,
        );
    }
}

#[cfg(feature = "aa-coverage")]
#[inline(always)]
fn fill_bottom_flat_aa_cov<D>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    coverage: &mut [u8],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let z_left = (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;
        let z_right = (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;

        aa_scanline_cov(
            curx1, curx2, scanline_y, z_left, z_right, color, fb, zbuffer, coverage, width,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[cfg(feature = "aa-coverage")]
#[inline(always)]
fn fill_top_flat_aa_cov<D>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    coverage: &mut [u8],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let z_left = (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;
        let z_right = (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32;

        aa_scanline_cov(
            curx1, curx2, scanline_y, z_left, z_right, color, fb, zbuffer, coverage, width,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

#[cfg(feature = "aa-coverage")]
#[inline(always)]
fn aa_scanline_cov<D>(
    cx1: i32,
    cx2: i32,
    y: i32,
    z_left: u32,
    z_right: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    coverage: &mut [u8],
    width: usize,
) where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let (left_fx, right_fx, z_l, z_r) = if cx1 <= cx2 {
        (cx1, cx2, z_left, z_right)
    } else {
        (cx2, cx1, z_right, z_left)
    };

    let l_int = left_fx >> 16;
    let r_int = right_fx >> 16;
    let l_frac_q16 = (left_fx & 0xFFFF) as u32;
    let r_frac_q16 = (right_fx & 0xFFFF) as u32;
    let span = r_int - l_int;

    if l_int == r_int {
        let cov_q16 = r_frac_q16.saturating_sub(l_frac_q16);
        aa_pixel_cov(
            fb,
            l_int,
            y,
            color,
            z_l,
            zbuffer,
            coverage,
            width,
            cov_q16 >> 8,
        );
        return;
    }

    let left_cov_q8 = 256 - (l_frac_q16 >> 8);
    aa_pixel_cov(
        fb,
        l_int,
        y,
        color,
        z_l,
        zbuffer,
        coverage,
        width,
        left_cov_q8,
    );

    if span > 1 {
        for x in (l_int + 1)..r_int {
            // Inner pixels are full coverage (q8 = 256). Use the same code
            // path as boundary pixels so coverage tracks correctly.
            let t_num = (x - l_int) as i64;
            let t_den = span as i64;
            let z = (z_l as i64 + ((z_r as i64 - z_l as i64) * t_num / t_den)) as u32;
            aa_pixel_cov(fb, x, y, color, z, zbuffer, coverage, width, 256);
        }
    }

    if r_frac_q16 > 0 {
        let right_cov_q8 = r_frac_q16 >> 8;
        aa_pixel_cov(
            fb,
            r_int,
            y,
            color,
            z_r,
            zbuffer,
            coverage,
            width,
            right_cov_q8,
        );
    }
}

/// Wu's anti-aliased line algorithm.
///
/// Walks the major axis one integer step at a time; at each step writes two
/// pixels straddling the line with complementary fractional coverage.
#[cfg(feature = "aa")]
pub fn draw_line_aa<D>(x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb565, fb: &mut D)
where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steep = dy > dx;
    let (x0, y0, x1, y1) = if steep {
        (y0, x0, y1, x1)
    } else {
        (x0, y0, x1, y1)
    };
    let (x0, y0, x1, y1) = if x0 > x1 {
        (x1, y1, x0, y0)
    } else {
        (x0, y0, x1, y1)
    };
    let dx = x1 - x0;
    let dy = y1 - y0;
    if dx == 0 {
        // Single pixel
        let (px, py) = if steep { (y0, x0) } else { (x0, y0) };
        plot_aa(fb, px, py, color, 256);
        return;
    }
    // 16.16 fixed-point gradient
    let gradient: i32 = ((dy as i64) << 16) as i32 / dx;
    // Start at exact (x0, y0); intery accumulates the y position in 16.16.
    let mut intery: i32 = y0 << 16;
    for x in x0..=x1 {
        let y_int = intery >> 16;
        let frac_q16 = (intery & 0xFFFF) as u32;
        let cov_top = 256 - (frac_q16 >> 8); // pixel at y_int
        let cov_bot = frac_q16 >> 8; //         pixel at y_int + 1
        if steep {
            plot_aa(fb, y_int, x, color, cov_top);
            plot_aa(fb, y_int + 1, x, color, cov_bot);
        } else {
            plot_aa(fb, x, y_int, color, cov_top);
            plot_aa(fb, x, y_int + 1, color, cov_bot);
        }
        intery += gradient;
    }
}

#[cfg(feature = "aa")]
#[inline(always)]
fn plot_aa<D>(fb: &mut D, x: i32, y: i32, color: Rgb565, coverage_q8: u32)
where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    if coverage_q8 == 0 {
        return;
    }
    let final_color = if coverage_q8 >= 256 {
        color
    } else {
        let bg = fb.read_pixel(Point::new(x, y));
        blend_q8(bg, color, coverage_q8)
    };
    fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
        .unwrap();
}

// Z-buffered drawing function with optional fog and dithering effects
// Requires a TextureManager for textured primitives
#[inline]
pub fn draw_zbuffered_with_effects<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::ColoredTriangleWithDepth {
            mut points,
            mut depths,
            color,
        } => {
            // Sort vertices by y coordinate (and corresponding depths)
            if points[0].y > points[1].y {
                points.swap(0, 1);
                depths.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                depths.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                depths.swap(1, 2);
            }

            let [p1, p2, p3] = points;
            let [z1, z2, z3] = depths;

            // Off-screen culling.
            let scr_w = width as i32;
            let scr_h = (zbuffer.len() / width) as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            fill_triangle_zbuffered(
                p1,
                p2,
                p3,
                z1,
                z2,
                z3,
                color,
                fb,
                zbuffer,
                width,
                fog_config,
                dither_config,
            );
        }
        DrawPrimitive::GouraudTriangleWithDepth {
            mut points,
            mut depths,
            mut colors,
        } => {
            // Sort vertices by y coordinate (and corresponding depths and colors)
            if points[0].y > points[1].y {
                points.swap(0, 1);
                depths.swap(0, 1);
                colors.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                depths.swap(0, 2);
                colors.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                depths.swap(1, 2);
                colors.swap(1, 2);
            }

            let [p1, p2, p3] = points;
            let [z1, z2, z3] = depths;
            let [c1, c2, c3] = colors;

            // Off-screen culling.
            let scr_w = width as i32;
            let scr_h = (zbuffer.len() / width) as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            fill_triangle_zbuffered_gouraud(
                p1,
                p2,
                p3,
                z1,
                z2,
                z3,
                c1,
                c2,
                c3,
                fb,
                zbuffer,
                width,
                fog_config,
                dither_config,
            );
        }
        // Textured triangles require texture manager - fall back to regular drawing
        DrawPrimitive::TexturedTriangle { .. }
        | DrawPrimitive::TexturedTriangleWithDepth { .. } => {
            // These variants require a texture manager which isn't provided here
            // Use draw_zbuffered_with_textures() instead
        }
        // For other primitives, fall back to regular drawing
        _ => draw(primitive, fb),
    }
}

// Z-buffered drawing function with textures, fog, and dithering effects
#[inline]
pub fn draw_zbuffered_with_textures<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
    const N: usize,
>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    texture_manager: &crate::texture::TextureManager<N>,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::TexturedTriangleWithDepth {
            mut points,
            mut depths,
            mut ws,
            mut uvs,
            texture_id,
        } => {
            // Get texture from manager
            if let Some(texture) = texture_manager.get(texture_id) {
                // Sort vertices by y coordinate (and corresponding depths, ws, and UVs)
                if points[0].y > points[1].y {
                    points.swap(0, 1);
                    depths.swap(0, 1);
                    ws.swap(0, 1);
                    uvs.swap(0, 1);
                }
                if points[0].y > points[2].y {
                    points.swap(0, 2);
                    depths.swap(0, 2);
                    ws.swap(0, 2);
                    uvs.swap(0, 2);
                }
                if points[1].y > points[2].y {
                    points.swap(1, 2);
                    depths.swap(1, 2);
                    ws.swap(1, 2);
                    uvs.swap(1, 2);
                }

                let [p1, p2, p3] = points;
                let [z1, z2, z3] = depths;
                let [w1, w2, w3] = ws;
                let [uv1, uv2, uv3] = uvs;

                // Off-screen culling.
                let scr_w = width as i32;
                let scr_h = (zbuffer.len() / width) as i32;
                if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                    return;
                }
                if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                    return;
                }
                if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                    return;
                }
                if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                    return;
                }

                fill_triangle_zbuffered_textured(
                    p1,
                    p2,
                    p3,
                    z1,
                    z2,
                    z3,
                    w1,
                    w2,
                    w3,
                    uv1,
                    uv2,
                    uv3,
                    texture,
                    fb,
                    zbuffer,
                    width,
                    fog_config,
                    dither_config,
                );
            }
        }
        // For other primitives, fall back to regular z-buffered drawing
        _ => draw_zbuffered_with_effects(primitive, fb, zbuffer, width, fog_config, dither_config),
    }
}

#[inline(always)]
fn fill_triangle_zbuffered<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    // Convert to embedded_graphics Points
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    // Convert float depths to fixed-point integers (16.16 format)
    // This avoids floating-point operations in the inner loop
    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    // Handle flat triangles
    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else {
        // Split into two flat triangles
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;

        fill_bottom_flat_triangle_zbuffered(
            p1_eg,
            p2_eg,
            p4,
            z1_int,
            z2_int,
            z4_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
        fill_top_flat_triangle_zbuffered(
            p2_eg,
            p4,
            p3_eg,
            z2_int,
            z4_int,
            z3_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    }
}

// Gouraud-shaded triangle with z-buffering
#[inline(always)]
fn fill_triangle_zbuffered_gouraud<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    c1: embedded_graphics_core::pixelcolor::Rgb565,
    c2: embedded_graphics_core::pixelcolor::Rgb565,
    c3: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    // Convert to embedded_graphics Points
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    // Convert float depths to fixed-point integers (16.16 format)
    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    // Handle flat triangles
    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered_gouraud(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            c1,
            c2,
            c3,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered_gouraud(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            c1,
            c2,
            c3,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else {
        // Split into two flat triangles
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        let c4 = interpolate_color(c1, c3, t);

        fill_bottom_flat_triangle_zbuffered_gouraud(
            p1_eg,
            p2_eg,
            p4,
            z1_int,
            z2_int,
            z4_int,
            c1,
            c2,
            c4,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
        fill_top_flat_triangle_zbuffered_gouraud(
            p2_eg,
            p4,
            p3_eg,
            z2_int,
            z4_int,
            z3_int,
            c2,
            c4,
            c3,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    }
}

#[inline(always)]
fn fill_bottom_flat_triangle_zbuffered<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    // Use fixed-point arithmetic (16.16 format) for edge slopes
    // This avoids floating-point operations entirely
    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16; // Fixed-point
    let mut curx2 = p1.x << 16; // Fixed-point

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        // Integer interpolation for Z using only integer math
        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        draw_scanline_zbuffered(
            curx1 >> 16, // Convert back from fixed-point
            curx2 >> 16, // Convert back from fixed-point
            scanline_y,
            z_left,
            z_right,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[inline(always)]
fn fill_top_flat_triangle_zbuffered<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    // Use fixed-point arithmetic (16.16 format) for edge slopes
    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16; // Fixed-point
    let mut curx2 = p3.x << 16; // Fixed-point

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        // Integer interpolation for Z using only integer math
        let z_left = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z2
        };

        draw_scanline_zbuffered(
            curx1 >> 16, // Convert back from fixed-point
            curx2 >> 16, // Convert back from fixed-point
            scanline_y,
            z_left,
            z_right,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

// Gouraud shaded bottom-flat triangle with z-buffering
#[inline(always)]
fn fill_bottom_flat_triangle_zbuffered_gouraud<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    c1: embedded_graphics_core::pixelcolor::Rgb565,
    c2: embedded_graphics_core::pixelcolor::Rgb565,
    c3: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        // Interpolate Z values
        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        // Interpolate colors
        let color_left = interpolate_color(c1, c2, t);
        let color_right = interpolate_color(c1, c3, t);

        draw_scanline_zbuffered_gouraud(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            color_left,
            color_right,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

// Gouraud shaded top-flat triangle with z-buffering
#[inline(always)]
fn fill_top_flat_triangle_zbuffered_gouraud<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    c1: embedded_graphics_core::pixelcolor::Rgb565,
    c2: embedded_graphics_core::pixelcolor::Rgb565,
    c3: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        // Interpolate Z values
        let z_left = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z2
        };

        // Interpolate colors
        let color_left = interpolate_color(c1, c3, t);
        let color_right = interpolate_color(c2, c3, t);

        draw_scanline_zbuffered_gouraud(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            color_left,
            color_right,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

// Draw scanline with Gouraud shading and z-buffering
#[inline(always)]
fn draw_scanline_zbuffered_gouraud<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    color1: embedded_graphics_core::pixelcolor::Rgb565,
    color2: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let start = x1.min(x2);
    let end = x1.max(x2);
    let span_width = end - start;

    for x in start..=end {
        if x < 0 || y < 0 {
            continue;
        }

        let zbuffer_idx = y as usize * width + x as usize;
        if zbuffer_idx >= zbuffer.len() {
            continue;
        }

        // Interpolate Z value
        let t = if span_width > 0 {
            (x - start) as f32 / span_width as f32
        } else {
            0.0
        };
        let z = if span_width > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * (x - start) as i64 / span_width as i64)) as u32
        } else {
            z1
        };

        // Z-test with epsilon to prevent Z-fighting on nearly coplanar faces
        // We make the test slightly more permissive by allowing pixels within epsilon range
        if z < zbuffer[zbuffer_idx].saturating_add(DEPTH_EPSILON) {
            zbuffer[zbuffer_idx] = z;

            // Interpolate color
            let mut final_color = interpolate_color(color1, color2, t);

            // Apply effects in order: fog first, then dithering
            if let Some(fog) = fog_config {
                final_color = fog.apply(final_color, z);
            }

            if let Some(dither) = dither_config {
                final_color = dither.apply(final_color, x, y);
            }

            fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                .unwrap();
        }
    }
}

#[inline(always)]
fn draw_scanline_zbuffered<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let start = x1.min(x2);
    let end = x1.max(x2);

    for x in start..=end {
        if x < 0 || y < 0 {
            continue;
        }

        let zbuffer_idx = y as usize * width + x as usize;
        if zbuffer_idx >= zbuffer.len() {
            continue;
        }

        // Integer interpolation for Z (fixed-point 16.16 format)
        // This avoids floating-point in the inner loop
        let span = end - start;
        let z = if span == 0 {
            z1
        } else {
            // Linear interpolation using integer math
            let t_num = (x - start) as i64;
            let t_den = span as i64;
            let z_diff = z2 as i64 - z1 as i64;
            (z1 as i64 + (t_num * z_diff / t_den)) as u32
        };

        // Z-buffer test with epsilon to prevent Z-fighting (closer = smaller depth value)
        // We make the test slightly more permissive by allowing pixels within epsilon range
        if z < zbuffer[zbuffer_idx].saturating_add(DEPTH_EPSILON) {
            zbuffer[zbuffer_idx] = z;

            // Apply effects in order: fog first, then dithering
            let mut final_color = color;

            if let Some(fog) = fog_config {
                final_color = fog.apply(final_color, z);
            }

            if let Some(dither) = dither_config {
                final_color = dither.apply(final_color, x, y);
            }

            fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                .unwrap();
        }
    }
}

// Textured triangle rendering with z-buffering
#[inline(always)]
fn fill_triangle_zbuffered_textured<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    // Convert to embedded_graphics Points
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    // Convert float depths to fixed-point integers (16.16 format)
    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    // Handle flat triangles
    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered_textured(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered_textured(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else {
        // Split into two flat triangles
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        // Interpolate W at split point
        let w4 = w1 + t * (w3 - w1);
        // Interpolate UV at split point
        let uv4 = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];

        fill_bottom_flat_triangle_zbuffered_textured(
            p1_eg,
            p2_eg,
            p4,
            z1_int,
            z2_int,
            z4_int,
            w1,
            w2,
            w4,
            uv1,
            uv2,
            uv4,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
        fill_top_flat_triangle_zbuffered_textured(
            p2_eg,
            p4,
            p3_eg,
            z2_int,
            z4_int,
            z3_int,
            w2,
            w4,
            w3,
            uv2,
            uv4,
            uv3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    }
}

// Textured bottom-flat triangle with z-buffering
#[inline(always)]
fn fill_bottom_flat_triangle_zbuffered_textured<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        // Interpolate Z values
        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        // Interpolate W values
        let w_left = w1 + t * (w2 - w1);
        let w_right = w1 + t * (w3 - w1);

        // Interpolate UVs
        let uv_left = [
            uv1[0] + t * (uv2[0] - uv1[0]),
            uv1[1] + t * (uv2[1] - uv1[1]),
        ];
        let uv_right = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];

        draw_scanline_zbuffered_textured(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            w_left,
            w_right,
            uv_left,
            uv_right,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

// Textured top-flat triangle with z-buffering
#[inline(always)]
fn fill_top_flat_triangle_zbuffered_textured<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        // Interpolate Z values
        let z_left = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z2
        };

        // Interpolate W values
        let w_left = w1 + t * (w3 - w1);
        let w_right = w2 + t * (w3 - w2);

        // Interpolate UVs
        let uv_left = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];
        let uv_right = [
            uv2[0] + t * (uv3[0] - uv2[0]),
            uv2[1] + t * (uv3[1] - uv2[1]),
        ];

        draw_scanline_zbuffered_textured(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            w_left,
            w_right,
            uv_left,
            uv_right,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

// Draw scanline with texture mapping and z-buffering
#[inline(always)]
fn draw_scanline_zbuffered_textured<
    D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    w1: f32,
    w2: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [u32],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let start = x1.min(x2);
    let end = x1.max(x2);
    let span_width = end - start;

    for x in start..=end {
        if x < 0 || y < 0 {
            continue;
        }

        let zbuffer_idx = y as usize * width + x as usize;
        if zbuffer_idx >= zbuffer.len() {
            continue;
        }

        // Interpolate Z value
        let t = if span_width > 0 {
            (x - start) as f32 / span_width as f32
        } else {
            0.0
        };
        let z = if span_width > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * (x - start) as i64 / span_width as i64)) as u32
        } else {
            z1
        };

        // Z-test with epsilon to prevent Z-fighting on nearly coplanar faces
        // We make the test slightly more permissive by allowing pixels within epsilon range
        if z < zbuffer[zbuffer_idx].saturating_add(DEPTH_EPSILON) {
            zbuffer[zbuffer_idx] = z;

            // Perspective-correct UV interpolation
            let ow1 = 1.0 / w1;
            let ow2 = 1.0 / w2;
            let one_over_w = ow1 + t * (ow2 - ow1);
            let u = (uv1[0] * ow1 + t * (uv2[0] * ow2 - uv1[0] * ow1)) / one_over_w;
            let v = (uv1[1] * ow1 + t * (uv2[1] * ow2 - uv1[1] * ow1)) / one_over_w;

            // Sample texture
            let mut final_color = texture.sample(u, v);

            // Apply effects in order: fog first, then dithering
            if let Some(fog) = fog_config {
                final_color = fog.apply(final_color, z);
            }

            if let Some(dither) = dither_config {
                final_color = dither.apply(final_color, x, y);
            }

            fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_graphics_core::pixelcolor::Rgb565;
    use embedded_graphics_core::prelude::*;
    use nalgebra::Point2;

    // Mock framebuffer for testing
    struct MockFramebuffer {
        pixels: std::vec::Vec<(i32, i32, Rgb565)>,
    }

    impl MockFramebuffer {
        fn new() -> Self {
            Self {
                pixels: std::vec::Vec::new(),
            }
        }

        fn contains_pixel(&self, x: i32, y: i32) -> bool {
            self.pixels.iter().any(|(px, py, _)| *px == x && *py == y)
        }

        fn pixel_count(&self) -> usize {
            self.pixels.len()
        }
    }

    impl DrawTarget for MockFramebuffer {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
        {
            for pixel in pixels {
                self.pixels.push((pixel.0.x, pixel.0.y, pixel.1));
            }
            Ok(())
        }
    }

    impl OriginDimensions for MockFramebuffer {
        fn size(&self) -> Size {
            Size::new(640, 480)
        }
    }

    #[test]
    fn test_draw_point() {
        let mut fb = MockFramebuffer::new();
        let point = Point2::new(10, 20);
        let color = Rgb565::CSS_RED;

        draw(DrawPrimitive::ColoredPoint(point, color), &mut fb);

        assert_eq!(fb.pixel_count(), 1);
        assert!(fb.contains_pixel(10, 20));
    }

    #[test]
    fn test_draw_line_horizontal() {
        let mut fb = MockFramebuffer::new();
        let p1 = Point2::new(10, 20);
        let p2 = Point2::new(20, 20);
        let color = Rgb565::CSS_GREEN;

        draw(DrawPrimitive::Line([p1, p2], color), &mut fb);

        // Should draw pixels along the horizontal line
        assert!(fb.pixel_count() >= 10); // At least 10 pixels
        assert!(fb.contains_pixel(10, 20));
        assert!(fb.contains_pixel(20, 20));
    }

    #[test]
    fn test_draw_line_vertical() {
        let mut fb = MockFramebuffer::new();
        let p1 = Point2::new(10, 10);
        let p2 = Point2::new(10, 20);
        let color = Rgb565::CSS_BLUE;

        draw(DrawPrimitive::Line([p1, p2], color), &mut fb);

        // Should draw pixels along the vertical line
        assert!(fb.pixel_count() >= 10);
        assert!(fb.contains_pixel(10, 10));
        assert!(fb.contains_pixel(10, 20));
    }

    #[test]
    fn test_draw_line_diagonal() {
        let mut fb = MockFramebuffer::new();
        let p1 = Point2::new(0, 0);
        let p2 = Point2::new(10, 10);
        let color = Rgb565::CSS_WHITE;

        draw(DrawPrimitive::Line([p1, p2], color), &mut fb);

        // Should draw pixels along the diagonal
        assert!(fb.pixel_count() >= 10);
        assert!(fb.contains_pixel(0, 0));
        assert!(fb.contains_pixel(10, 10));
    }

    #[test]
    fn test_draw_triangle_flat_bottom() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(50, 10), // Top vertex
            Point2::new(30, 30), // Bottom left
            Point2::new(70, 30), // Bottom right
        ];
        let color = Rgb565::CSS_YELLOW;

        draw(DrawPrimitive::ColoredTriangle(vertices, color), &mut fb);

        // Should draw multiple pixels for the filled triangle
        let count = fb.pixel_count();
        assert!(count > 0, "Expected pixels to be drawn, got {}", count);
        // Top vertex should be drawn
        assert!(fb.contains_pixel(50, 10));
    }

    #[test]
    fn test_draw_triangle_flat_top() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(30, 10), // Top left
            Point2::new(70, 10), // Top right
            Point2::new(50, 30), // Bottom vertex
        ];
        let color = Rgb565::CSS_CYAN;

        draw(DrawPrimitive::ColoredTriangle(vertices, color), &mut fb);

        // Should draw multiple pixels for the filled triangle
        assert!(fb.pixel_count() > 20);
        assert!(fb.contains_pixel(50, 30));
    }

    #[test]
    fn test_draw_triangle_general() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(50, 10),
            Point2::new(30, 30),
            Point2::new(80, 40),
        ];
        let color = Rgb565::CSS_MAGENTA;

        draw(DrawPrimitive::ColoredTriangle(vertices, color), &mut fb);

        // Should draw many pixels for the filled triangle
        assert!(fb.pixel_count() > 30);
    }

    #[test]
    fn test_triangle_vertex_sorting() {
        let mut fb = MockFramebuffer::new();
        // Vertices in reverse y order
        let vertices = [
            Point2::new(50, 30), // Bottom (will be sorted to top)
            Point2::new(30, 10), // Top
            Point2::new(70, 20), // Middle
        ];
        let color = Rgb565::CSS_WHITE;

        // Should not panic and should draw the triangle correctly
        draw(DrawPrimitive::ColoredTriangle(vertices, color), &mut fb);

        assert!(fb.pixel_count() > 10);
    }

    #[test]
    fn test_draw_multiple_primitives() {
        let mut fb = MockFramebuffer::new();

        draw(
            DrawPrimitive::ColoredPoint(Point2::new(5, 5), Rgb565::CSS_RED),
            &mut fb,
        );
        draw(
            DrawPrimitive::Line(
                [Point2::new(10, 10), Point2::new(20, 20)],
                Rgb565::CSS_GREEN,
            ),
            &mut fb,
        );

        // Should have pixels from both primitives
        assert!(fb.pixel_count() > 11); // 1 point + at least 10 from line
        assert!(fb.contains_pixel(5, 5));
    }
}
