use crate::ZDepth;
use embedded_graphics_core::pixelcolor::Rgb565;

use super::FragmentShader;
use super::blend::fast_blend_rgb565;

/// Configuration for screen-space water reflections (SSR).
#[derive(Debug, Clone, Copy)]
pub struct WaterReflectConfig<'a> {
    /// Reference color buffer from the current or previous frame/field.
    pub reference_buffer: &'a [Rgb565],
    /// Buffer width in pixels.
    pub width: usize,
    /// Buffer height in pixels.
    pub height: usize,
    /// Screen row of the waterline/horizon axis.
    pub waterline_y: i32,
    /// Base water surface color.
    pub surface_color: Rgb565,
    /// Blend factor towards `surface_color` (0 = 100% reflection, 255 = 100% flat surface color).
    pub alpha: u8,
    /// Wave ripple displacement amplitude in pixels.
    pub ripple_amplitude: i32,
    /// Wave phase offset for animation.
    pub ripple_phase: i32,
}

impl<'a> WaterReflectConfig<'a> {
    /// Create a new SSR water reflection configuration.
    pub fn new(
        reference_buffer: &'a [Rgb565],
        width: usize,
        height: usize,
        waterline_y: i32,
        surface_color: Rgb565,
        alpha: u8,
    ) -> Self {
        Self {
            reference_buffer,
            width,
            height,
            waterline_y,
            surface_color,
            alpha,
            ripple_amplitude: 0,
            ripple_phase: 0,
        }
    }

    /// Set ripple wave parameters for animated water surface distortion.
    pub fn with_ripples(mut self, amplitude: i32, phase: i32) -> Self {
        self.ripple_amplitude = amplitude;
        self.ripple_phase = phase;
        self
    }

    /// Sample the mirrored pixel from the reference buffer with optional ripple distortion.
    #[inline]
    pub fn sample_reflection(&self, x: i32, y: i32) -> Rgb565 {
        if self.reference_buffer.is_empty() || self.width == 0 || self.height == 0 {
            return self.surface_color;
        }

        // Mirror formula: mirror_y = 2 * waterline_y - y
        let raw_mirror_y = 2 * self.waterline_y - y;

        // Apply simple integer ripple displacement if amplitude > 0
        let (ripple_x, ripple_y) = if self.ripple_amplitude > 0 {
            let offset_x = ((x + self.ripple_phase) & 7) - 3;
            let offset_y = ((y + self.ripple_phase) & 7) - 3;
            (
                (offset_x * self.ripple_amplitude) / 4,
                (offset_y * self.ripple_amplitude) / 4,
            )
        } else {
            (0, 0)
        };

        let mirror_x = (x + ripple_x).clamp(0, self.width as i32 - 1) as usize;
        let mirror_y = (raw_mirror_y + ripple_y).clamp(0, self.height as i32 - 1) as usize;

        let idx = mirror_y * self.width + mirror_x;
        if idx < self.reference_buffer.len() {
            self.reference_buffer[idx]
        } else {
            self.surface_color
        }
    }
}

/// Fragment shader decorator for screen-space temporal water reflections.
#[derive(Debug, Clone, Copy)]
pub struct WaterReflectShader<'a, S> {
    pub inner: S,
    pub config: &'a WaterReflectConfig<'a>,
}

impl<'a, S: FragmentShader> FragmentShader for WaterReflectShader<'a, S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: ZDepth, interps: Self::Interpolants) -> Rgb565 {
        let base_color = self.inner.shade(x, y, z, interps);
        if y < self.config.waterline_y {
            return base_color;
        }

        let reflected_color = self.config.sample_reflection(x, y);
        if self.config.alpha == 0 {
            reflected_color
        } else if self.config.alpha == 255 {
            base_color
        } else {
            fast_blend_rgb565(reflected_color, base_color, self.config.alpha)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::FlatColorShader;
    use embedded_graphics_core::pixelcolor::RgbColor;

    #[test]
    fn test_water_reflect_mirror_sampling() {
        // 4x4 buffer where row 0 is RED, row 1 is GREEN, row 2 is BLUE, row 3 is WHITE
        let mut buffer = [Rgb565::BLACK; 16];
        for x in 0..4 {
            buffer[x] = Rgb565::RED;
            buffer[4 + x] = Rgb565::GREEN;
            buffer[2 * 4 + x] = Rgb565::BLUE;
            buffer[3 * 4 + x] = Rgb565::WHITE;
        }

        // Waterline at row 2:
        // For y = 2 -> mirror_y = 2*2 - 2 = 2 (BLUE)
        // For y = 3 -> mirror_y = 2*2 - 3 = 1 (GREEN)
        let config = WaterReflectConfig::new(&buffer, 4, 4, 2, Rgb565::CYAN, 0);
        let shader = WaterReflectShader {
            inner: FlatColorShader {
                color: Rgb565::CYAN,
            },
            config: &config,
        };

        let color_above = shader.shade(0, 1, 0, ());
        assert_eq!(color_above, Rgb565::CYAN); // Above waterline returns inner base color

        let color_at_waterline = shader.shade(0, 2, 0, ());
        assert_eq!(color_at_waterline, Rgb565::BLUE);

        let color_below_waterline = shader.shade(0, 3, 0, ());
        assert_eq!(color_below_waterline, Rgb565::GREEN);
    }
}
