use crate::ZDepth;
use embedded_graphics_core::pixelcolor::Rgb565;

pub mod blend;
pub mod dither;
pub mod fog;
pub mod retro;

pub use blend::{
    fast_blend_rgb565, fast_blend_rgba8888, fast_blend_rgba8888_to_rgb565, reverse_color_rgb565,
    reverse_color_rgba8888,
};
pub use dither::{DitherConfig, DitherShader};
pub use fog::{FogConfig, FogShader};
pub use retro::{PaletteShader, ScreenTintShader};

/// Zero-cost composable fragment shader interface.
pub trait FragmentShader {
    type Interpolants: Copy;

    /// Evaluate fragment color at `(x, y)` with depth `z` and per-pixel interpolants.
    fn shade(&self, x: i32, y: i32, z: ZDepth, interps: Self::Interpolants) -> Rgb565;
}

/// Constant flat color fragment shader.
#[derive(Debug, Clone, Copy)]
pub struct FlatColorShader {
    pub color: Rgb565,
}

impl FragmentShader for FlatColorShader {
    type Interpolants = ();

    #[inline(always)]
    fn shade(&self, _x: i32, _y: i32, _z: ZDepth, _interps: ()) -> Rgb565 {
        self.color
    }
}

/// Gouraud vertex-interpolated color fragment shader.
#[derive(Debug, Clone, Copy)]
pub struct GouraudShader;

impl FragmentShader for GouraudShader {
    type Interpolants = Rgb565;

    #[inline(always)]
    fn shade(&self, _x: i32, _y: i32, _z: ZDepth, color: Rgb565) -> Rgb565 {
        color
    }
}
