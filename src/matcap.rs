//! Procedural MatCap (Spherical Environment Map) texture generator for embedded systems.
//!
//! Generates 32x32 or 64x64 static/const RGB565 sphere textures in RAM/Flash without
//! requiring external PNG/BMP image files.

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// Procedural MatCap texture presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatCapPreset {
    /// High contrast white-to-dark specular metallic reflection
    Chrome,
    /// Warm golden yellow/orange metallic shine
    Gold,
    /// Soft white/iridescent pastels
    Pearl,
    /// Matte warm grey/clay sculpture shading
    Clay,
    /// Synthwave sci-fi green metallic gloss
    RetroEmerald,
}

impl MatCapPreset {
    /// Generates a 32x32 RGB565 MatCap texture array.
    pub fn generate_32x32(&self) -> [Rgb565; 1024] {
        let mut pixels = [Rgb565::BLACK; 1024];
        for y in 0..32 {
            let ny = (y as f32 / 31.0) * 2.0 - 1.0;
            for x in 0..32 {
                let nx = (x as f32 / 31.0) * 2.0 - 1.0;
                pixels[y * 32 + x] = self.shade_pixel(nx, ny);
            }
        }
        pixels
    }

    /// Generates a 64x64 RGB565 MatCap texture array.
    pub fn generate_64x64(&self) -> [Rgb565; 4096] {
        let mut pixels = [Rgb565::BLACK; 4096];
        for y in 0..64 {
            let ny = (y as f32 / 63.0) * 2.0 - 1.0;
            for x in 0..64 {
                let nx = (x as f32 / 63.0) * 2.0 - 1.0;
                pixels[y * 64 + x] = self.shade_pixel(nx, ny);
            }
        }
        pixels
    }

    fn shade_pixel(&self, nx: f32, ny: f32) -> Rgb565 {
        let r_sq = nx * nx + ny * ny;
        if r_sq > 1.0 {
            return Rgb565::BLACK;
        }
        let nz = (1.0 - r_sq).sqrt();

        let (r, g, b) = match self {
            MatCapPreset::Chrome => {
                // High contrast white-to-dark specular metallic reflection + horizon ring
                let light_dir = (0.50257, 0.7036, 0.50257);
                let dot = (nx * light_dir.0 + ny * light_dir.1 + nz * light_dir.2).max(0.0);
                let dot2 = dot * dot;
                let dot4 = dot2 * dot2;
                let spec = dot4 * dot4; // dot^8
                let ny_abs = if ny < 0.0 { -ny } else { ny };
                let h_term = (1.0 - (ny_abs * 2.0)).max(0.0);
                let horizon = h_term * h_term * h_term * 0.4;
                let v = (spec + horizon + nz * 0.15).min(1.0);
                (v, v, v)
            }
            MatCapPreset::Gold => {
                // Warm golden yellow/orange metallic shine
                let light_dir = (0.485, 0.679, 0.551);
                let dot = (nx * light_dir.0 + ny * light_dir.1 + nz * light_dir.2).max(0.0);
                let dot2 = dot * dot;
                let dot3 = dot2 * dot;
                let spec = dot3 * dot3; // dot^6
                let r = (nz * 0.7 + 0.3 + spec).min(1.0);
                let g = (nz * 0.525 + 0.15 + spec * 0.8).min(1.0);
                let b = (nz * 0.1 + spec * 0.2).min(1.0);
                (r, g, b)
            }
            MatCapPreset::Pearl => {
                // Soft white/iridescent pastels
                let light_dir = (0.3, 0.5, 0.818);
                let dot = (nx * light_dir.0 + ny * light_dir.1 + nz * light_dir.2).max(0.0);
                let dot2 = dot * dot;
                let dot4 = dot2 * dot2;
                let dot6 = dot4 * dot2;
                let spec = dot6 * dot6; // dot^12
                let diff = nz * 0.5 + 0.5;
                let r = (diff * 0.85 + nx * 0.05 + spec * 0.5).clamp(0.0, 1.0);
                let g = (diff * 0.85 + ny * 0.05 + spec * 0.5).clamp(0.0, 1.0);
                let b = (diff * 0.9 + (1.0 - nz) * 0.1 + spec * 0.5).clamp(0.0, 1.0);
                (r, g, b)
            }
            MatCapPreset::Clay => {
                // Matte warm grey/clay sculpture shading (nz * 0.7 + 0.3)
                let diff = nz * 0.7 + 0.3;
                let r = (diff * 0.85).min(1.0);
                let g = (diff * 0.70).min(1.0);
                let b = (diff * 0.60).min(1.0);
                (r, g, b)
            }
            MatCapPreset::RetroEmerald => {
                // Synthwave sci-fi green metallic gloss
                let light_dir = (0.4, 0.6, 0.6928);
                let dot = (nx * light_dir.0 + ny * light_dir.1 + nz * light_dir.2).max(0.0);
                let dot2 = dot * dot;
                let dot5 = dot2 * dot2 * dot;
                let spec = dot5 * dot5; // dot^10
                let r = (nz * 0.05 + spec * 0.4).min(1.0);
                let g = (nz * 0.85 + 0.15 + spec * 0.9).min(1.0);
                let b = (nz * 0.4 + spec * 0.6).min(1.0);
                (r, g, b)
            }
        };

        let r5 = (r * 31.0) as u8;
        let g6 = (g * 63.0) as u8;
        let b5 = (b * 31.0) as u8;
        Rgb565::new(r5, g6, b5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcap_chrome_generation() {
        let tex = MatCapPreset::Chrome.generate_32x32();
        assert_eq!(tex.len(), 1024);
        let center_pixel = tex[16 * 32 + 16];
        assert_ne!(center_pixel, Rgb565::BLACK);
    }

    #[test]
    fn test_matcap_gold_generation() {
        let tex = MatCapPreset::Gold.generate_32x32();
        assert_eq!(tex.len(), 1024);
        let center_pixel = tex[16 * 32 + 16];
        assert!(center_pixel.r() > 15, "Red channel should be strong");
        assert!(center_pixel.g() > 30, "Green channel should be strong");
    }
}
