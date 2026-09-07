//! Atmospheric lens-flare effect with occlusion picking.
//!
//! Inspired by Jet's `LensFlare.hpp` and `Picking.hpp`. Projects a directional light source (sun)
//! to screen space, checks occlusion via a single depth pick query, and renders a chain of
//! optical flare elements along the sun-to-screen-center vector.

use core::fmt::Debug;
use embedded_graphics_core::{
    Pixel,
    draw_target::DrawTarget,
    pixelcolor::{Rgb565, RgbColor},
    prelude::Point,
};
use heapless::Vec;
use nalgebra::{Point3, Vector3};

use crate::{
    camera::Camera,
    renderer::{PickQuery, PickResult},
    shader::blend::fast_blend_rgb565,
};

/// A single flare element along the sun-to-screen-center axis.
#[derive(Debug, Clone, Copy)]
pub struct FlareElement {
    /// Position along the sun -> center axis.
    /// * `0.0`: pinned to the projected sun position.
    /// * `0.5`: halfway between sun and screen center.
    /// * `1.0`: at screen center.
    /// * `-0.2`: 20% past the sun away from center.
    /// * `1.3`: 30% past the center away from sun.
    pub axis_t: f32,
    /// Radius of the flare circle in screen pixels.
    pub radius: i32,
    /// Color of the flare element.
    pub color: Rgb565,
    /// Maximum opacity (0..=255).
    pub base_alpha: u8,
}

impl FlareElement {
    pub const fn new(axis_t: f32, radius: i32, color: Rgb565, base_alpha: u8) -> Self {
        Self {
            axis_t,
            radius,
            color,
            base_alpha,
        }
    }
}

/// Atmospheric lens flare manager.
pub struct LensFlare<const MAX_ELEMS: usize> {
    /// World-space unit direction pointing toward the sun / light source.
    pub sun_dir: Vector3<f32>,
    /// Array of flare elements.
    pub elements: Vec<FlareElement, MAX_ELEMS>,
    /// Speed of fade in/out transitions (units per second).
    pub fade_speed: f32,
    /// Current smoothed visibility in 0.0..=1.0.
    pub visibility: f32,
    sun_screen: Option<(i32, i32)>,
}

impl<const MAX_ELEMS: usize> LensFlare<MAX_ELEMS> {
    /// Create a new lens flare manager.
    pub fn new(sun_dir: Vector3<f32>, fade_speed: f32) -> Self {
        Self {
            sun_dir: sun_dir.normalize(),
            elements: Vec::new(),
            fade_speed,
            visibility: 0.0,
            sun_screen: None,
        }
    }

    /// Add a flare element to the chain.
    pub fn add_element(&mut self, element: FlareElement) -> bool {
        self.elements.push(element).is_ok()
    }

    /// Prepare the lens flare for the current frame.
    ///
    /// Projects the sun into screen coordinates. If visible on screen and in front of the camera,
    /// returns a `PickQuery` to test for geometric occlusion.
    pub fn prepare(
        &mut self,
        camera: &Camera,
        screen_w: usize,
        screen_h: usize,
    ) -> Option<PickQuery> {
        let sun_world = camera.position + self.sun_dir * 1000.0;
        let vp = camera.vp_matrix;

        let clip = vp * Point3::new(sun_world.x, sun_world.y, sun_world.z).to_homogeneous();
        if clip.w <= 0.001 {
            self.sun_screen = None;
            return None;
        }

        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;

        // Check if projected sun is within the viewport bounds
        if !(-1.2..=1.2).contains(&ndc_x) || !(-1.2..=1.2).contains(&ndc_y) {
            self.sun_screen = None;
            return None;
        }

        let sx = ((ndc_x * 0.5 + 0.5) * screen_w as f32) as i32;
        let sy = ((1.0 - (ndc_y * 0.5 + 0.5)) * screen_h as f32) as i32;

        self.sun_screen = Some((sx, sy));

        if sx >= 0 && sx < screen_w as i32 && sy >= 0 && sy < screen_h as i32 {
            Some(PickQuery::new(sx, sy))
        } else {
            None
        }
    }

    /// Update the flare visibility state using the pick result from command execution.
    pub fn update(&mut self, dt: f32, pick_hit: Option<&PickResult>) {
        let target_visibility = if self.sun_screen.is_some() {
            if pick_hit.is_none() {
                1.0
            } else {
                0.0 // Occluded by scene geometry!
            }
        } else {
            0.0
        };

        let delta = self.fade_speed * dt;
        if self.visibility < target_visibility {
            self.visibility = (self.visibility + delta).min(target_visibility);
        } else if self.visibility > target_visibility {
            self.visibility = (self.visibility - delta).max(target_visibility);
        }
    }

    /// Render visible flare elements directly to the target framebuffer.
    pub fn render<D>(&self, fb: &mut D, screen_w: usize, screen_h: usize) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
        D::Error: Debug,
    {
        if self.visibility <= 0.005 {
            return Ok(());
        }

        let Some((sun_x, sun_y)) = self.sun_screen else {
            return Ok(());
        };

        let center_x = screen_w as f32 * 0.5;
        let center_y = screen_h as f32 * 0.5;
        let dir_x = center_x - sun_x as f32;
        let dir_y = center_y - sun_y as f32;

        let w = screen_w as i32;
        let h = screen_h as i32;

        for elem in &self.elements {
            let cx = (sun_x as f32 + dir_x * elem.axis_t) as i32;
            let cy = (sun_y as f32 + dir_y * elem.axis_t) as i32;
            let alpha = ((elem.base_alpha as f32) * self.visibility) as u8;
            if alpha == 0 || elem.radius <= 0 {
                continue;
            }

            let r = elem.radius;
            let min_x = (cx - r).max(0);
            let max_x = (cx + r).min(w - 1);
            let min_y = (cy - r).max(0);
            let max_y = (cy + r).min(h - 1);

            let r_sq = r * r;
            for py in min_y..=max_y {
                let dy = py - cy;
                for px in min_x..=max_x {
                    let dx = px - cx;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq <= r_sq {
                        // Soft falloff towards edges
                        let falloff = 1.0 - (dist_sq as f32 / r_sq as f32);
                        let final_alpha = ((alpha as f32) * falloff) as u8;
                        if final_alpha > 0 {
                            let blended = fast_blend_rgb565(Rgb565::BLACK, elem.color, final_alpha);
                            fb.draw_iter([Pixel(Point::new(px, py), blended)])?;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::geometry::{OriginDimensions, Size};

    struct MockFb {
        pixels: [Rgb565; 400],
    }

    impl OriginDimensions for MockFb {
        fn size(&self) -> Size {
            Size::new(20, 20)
        }
    }

    impl DrawTarget for MockFb {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(point, color) in pixels {
                if point.x >= 0 && point.x < 20 && point.y >= 0 && point.y < 20 {
                    let idx = (point.y * 20 + point.x) as usize;
                    self.pixels[idx] = color;
                }
            }
            Ok(())
        }
    }

    #[test]
    fn test_lens_flare_lifecycle() {
        let mut flare = LensFlare::<4>::new(Vector3::new(0.0, 0.0, -1.0), 10.0);
        flare.add_element(FlareElement::new(0.0, 8, Rgb565::YELLOW, 255));
        flare.add_element(FlareElement::new(0.5, 4, Rgb565::CYAN, 128));

        let camera = Camera::new(320.0 / 240.0);

        let query = flare.prepare(&camera, 320, 240);
        assert!(query.is_some());
        let q = query.unwrap();
        // Camera looks towards -Z by default, so sun at (0,0,-1) maps to center
        assert_eq!(q.x, 160);
        assert_eq!(q.y, 120);

        // Sun unoccluded -> visibility rises
        flare.update(0.1, None);
        assert!(flare.visibility > 0.0);

        // Sun occluded -> visibility drops
        let hit = PickResult {
            x: 160,
            y: 120,
            depth: 100,
            command_index: 0,
        };
        flare.update(0.2, Some(&hit));
        assert_eq!(flare.visibility, 0.0);
    }

    #[test]
    fn test_lens_flare_render() {
        let mut flare = LensFlare::<2>::new(Vector3::new(0.0, 0.0, -1.0), 10.0);
        flare.add_element(FlareElement::new(0.0, 4, Rgb565::WHITE, 255));
        flare.sun_screen = Some((10, 10));
        flare.visibility = 1.0;

        let mut display = MockFb {
            pixels: [Rgb565::BLACK; 400],
        };

        flare.render(&mut display, 20, 20).unwrap();
    }
}
