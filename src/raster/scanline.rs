use embedded_graphics_core::prelude::Point;

// Row width configuration - features are prioritized if multiple are enabled
#[cfg(feature = "row_width_320")]
pub const MAX_ROW_WIDTH: usize = 320;
#[cfg(all(feature = "row_width_240", not(feature = "row_width_320")))]
pub const MAX_ROW_WIDTH: usize = 240;
#[cfg(all(
    feature = "row_width_160",
    not(feature = "row_width_240"),
    not(feature = "row_width_320"),
    not(feature = "row_width_96")
))]
pub const MAX_ROW_WIDTH: usize = 160;
#[cfg(all(
    feature = "row_width_96",
    not(feature = "row_width_160"),
    not(feature = "row_width_240"),
    not(feature = "row_width_320")
))]
pub const MAX_ROW_WIDTH: usize = 96;
#[cfg(not(any(
    feature = "row_width_320",
    feature = "row_width_240",
    feature = "row_width_160",
    feature = "row_width_96"
)))]
pub const MAX_ROW_WIDTH: usize = 100;

// Fixed-point 16.16 edge stepper — integer-only replacement for f32 invslope.
pub const FP_SHIFT: i64 = 16;

#[inline(always)]
pub fn fixed_to_i32(value: i64) -> i32 {
    if value >= 0 {
        (value >> FP_SHIFT) as i32
    } else {
        -((-value) >> FP_SHIFT) as i32
    }
}

pub struct EdgeStepper {
    pub x: i64,
    pub step: i64,
}

impl EdgeStepper {
    pub fn new(start: Point, end: Point, y: i32) -> Self {
        let dy = (end.y - start.y) as i64;
        let (step, x) = if dy != 0 {
            let s = (((end.x - start.x) as i64) << FP_SHIFT) / dy;
            let x = ((start.x as i64) << FP_SHIFT) + s * (y - start.y) as i64;
            (s, x)
        } else {
            (0, (start.x as i64) << FP_SHIFT)
        };
        Self { x, step }
    }

    #[inline(always)]
    pub fn current_x(&self) -> i32 {
        fixed_to_i32(self.x)
    }

    #[inline(always)]
    pub fn advance(&mut self) {
        self.x += self.step;
    }
}
