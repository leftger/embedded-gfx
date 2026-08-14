use embedded_3dgfx::ZDepth;
use embedded_3dgfx::shader::{DitherConfig, DitherShader, FogConfig, FogShader, FragmentShader};
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// A custom user-defined material shader that generates a procedural wave pattern.
#[derive(Debug, Clone, Copy)]
pub struct WaveMaterialShader {
    pub base_color: Rgb565,
    pub wave_time: u32,
}

impl FragmentShader for WaveMaterialShader {
    type Interpolants = ();

    fn shade(&self, x: i32, y: i32, _z: ZDepth, _interps: ()) -> Rgb565 {
        // Simple procedural wave noise based on (x, y, time)
        let wave = ((x + y + self.wave_time as i32) & 15) as u8;
        let r = (self.base_color.r().saturating_add(wave)).min(31);
        let g = (self.base_color.g().saturating_add(wave * 2)).min(63);
        let b = (self.base_color.b().saturating_add(wave)).min(31);
        Rgb565::new(r, g, b)
    }
}

fn main() {
    println!("=== Custom Material Fragment Pipeline Demo ===");

    // 1. Instanciate base material shader
    let wave_mat = WaveMaterialShader {
        base_color: Rgb565::new(10, 20, 10),
        wave_time: 42,
    };

    // 2. Compose with Fog decorator
    let fog = FogConfig::new(Rgb565::new(2, 2, 2), 10.0, 100.0);
    let fogged_mat = FogShader {
        inner: wave_mat,
        fog: &fog,
    };

    // 3. Compose with Dither decorator
    let dither = DitherConfig::new(128);
    let final_pipeline = DitherShader {
        inner: fogged_mat,
        dither: &dither,
    };

    // Evaluate pixel fragment at screen coordinates (100, 120) and depth 50.0
    let sample = final_pipeline.shade(100, 120, 50, ());
    println!(
        "Sampled fragment color at (100, 120): R={}, G={}, B={}",
        sample.r(),
        sample.g(),
        sample.b()
    );
}
