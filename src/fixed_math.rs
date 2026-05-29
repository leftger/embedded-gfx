// Q16.16 fixed-point arithmetic using i32 storage.
// Multiply uses an i64 intermediate then narrows back to i32 — one instruction
// on Cortex-M4/M7 (SMULL + shift), cheaper than full i64×i64 on M3.
pub const FP_SHIFT: u32 = 16;
pub const FP_ONE: i32 = 1_i32 << FP_SHIFT;

#[inline]
pub fn to_fp(v: f32) -> i32 {
    (v * FP_ONE as f32) as i32
}

#[inline]
pub fn from_fp(v: i32) -> f32 {
    v as f32 / FP_ONE as f32
}

#[inline]
pub fn mul_fp(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> FP_SHIFT) as i32
}

#[inline]
pub fn div_fp(a: i32, b: i32) -> Option<i32> {
    if b == 0 {
        return None;
    }
    Some((((a as i64) << FP_SHIFT) / b as i64) as i32)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn fixed_roundtrip_is_close() {
        let values = [0.0f32, 0.25, -0.5, 1.0, 3.14159];
        for v in values {
            let fp = to_fp(v);
            let out = from_fp(fp);
            assert!((out - v).abs() <= 1.0 / FP_ONE as f32);
        }
    }

    #[test]
    fn fixed_mul_div_behaves_as_expected() {
        let a = to_fp(1.5);
        let b = to_fp(2.0);
        let prod = from_fp(mul_fp(a, b));
        assert!((prod - 3.0).abs() < 0.001);

        let quot = from_fp(div_fp(to_fp(3.0), to_fp(2.0)).unwrap());
        assert!((quot - 1.5).abs() < 0.001);
    }
}
