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
