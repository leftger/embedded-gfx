//! Doom-style sector light animation descriptors and evaluators.

use micromath::F32Ext;

/// Animated light waveform kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightEffectKind {
    Glow,
    Random,
    Alternate,
}

/// Compact per-sector light descriptor.
///
/// `base`/`alt` are 0..=255 brightness values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectorLight {
    pub base: u8,
    pub alt: u8,
    pub speed: f32,
    pub duration: f32,
    pub sync: f32,
    pub effect: Option<LightEffectKind>,
}

impl SectorLight {
    /// Constant full-bright light.
    pub const fn fullbright() -> Self {
        Self {
            base: 255,
            alt: 255,
            speed: 0.0,
            duration: 0.0,
            sync: 0.0,
            effect: None,
        }
    }
}

#[inline]
fn fract(x: f32) -> f32 {
    x - x.floor()
}

#[inline]
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[inline]
fn noise(sync: f32, time: f32) -> f32 {
    fract(1.0 + ((sync + time / 1000.0) * 12.9898 + sync * 78.233).sin() * 43758.547)
}

/// Evaluate a normalized brightness value in `[0.0, 1.0]`.
pub fn light_level_at(light: &SectorLight, time: f32) -> f32 {
    let base = light.base as f32 / 255.0;
    let alt = light.alt as f32 / 255.0;
    let effect = if let Some(e) = light.effect {
        e
    } else {
        return base;
    };
    match effect {
        LightEffectKind::Glow => {
            let scale = (base - alt).abs().max(1e-6);
            let phase = time * light.speed / scale;
            clamp01((0.5 - fract(phase)).abs() * 2.0 * scale + alt.min(base))
        }
        LightEffectKind::Random => {
            if noise(light.sync, (time * light.speed).floor()) < light.duration {
                alt
            } else {
                base
            }
        }
        LightEffectKind::Alternate => {
            if fract(time * light.speed + light.sync * 3.5435) < light.duration {
                alt
            } else {
                base
            }
        }
    }
}

/// Evaluate a byte brightness in `0..=255`.
pub fn light_level_u8_at(light: &SectorLight, time: f32) -> u8 {
    (light_level_at(light, time) * 255.0).clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_light_returns_base() {
        let light = SectorLight {
            base: 192,
            alt: 64,
            speed: 3.0,
            duration: 0.5,
            sync: 0.25,
            effect: None,
        };
        assert_eq!(light_level_u8_at(&light, 0.0), 192);
        assert_eq!(light_level_u8_at(&light, 8.0), 192);
    }

    #[test]
    fn random_light_is_deterministic_for_same_input() {
        let light = SectorLight {
            base: 200,
            alt: 80,
            speed: 20.0,
            duration: 0.3,
            sync: 0.11,
            effect: Some(LightEffectKind::Random),
        };
        let a = light_level_u8_at(&light, 1.25);
        let b = light_level_u8_at(&light, 1.25);
        assert_eq!(a, b);
    }

    #[test]
    fn glow_and_alternate_remain_in_range() {
        let glow = SectorLight {
            base: 255,
            alt: 48,
            speed: 0.5,
            duration: 0.0,
            sync: 0.0,
            effect: Some(LightEffectKind::Glow),
        };
        let alt = SectorLight {
            base: 255,
            alt: 96,
            speed: 2.0,
            duration: 0.5,
            sync: 0.1,
            effect: Some(LightEffectKind::Alternate),
        };
        for t in [0.0, 0.1, 0.5, 1.0, 2.0, 4.0] {
            let g = light_level_u8_at(&glow, t);
            let a = light_level_u8_at(&alt, t);
            assert!(g <= 255);
            assert!(a <= 255);
        }
    }
}
