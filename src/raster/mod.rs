pub mod aa;
pub mod line;
pub mod scanline;
pub mod triangle;

#[cfg(feature = "aa")]
pub use aa::ReadPixel;
pub use line::Bresenham;
#[cfg(feature = "aa")]
pub use line::draw_line_aa;
pub use scanline::{EdgeStepper, FP_SHIFT, MAX_ROW_WIDTH, fixed_to_i32};
pub use triangle::{draw_triangle_zbuffered, tri_area2};
