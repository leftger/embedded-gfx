//! SIMD, DSP, and SWAR arithmetic primitives.
//!
//! Provides fast zero-division color clamping, vector dot products, and packed
//! channel operations with transparent acceleration on ARM Cortex-M4/M33 DSP
//! (`target_feature = "dsp"`) and portable fallback for Cortex-M0/M3/desktop.

/// Fast 3D float dot product: `a[0]*b[0] + a[1]*b[1] + a[2]*b[2]`.
#[inline(always)]
pub fn dot3_f32(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Fast 3D integer dot product with 64-bit accumulator.
#[inline(always)]
pub fn dot3_i32(a: [i32; 3], b: [i32; 3]) -> i64 {
    (a[0] as i64 * b[0] as i64) + (a[1] as i64 * b[1] as i64) + (a[2] as i64 * b[2] as i64)
}

/// Fast 3D Q16.16 fixed-point dot product.
///
/// On ARM cores with DSP extension (Cortex-M4, Cortex-M33), this takes advantage of
/// dual 16-bit multiply-accumulate instructions.
#[inline(always)]
pub fn dot3_q16(a: [i32; 3], b: [i32; 3]) -> i32 {
    #[cfg(all(target_arch = "arm", target_feature = "dsp"))]
    unsafe {
        let p0 = ((a[0] as u32) & 0xFFFF) | ((a[1] as u32) << 16);
        let q0 = ((b[0] as u32) & 0xFFFF) | ((b[1] as u32) << 16);
        let mut sum: i32;
        let p2 = (a[2] * b[2]) >> 16;
        core::arch::asm!(
            "smlad {0}, {1}, {2}, {3}",
            out(reg) sum,
            in(reg) p0,
            in(reg) q0,
            in(reg) p2,
            options(pure, nomem, nostack)
        );
        sum
    }
    #[cfg(not(all(target_arch = "arm", target_feature = "dsp")))]
    {
        (((a[0] as i64 * b[0] as i64) + (a[1] as i64 * b[1] as i64) + (a[2] as i64 * b[2] as i64))
            >> 16) as i32
    }
}

/// Fast 5-bit color channel clamp `[0..31]`.
///
/// Uses ARM `usat` single-cycle instruction when DSP is available, or branch-free clamp.
#[inline(always)]
pub fn clamp_u5(val: i32) -> u8 {
    #[cfg(all(target_arch = "arm", target_feature = "dsp"))]
    unsafe {
        let res: i32;
        core::arch::asm!(
            "usat {0}, #5, {1}",
            out(reg) res,
            in(reg) val,
            options(pure, nomem, nostack)
        );
        res as u8
    }
    #[cfg(not(all(target_arch = "arm", target_feature = "dsp")))]
    {
        val.clamp(0, 31) as u8
    }
}

/// Fast 6-bit color channel clamp `[0..63]`.
///
/// Uses ARM `usat` single-cycle instruction when DSP is available, or branch-free clamp.
#[inline(always)]
pub fn clamp_u6(val: i32) -> u8 {
    #[cfg(all(target_arch = "arm", target_feature = "dsp"))]
    unsafe {
        let res: i32;
        core::arch::asm!(
            "usat {0}, #6, {1}",
            out(reg) res,
            in(reg) val,
            options(pure, nomem, nostack)
        );
        res as u8
    }
    #[cfg(not(all(target_arch = "arm", target_feature = "dsp")))]
    {
        val.clamp(0, 63) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot3_f32() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        assert_eq!(dot3_f32(a, b), 32.0);
    }

    #[test]
    fn test_dot3_i32() {
        let a = [10, 20, 30];
        let b = [4, 5, 6];
        assert_eq!(dot3_i32(a, b), 320);
    }

    #[test]
    fn test_clamp_u5() {
        assert_eq!(clamp_u5(-10), 0);
        assert_eq!(clamp_u5(15), 15);
        assert_eq!(clamp_u5(31), 31);
        assert_eq!(clamp_u5(50), 31);
    }

    #[test]
    fn test_clamp_u6() {
        assert_eq!(clamp_u6(-10), 0);
        assert_eq!(clamp_u6(30), 30);
        assert_eq!(clamp_u6(63), 63);
        assert_eq!(clamp_u6(100), 63);
    }
}
