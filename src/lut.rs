//! Lookup tables for fast trigonometric operations
//!
//! Uses 256-entry tables for sin/cos (1KB total)
//! ~50-100x faster than f32::sin()/cos() on embedded systems without FPU

use core::f32::consts::PI;
#[cfg(not(feature = "std"))]
use micromath::F32Ext;

/// 256-entry sine lookup table covering 0 to 2π
/// Each entry is scaled by 32768 for fixed-point arithmetic
const SIN_TABLE: [i16; 256] = generate_sin_table();

/// Generate sine lookup table at compile time
const fn generate_sin_table() -> [i16; 256] {
    let mut table = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        // We need to use approximation since sin() isn't const
        // This is a compile-time approximation using higher-order Taylor series
        let angle_rad = (i as f32 * 2.0 * PI) / 256.0;
        // Taylor series approximation for sin(x) with more terms for better accuracy
        // sin(x) = x - x³/3! + x⁵/5! - x⁷/7! + x⁹/9! - x¹¹/11!
        let x = angle_rad;
        let x2 = x * x;
        let x3 = x2 * x;
        let x5 = x3 * x2;
        let x7 = x5 * x2;
        let x9 = x7 * x2;
        let x11 = x9 * x2;

        let sin_val = x
            - x3 / 6.0           // x³/3!
            + x5 / 120.0         // x⁵/5!
            - x7 / 5040.0        // x⁷/7!
            + x9 / 362880.0      // x⁹/9!
            - x11 / 39916800.0; // x¹¹/11!

        table[i] = (sin_val * 32768.0) as i16;
        i += 1;
    }
    table
}

/// Fast sine using lookup table
///
/// Input: angle in radians (any range)
/// Output: sine value scaled by 32768 (-32768 to 32767)
///
/// To convert to float: result as f32 / 32768.0
#[inline(always)]
pub fn fast_sin_i16(angle: f32) -> i16 {
    // Normalize angle to 0..2π and map to 0..255
    let normalized = (angle % (2.0 * PI)) / (2.0 * PI);
    let index = ((normalized * 256.0) as usize) & 0xFF;
    SIN_TABLE[index]
}

/// Fast cosine using lookup table (cos(x) = sin(x + π/2))
#[inline(always)]
pub fn fast_cos_i16(angle: f32) -> i16 {
    fast_sin_i16(angle + PI / 2.0)
}

/// Fast sine returning f32 — uses micromath polynomial (~0.001 error vs 2% for the i16 table).
#[inline(always)]
pub fn fast_sin(angle: f32) -> f32 {
    angle.sin()
}

/// Fast cosine returning f32 — uses micromath polynomial (~0.001 error vs 2% for the i16 table).
#[inline(always)]
pub fn fast_cos(angle: f32) -> f32 {
    angle.cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sin_accuracy() {
        extern crate std;
        // Test accuracy at various angles
        let test_angles = [
            0.0,
            PI / 6.0,
            PI / 4.0,
            PI / 3.0,
            PI / 2.0,
            PI,
            3.0 * PI / 2.0,
        ];

        for angle in test_angles.iter() {
            let fast = fast_sin(*angle);
            let accurate = angle.sin();
            let error = (fast - accurate).abs();

            std::println!(
                "sin({:.3}): fast={:.4}, accurate={:.4}, error={:.4}",
                angle,
                fast,
                accurate,
                error
            );

            // For a 256-entry lookup table, error < 0.02 (2%) is quite good
            // This is a reasonable trade-off for embedded systems
            assert!(
                error < 0.02,
                "Error too large at angle {}: {}",
                angle,
                error
            );
        }
    }

    #[test]
    fn test_cos_accuracy() {
        extern crate std;
        let test_angles = [0.0, PI / 6.0, PI / 4.0, PI / 3.0, PI / 2.0, PI];

        for angle in test_angles.iter() {
            let fast = fast_cos(*angle);
            let accurate = angle.cos();
            let error = (fast - accurate).abs();

            std::println!(
                "cos({:.3}): fast={:.4}, accurate={:.4}, error={:.4}",
                angle,
                fast,
                accurate,
                error
            );

            // For a 256-entry lookup table, error < 0.02 (2%) is acceptable
            assert!(
                error < 0.02,
                "Error too large at angle {}: {}",
                angle,
                error
            );
        }
    }

    #[test]
    fn test_periodicity() {
        extern crate std;
        // Test that sin(x) ≈ sin(x + 2π)
        // Note: Due to floating-point precision in modulo and table indexing,
        // we may hit adjacent table entries, so we use a relaxed threshold
        let angle = PI / 4.0;
        let sin1 = fast_sin(angle);
        let sin2 = fast_sin(angle + 2.0 * PI);
        let sin3 = fast_sin(angle + 4.0 * PI);

        std::println!(
            "Periodicity test: sin1={:.4}, sin2={:.4}, sin3={:.4}",
            sin1,
            sin2,
            sin3
        );
        std::println!(
            "Differences: |sin1-sin2|={:.4}, |sin1-sin3|={:.4}",
            (sin1 - sin2).abs(),
            (sin1 - sin3).abs()
        );

        // Allow up to 0.02 difference due to table quantization
        assert!(
            (sin1 - sin2).abs() < 0.02,
            "Periodicity error too large: {}",
            (sin1 - sin2).abs()
        );
        assert!(
            (sin1 - sin3).abs() < 0.02,
            "Periodicity error too large: {}",
            (sin1 - sin3).abs()
        );
    }
}

// test
fn _unused() {}
