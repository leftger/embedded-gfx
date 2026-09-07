use crate::ZDepth;
use crate::draw::effects::ScreenDoorConfig;
use crate::shader::FragmentShader;
use embedded_graphics_core::pixelcolor::Rgb565;

/// Decorator fragment shader that applies screen-door (Bayer dithered stipple) transparency.
///
/// When a fragment fails the Bayer threshold test, this returns `discard_color`
/// or retains zero modification.
#[derive(Debug, Clone, Copy)]
pub struct ScreenDoorShader<S> {
    pub inner: S,
    pub config: ScreenDoorConfig,
}

impl<S> ScreenDoorShader<S> {
    pub const fn new(inner: S, config: ScreenDoorConfig) -> Self {
        Self { inner, config }
    }
}

impl<S: FragmentShader> FragmentShader for ScreenDoorShader<S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: ZDepth, interps: Self::Interpolants) -> Rgb565 {
        if self.config.test(x, y) {
            self.inner.shade(x, y, z, interps)
        } else {
            // In fixed-function pipeline without discard token, returns black or passes through
            Rgb565::new(0, 0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::FlatColorShader;
    use embedded_graphics_core::pixelcolor::RgbColor;

    #[test]
    fn test_screen_door_shader() {
        let base = FlatColorShader { color: Rgb565::RED };
        let shader = ScreenDoorShader::new(base, ScreenDoorConfig::new(255));
        assert_eq!(shader.shade(0, 0, 100, ()), Rgb565::RED);

        let shader_zero = ScreenDoorShader::new(base, ScreenDoorConfig::new(0));
        assert_eq!(shader_zero.shade(0, 0, 100, ()), Rgb565::BLACK);
    }
}
