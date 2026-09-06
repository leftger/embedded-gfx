use crate::retro::{PaletteMode, ScreenTint};
use crate::shader::FragmentShader;
use embedded_graphics_core::pixelcolor::Rgb565;

/// Fragment shader decorator that applies full-screen tint.
#[derive(Debug, Clone, Copy)]
pub struct ScreenTintShader<S> {
    pub inner: S,
    pub tint: ScreenTint,
}

impl<S: FragmentShader> FragmentShader for ScreenTintShader<S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: crate::ZDepth, interps: Self::Interpolants) -> Rgb565 {
        let base = self.inner.shade(x, y, z, interps);
        self.tint.apply(base)
    }
}

/// Fragment shader decorator that applies palette quantization (e.g. RGB332).
#[derive(Debug, Clone, Copy)]
pub struct PaletteShader<S> {
    pub inner: S,
    pub palette_mode: PaletteMode,
}

impl<S: FragmentShader> FragmentShader for PaletteShader<S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: crate::ZDepth, interps: Self::Interpolants) -> Rgb565 {
        let base = self.inner.shade(x, y, z, interps);
        self.palette_mode.apply(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::{FlatColorShader, FragmentShader};
    use embedded_graphics_core::pixelcolor::RgbColor;

    #[test]
    fn test_screen_tint_shader() {
        let base_shader = FlatColorShader {
            color: Rgb565::WHITE,
        };
        let tint = ScreenTint {
            color: Rgb565::GREEN,
            strength: 128,
        };
        let tint_shader = ScreenTintShader {
            inner: base_shader,
            tint,
        };
        let tinted = tint_shader.shade(0, 0, crate::to_zdepth(0), ());
        assert_eq!(tinted, tint.apply(Rgb565::WHITE));
    }

    #[test]
    fn test_palette_shader() {
        let base_shader = FlatColorShader {
            color: Rgb565::new(31, 63, 31),
        };
        let pal_shader = PaletteShader {
            inner: base_shader,
            palette_mode: PaletteMode::Rgb332,
        };
        let quant = pal_shader.shade(0, 0, crate::to_zdepth(0), ());
        assert_eq!(quant, PaletteMode::Rgb332.apply(Rgb565::new(31, 63, 31)));
    }
}
