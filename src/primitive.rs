use embedded_graphics_core::pixelcolor::Rgb565;
use nalgebra::Point2;

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
    TranslucentTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        color: Rgb565,
        alpha: u8,
    },
    /// Screen-door dithered stipple triangle (zero-cost, order-independent transparency).
    ScreenDoorTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        color: Rgb565,
        alpha: u8,
    },
    #[cfg(feature = "lighting")]
    GouraudTriangle {
        points: [Point2<i32>; 3],
        colors: [Rgb565; 3],
    },
    #[cfg(feature = "lighting")]
    GouraudTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        colors: [Rgb565; 3],
    },
    #[cfg(feature = "textured")]
    TexturedTriangle {
        points: [Point2<i32>; 3],
        uvs: [[f32; 2]; 3],
        texture_id: u32,
    },
    #[cfg(feature = "textured")]
    TexturedTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        ws: [f32; 3],
        uvs: [[f32; 2]; 3],
        texture_id: u32,
    },
    #[cfg(feature = "textured")]
    TexturedGouraudTriangleWithDepth {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        ws: [f32; 3],
        uvs: [[f32; 2]; 3],
        colors: [Rgb565; 3],
        texture_id: u32,
    },
    /// Perspective-correct textured triangle with baked lightmap.
    ///
    /// The final pixel colour is:
    /// `clamp(surface.sample(su,sv) × lightmap.sample(lu,lv) + dynamic_tint)`
    /// where `×` is per-channel normalised multiply.
    /// Set `lightmap_id = u32::MAX` to fall back to full-bright surface colour.
    /// Set `dynamic_tint = Rgb565::new(0,0,0)` for no dynamic lighting.
    #[cfg(feature = "textured")]
    LightmappedTriangle {
        points: [Point2<i32>; 3],
        depths: [f32; 3],
        ws: [f32; 3],
        surface_uvs: [[f32; 2]; 3],
        lm_uvs: [[f32; 2]; 3],
        texture_id: u32,
        lightmap_id: u32,
        /// Per-face brightness multiplier in 0..=255 (255 = no darkening).
        brightness: u8,
        /// Additive RGB565 tint from runtime point lights.
        dynamic_tint: Rgb565,
    },
}

impl DrawPrimitive {
    /// Calculate screen-space axis-aligned bounding box (min_x, min_y, max_x, max_y)
    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        match self {
            DrawPrimitive::ColoredPoint(p, _) => (p.x, p.y, p.x, p.y),
            DrawPrimitive::Line([a, b], _) => {
                (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
            }
            DrawPrimitive::ColoredTriangle(points, _)
            | DrawPrimitive::ColoredTriangleWithDepth { points, .. }
            | DrawPrimitive::TranslucentTriangleWithDepth { points, .. }
            | DrawPrimitive::ScreenDoorTriangleWithDepth { points, .. } => {
                let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
                (min_x, min_y, max_x, max_y)
            }
            #[cfg(feature = "lighting")]
            DrawPrimitive::GouraudTriangle { points, .. }
            | DrawPrimitive::GouraudTriangleWithDepth { points, .. } => {
                let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
                (min_x, min_y, max_x, max_y)
            }
            #[cfg(feature = "textured")]
            DrawPrimitive::TexturedTriangle { points, .. }
            | DrawPrimitive::TexturedTriangleWithDepth { points, .. }
            | DrawPrimitive::TexturedGouraudTriangleWithDepth { points, .. }
            | DrawPrimitive::LightmappedTriangle { points, .. } => {
                let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
                (min_x, min_y, max_x, max_y)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::pixelcolor::RgbColor;

    fn pts() -> [Point2<i32>; 3] {
        [Point2::new(-2, 0), Point2::new(3, -4), Point2::new(1, 5)]
    }

    #[test]
    fn primitive_bounds_cover_every_family() {
        let p = pts();
        assert_eq!(
            DrawPrimitive::ColoredPoint(p[0], Rgb565::RED).bounds(),
            (-2, 0, -2, 0)
        );
        assert_eq!(
            DrawPrimitive::Line([p[0], p[2]], Rgb565::RED).bounds(),
            (-2, 0, 1, 5)
        );
        assert_eq!(
            DrawPrimitive::ColoredTriangle(p, Rgb565::RED).bounds(),
            (-2, -4, 3, 5)
        );
        assert_eq!(
            DrawPrimitive::ColoredTriangleWithDepth {
                points: p,
                depths: [1.0; 3],
                color: Rgb565::RED,
            }
            .bounds(),
            (-2, -4, 3, 5)
        );
        assert_eq!(
            DrawPrimitive::TranslucentTriangleWithDepth {
                points: p,
                depths: [1.0; 3],
                color: Rgb565::RED,
                alpha: 128,
            }
            .bounds(),
            (-2, -4, 3, 5)
        );
        assert_eq!(
            DrawPrimitive::ScreenDoorTriangleWithDepth {
                points: p,
                depths: [1.0; 3],
                color: Rgb565::RED,
                alpha: 128,
            }
            .bounds(),
            (-2, -4, 3, 5)
        );

        #[cfg(feature = "lighting")]
        {
            assert_eq!(
                DrawPrimitive::GouraudTriangle {
                    points: p,
                    colors: [Rgb565::RED; 3],
                }
                .bounds(),
                (-2, -4, 3, 5)
            );
            assert_eq!(
                DrawPrimitive::GouraudTriangleWithDepth {
                    points: p,
                    depths: [1.0; 3],
                    colors: [Rgb565::RED; 3],
                }
                .bounds(),
                (-2, -4, 3, 5)
            );
        }

        #[cfg(feature = "textured")]
        {
            let uvs = [[0.0; 2]; 3];
            assert_eq!(
                DrawPrimitive::TexturedTriangle {
                    points: p,
                    uvs,
                    texture_id: 0,
                }
                .bounds(),
                (-2, -4, 3, 5)
            );
            assert_eq!(
                DrawPrimitive::TexturedTriangleWithDepth {
                    points: p,
                    depths: [1.0; 3],
                    ws: [1.0; 3],
                    uvs,
                    texture_id: 0,
                }
                .bounds(),
                (-2, -4, 3, 5)
            );
            assert_eq!(
                DrawPrimitive::TexturedGouraudTriangleWithDepth {
                    points: p,
                    depths: [1.0; 3],
                    ws: [1.0; 3],
                    uvs,
                    colors: [Rgb565::RED; 3],
                    texture_id: 0,
                }
                .bounds(),
                (-2, -4, 3, 5)
            );
            assert_eq!(
                DrawPrimitive::LightmappedTriangle {
                    points: p,
                    depths: [1.0; 3],
                    ws: [1.0; 3],
                    surface_uvs: uvs,
                    lm_uvs: uvs,
                    texture_id: 0,
                    lightmap_id: 0,
                    brightness: 255,
                    dynamic_tint: Rgb565::BLACK,
                }
                .bounds(),
                (-2, -4, 3, 5)
            );
        }
    }
}
