//! High-performance 2.5D DDA Raycasting and Mode 7 True 3D Perspective Floorcasting Engine.
//!
//! Provides ultra-fast 60 FPS 2.5D environment rendering for microcontrollers and embedded displays,
//! including Mode 7 perspective floor/ceiling projection, DDA wall column raycasting, distance shading,
//! and billboard sprite depth sorting.

use embedded_graphics_core::pixelcolor::IntoStorage;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::RgbColor;
use micromath::F32Ext;

/// 16x16 Texture Sampler for 2.5D Walls, Floors, and Ceilings.
#[derive(Debug, Clone, Copy)]
pub struct RaycastTexture {
    pub pixels: [Rgb565; 256],
}

impl RaycastTexture {
    /// Creates a 16x16 texture from a flat array of 256 Rgb565 pixels.
    pub const fn new(pixels: [Rgb565; 256]) -> Self {
        Self { pixels }
    }

    /// Samples a pixel at (u, v) normalized coordinates [0..15].
    #[inline(always)]
    pub fn sample(&self, u: usize, v: usize) -> Rgb565 {
        self.pixels[(v & 15) * 16 + (u & 15)]
    }
}

/// Billboard sprite instance for 3D world placement.
#[derive(Debug, Clone, Copy)]
pub struct RaycastSprite {
    pub x: f32,
    pub y: f32,
    pub texture_id: u8,
    pub active: bool,
}

/// Mode 7 True 3D Perspective Floor and Ceiling Renderer.
#[derive(Debug, Clone)]
pub struct Mode7Renderer {
    width: usize,
    height: usize,
    fov_scale: f32,
}

impl Mode7Renderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            fov_scale: 0.66,
        }
    }

    /// Set field of view scale factor (default: 0.66 for ~66 deg FOV).
    pub fn set_fov_scale(&mut self, fov_scale: f32) {
        self.fov_scale = fov_scale;
    }

    /// Render perspective-correct floor and ceiling into a 32-bit packed Rgb565 buffer (`u32` pairs).
    pub fn render_floor_and_ceiling(
        &self,
        pos_x: f32,
        pos_y: f32,
        angle: f32,
        head_bob: i32,
        floor_color_a: Rgb565,
        floor_color_b: Rgb565,
        ceil_color_a: Rgb565,
        ceil_color_b: Rgb565,
        framebuf_u32: &mut [u32],
    ) {
        let dir_x = angle.cos();
        let dir_y = angle.sin();

        let plane_x = -dir_y * self.fov_scale;
        let plane_y = dir_x * self.fov_scale;

        // Frustum boundary ray vectors aligned with display orientation
        let ray_dir_x0 = dir_x + plane_x;
        let ray_dir_y0 = dir_y + plane_y;
        let ray_dir_x1 = dir_x - plane_x;
        let ray_dir_y1 = dir_y - plane_y;

        let center_y = (self.height / 2) as i32 + head_bob;
        let stride_u32 = self.width / 2;

        // 1. Mode 7 Floor Projection (horizon down to bottom)
        let floor_start = center_y.clamp(0, self.height as i32) as usize;
        for y in floor_start..self.height {
            let p = (y as i32 - center_y).max(1);
            let row_dist = (0.5 * self.height as f32) / (p as f32);

            let floor_step_x = row_dist * (ray_dir_x1 - ray_dir_x0) / (self.width as f32);
            let floor_step_y = row_dist * (ray_dir_y1 - ray_dir_y0) / (self.width as f32);

            let mut floor_x = pos_x + row_dist * ray_dir_x0;
            let mut floor_y = pos_y + row_dist * ray_dir_y0;

            let f_shade = (1.0 / (1.0 + row_dist * 0.18)).clamp(0.08, 0.85);
            let row_u32 = y * stride_u32;

            for x_u32 in 0..stride_u32 {
                let cell_x = floor_x as i32;
                let cell_y = floor_y as i32;

                let tx = ((floor_x - cell_x as f32) * 16.0) as usize & 15;
                let ty = ((floor_y - cell_y as f32) * 16.0) as usize & 15;

                let is_grout = (tx == 0) || (ty == 0);
                let f_base = if is_grout {
                    Rgb565::new(4, 3, 2)
                } else if (cell_x + cell_y) % 2 == 0 {
                    floor_color_a
                } else {
                    floor_color_b
                };

                let shaded = apply_shade(f_base, f_shade);
                framebuf_u32[row_u32 + x_u32] = pack_rgb565_u32(shaded);

                floor_x += floor_step_x * 2.0;
                floor_y += floor_step_y * 2.0;
            }
        }

        // 2. Mode 7 Ceiling Projection (top down to horizon)
        let ceil_end = center_y.clamp(0, self.height as i32) as usize;
        for y in 0..ceil_end {
            let p = (center_y - y as i32).max(1);
            let row_dist = (0.5 * self.height as f32) / (p as f32);

            let ceil_step_x = row_dist * (ray_dir_x1 - ray_dir_x0) / (self.width as f32);
            let ceil_step_y = row_dist * (ray_dir_y1 - ray_dir_y0) / (self.width as f32);

            let mut ceil_x = pos_x + row_dist * ray_dir_x0;
            let mut ceil_y = pos_y + row_dist * ray_dir_y0;

            let c_shade = (1.0 / (1.0 + row_dist * 0.22)).clamp(0.08, 0.7);
            let row_u32 = y * stride_u32;

            for x_u32 in 0..stride_u32 {
                let cell_x = ceil_x as i32;
                let cell_y = ceil_y as i32;

                let tx = ((ceil_x - cell_x as f32) * 16.0) as usize & 15;
                let ty = ((ceil_y - cell_y as f32) * 16.0) as usize & 15;

                let is_beam = (tx == 0) || (ty == 0);
                let c_base = if is_beam {
                    Rgb565::new(2, 4, 8)
                } else if (cell_x + cell_y) % 2 == 0 {
                    ceil_color_a
                } else {
                    ceil_color_b
                };

                let shaded = apply_shade(c_base, c_shade);
                framebuf_u32[row_u32 + x_u32] = pack_rgb565_u32(shaded);

                ceil_x += ceil_step_x * 2.0;
                ceil_y += ceil_step_y * 2.0;
            }
        }
    }

    /// Fast-path floor and ceiling renderer for flat/checkerboard patterns without per-pixel distance shading.
    pub fn render_floor_and_ceiling_fast(
        &self,
        pos_x: f32,
        pos_y: f32,
        angle: f32,
        head_bob: i32,
        floor_color_a: Rgb565,
        floor_color_b: Rgb565,
        ceil_color_a: Rgb565,
        ceil_color_b: Rgb565,
        framebuf_u32: &mut [u32],
    ) {
        let dir_x = angle.cos();
        let dir_y = angle.sin();
        let plane_x = -dir_y * self.fov_scale;
        let plane_y = dir_x * self.fov_scale;

        let horizon = (self.height / 2) as i32 + head_bob;

        let floor_a_u32 = pack_rgb565_u32(floor_color_a);
        let floor_b_u32 = pack_rgb565_u32(floor_color_b);
        let ceiling_a_u32 = pack_rgb565_u32(ceil_color_a);
        let ceiling_b_u32 = pack_rgb565_u32(ceil_color_b);

        let fov_inv = 2.0 / (self.width as f32);
        let stride_u32 = self.width / 2;

        for y in 0..self.height {
            let p = (y as i32 - horizon) as f32;
            if p == 0.0 {
                continue;
            }

            let is_floor = p > 0.0;
            let row_distance = if is_floor {
                (self.height as f32 * 0.625) / p
            } else {
                (self.height as f32 * 0.625) / -p
            };

            let step_x = -row_distance * (plane_x * fov_inv) * 2.0;
            let step_y = -row_distance * (plane_y * fov_inv) * 2.0;

            let mut curr_x = pos_x + row_distance * (dir_x + plane_x);
            let mut curr_y = pos_y + row_distance * (dir_y + plane_y);

            let row_u32 = y * stride_u32;
            let (col_a, col_b) = if is_floor {
                (floor_a_u32, floor_b_u32)
            } else {
                (ceiling_a_u32, ceiling_b_u32)
            };

            for x2 in 0..stride_u32 {
                let tx = (curr_x as usize) & 1;
                let ty = (curr_y as usize) & 1;
                let pixel_u32 = if (tx ^ ty) == 0 { col_a } else { col_b };

                if row_u32 + x2 < framebuf_u32.len() {
                    framebuf_u32[row_u32 + x2] = pixel_u32;
                }
                curr_x += step_x;
                curr_y += step_y;
            }
        }
    }
}

/// DDA Raycaster Engine for 2.5D Grid Maps.
#[derive(Debug, Clone)]
pub struct Raycaster2D {
    width: usize,
    height: usize,
    fov_scale: f32,
}

impl Raycaster2D {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            fov_scale: 0.66,
        }
    }

    /// Render 3D textured walls over an existing Mode 7 / background framebuffer.
    pub fn render_walls(
        &self,
        pos_x: f32,
        pos_y: f32,
        angle: f32,
        head_bob: i32,
        map: &[u8],
        map_size: usize,
        wall_colors: &[Rgb565],
        z_buffer: &mut [f32],
        framebuf_u32: &mut [u32],
    ) {
        let dir_x = angle.cos();
        let dir_y = angle.sin();

        let plane_x = -dir_y * self.fov_scale;
        let plane_y = dir_x * self.fov_scale;

        let stride_u32 = self.width / 2;

        for x in (0..self.width).step_by(4) {
            let camera_x = -(2.0 * (x as f32) / (self.width as f32) - 1.0);
            let ray_dir_x = dir_x + plane_x * camera_x;
            let ray_dir_y = dir_y + plane_y * camera_x;

            let mut map_x = pos_x as i32;
            let mut map_y = pos_y as i32;

            let delta_dist_x = if ray_dir_x == 0.0 {
                1e30
            } else {
                (1.0 / ray_dir_x).abs()
            };
            let delta_dist_y = if ray_dir_y == 0.0 {
                1e30
            } else {
                (1.0 / ray_dir_y).abs()
            };

            let (step_x, mut side_dist_x) = if ray_dir_x < 0.0 {
                (-1, (pos_x - map_x as f32) * delta_dist_x)
            } else {
                (1, (map_x as f32 + 1.0 - pos_x) * delta_dist_x)
            };

            let (step_y, mut side_dist_y) = if ray_dir_y < 0.0 {
                (-1, (pos_y - map_y as f32) * delta_dist_y)
            } else {
                (1, (map_y as f32 + 1.0 - pos_y) * delta_dist_y)
            };

            let mut hit_wall = 0u8;
            let mut side = 0u8;
            let mut steps = 0;

            while hit_wall == 0 && steps < 24 {
                if side_dist_x < side_dist_y {
                    side_dist_x += delta_dist_x;
                    map_x += step_x;
                    side = 0;
                } else {
                    side_dist_y += delta_dist_y;
                    map_y += step_y;
                    side = 1;
                }

                if map_x >= 0 && map_x < map_size as i32 && map_y >= 0 && map_y < map_size as i32 {
                    let tile = map[(map_y as usize) * map_size + (map_x as usize)];
                    if tile > 0 {
                        hit_wall = tile;
                    }
                } else {
                    hit_wall = 1;
                }
                steps += 1;
            }

            let perp_wall_dist = if side == 0 {
                side_dist_x - delta_dist_x
            } else {
                side_dist_y - delta_dist_y
            }
            .max(0.1);

            for i in 0..4 {
                if x + i < self.width {
                    z_buffer[x + i] = perp_wall_dist;
                }
            }

            let line_height = (self.height as f32 / perp_wall_dist) as i32;
            let center_y = (self.height / 2) as i32 + head_bob;

            let draw_start = (center_y - line_height / 2).clamp(0, self.height as i32 - 1) as usize;
            let draw_end = (center_y + line_height / 2).clamp(0, self.height as i32 - 1) as usize;

            let mut wall_x = if side == 0 {
                pos_y + perp_wall_dist * ray_dir_y
            } else {
                pos_x + perp_wall_dist * ray_dir_x
            };
            wall_x -= wall_x.floor();
            let tex_x = ((wall_x * 16.0) as usize).clamp(0, 15);

            let shade_factor = 1.0 / (1.0 + perp_wall_dist * 0.18);
            let color_idx = (hit_wall as usize).saturating_sub(1) % wall_colors.len().max(1);
            let base_color = if wall_colors.is_empty() {
                Rgb565::RED
            } else {
                wall_colors[color_idx]
            };
            let base_color = if side == 1 {
                apply_shade(base_color, 0.7)
            } else {
                base_color
            };

            let x_u32 = x / 2;
            let tex_step = 16.0 / (line_height as f32).max(1.0);
            let mut tex_pos = ((draw_start as i32 - center_y + line_height / 2) as f32) * tex_step;

            for y in draw_start..=draw_end {
                let tex_y = (tex_pos as usize) & 15;
                tex_pos += tex_step;

                let is_pattern = (tex_x == 0) || (tex_y == 0);
                let pixel = if is_pattern {
                    apply_shade(base_color, 0.5)
                } else {
                    base_color
                };
                let shaded = apply_shade(pixel, shade_factor);
                let pixel_u32 = pack_rgb565_u32(shaded);

                let idx = y * stride_u32 + x_u32;
                if idx < framebuf_u32.len() {
                    framebuf_u32[idx] = pixel_u32;
                    if idx + 1 < framebuf_u32.len() {
                        framebuf_u32[idx + 1] = pixel_u32;
                    }
                }
            }
        }
    }

    /// Render 3D textured walls using a **per-tile pixel callback** instead of a flat colour
    /// palette.
    ///
    /// The `get_pixel` closure receives `(tile_id, tex_x, tex_y)` and returns the raw
    /// `Rgb565` texel **before** distance shading is applied.  This lets callers supply
    /// hand-painted bitmaps, procedural patterns (brick mortar, tech panels, hazard
    /// stripes …) or atlas lookups without the overhead of a full texture object.
    ///
    /// All other parameters are identical to [`render_walls`].
    pub fn render_walls_textured<F>(
        &self,
        pos_x: f32,
        pos_y: f32,
        angle: f32,
        head_bob: i32,
        map: &[u8],
        map_size: usize,
        z_buffer: &mut [f32],
        framebuf_u32: &mut [u32],
        get_pixel: F,
    ) where
        F: Fn(u8, usize, usize) -> Rgb565,
    {
        let dir_x = angle.cos();
        let dir_y = angle.sin();

        let plane_x = -dir_y * self.fov_scale;
        let plane_y = dir_x * self.fov_scale;

        let stride_u32 = self.width / 2;

        for x in (0..self.width).step_by(4) {
            let camera_x = -(2.0 * (x as f32) / (self.width as f32) - 1.0);
            let ray_dir_x = dir_x + plane_x * camera_x;
            let ray_dir_y = dir_y + plane_y * camera_x;

            let mut map_x = pos_x as i32;
            let mut map_y = pos_y as i32;

            let delta_dist_x = if ray_dir_x == 0.0 {
                1e30
            } else {
                (1.0 / ray_dir_x).abs()
            };
            let delta_dist_y = if ray_dir_y == 0.0 {
                1e30
            } else {
                (1.0 / ray_dir_y).abs()
            };

            let (step_x, mut side_dist_x) = if ray_dir_x < 0.0 {
                (-1, (pos_x - map_x as f32) * delta_dist_x)
            } else {
                (1, (map_x as f32 + 1.0 - pos_x) * delta_dist_x)
            };

            let (step_y, mut side_dist_y) = if ray_dir_y < 0.0 {
                (-1, (pos_y - map_y as f32) * delta_dist_y)
            } else {
                (1, (map_y as f32 + 1.0 - pos_y) * delta_dist_y)
            };

            let mut hit_wall = 0u8;
            let mut side = 0u8;
            let mut steps = 0;

            while hit_wall == 0 && steps < 24 {
                if side_dist_x < side_dist_y {
                    side_dist_x += delta_dist_x;
                    map_x += step_x;
                    side = 0;
                } else {
                    side_dist_y += delta_dist_y;
                    map_y += step_y;
                    side = 1;
                }

                if map_x >= 0 && map_x < map_size as i32 && map_y >= 0 && map_y < map_size as i32 {
                    let tile = map[(map_y as usize) * map_size + (map_x as usize)];
                    if tile > 0 {
                        hit_wall = tile;
                    }
                } else {
                    hit_wall = 1;
                }
                steps += 1;
            }

            let perp_wall_dist = if side == 0 {
                side_dist_x - delta_dist_x
            } else {
                side_dist_y - delta_dist_y
            }
            .max(0.1);

            for i in 0..4 {
                if x + i < self.width {
                    z_buffer[x + i] = perp_wall_dist;
                }
            }

            let line_height = (self.height as f32 / perp_wall_dist) as i32;
            let center_y = (self.height / 2) as i32 + head_bob;

            let draw_start = (center_y - line_height / 2).clamp(0, self.height as i32 - 1) as usize;
            let draw_end = (center_y + line_height / 2).clamp(0, self.height as i32 - 1) as usize;

            // Fractional wall-hit position → texture column
            let mut wall_x = if side == 0 {
                pos_y + perp_wall_dist * ray_dir_y
            } else {
                pos_x + perp_wall_dist * ray_dir_x
            };
            wall_x -= wall_x.floor();
            let tex_x = ((wall_x * 16.0) as usize).clamp(0, 15);

            // Distance shading: darker further away; side faces at 70% brightness
            let base_shade = 1.0 / (1.0 + perp_wall_dist * 0.18);
            let shade = if side == 1 {
                base_shade * 0.7
            } else {
                base_shade
            };
            let shade_q8 = (shade.clamp(0.05, 1.0) * 256.0) as u32;

            let x_u32 = x / 2;
            let tex_step = 16.0 / (line_height as f32).max(1.0);
            let mut tex_pos = ((draw_start as i32 - center_y + line_height / 2) as f32) * tex_step;

            for y in draw_start..=draw_end {
                let tex_y = (tex_pos as usize) & 15;
                tex_pos += tex_step;

                let raw_color = get_pixel(hit_wall, tex_x, tex_y);
                let shaded = apply_shade_q8(raw_color, shade_q8);
                let pixel_u32 = pack_rgb565_u32(shaded);

                let idx = y * stride_u32 + x_u32;
                if idx < framebuf_u32.len() {
                    framebuf_u32[idx] = pixel_u32;
                    if idx + 1 < framebuf_u32.len() {
                        framebuf_u32[idx + 1] = pixel_u32;
                    }
                }
            }
        }
    }

    /// Fast-path sprite rendering using a **per-sprite setup callback** and per-pixel color getter.
    ///
    /// `prepare_sprite(sprite, transform_y)` is called **ONCE PER SPRITE** with the perpendicular
    /// distance `transform_y`. Return `None` to skip rendering the sprite, or `Some(data)` to pass
    /// pre-calculated properties (such as pre-shaded colors) to `get_pixel`.
    pub fn render_sprites_fast<P, F, T>(
        &self,
        pos_x: f32,
        pos_y: f32,
        angle: f32,
        head_bob: i32,
        sprites: &[RaycastSprite],
        z_buffer: &[f32],
        framebuf: &mut [Rgb565],
        prepare_sprite: P,
        get_pixel: F,
    ) where
        P: Fn(&RaycastSprite, f32) -> Option<T>,
        F: Fn(&T, usize, usize, usize, usize) -> Option<Rgb565>,
    {
        let dir_x = angle.cos();
        let dir_y = angle.sin();
        let plane_x = -dir_y * self.fov_scale;
        let plane_y = dir_x * self.fov_scale;
        let inv_det = 1.0 / (plane_x * dir_y - dir_x * plane_y);
        let center_y = (self.height / 2) as i32 + head_bob;

        for sprite in sprites {
            if !sprite.active {
                continue;
            }

            let sx = sprite.x - pos_x;
            let sy = sprite.y - pos_y;

            let transform_x = inv_det * (dir_y * sx - dir_x * sy);
            let transform_y = inv_det * (-plane_y * sx + plane_x * sy);

            // Only render sprites in front of the camera
            if transform_y <= 0.3 {
                continue;
            }

            let sprite_data = match prepare_sprite(sprite, transform_y) {
                Some(d) => d,
                None => continue,
            };

            let sprite_screen_x =
                ((self.width as f32 / 2.0) * (1.0 - transform_x / transform_y)) as i32;
            let sprite_height = ((self.height as f32 / transform_y).abs()) as i32;
            let sprite_width = sprite_height;

            let draw_start_y =
                (center_y - sprite_height / 2).clamp(0, self.height as i32 - 1) as usize;
            let draw_end_y =
                (center_y + sprite_height / 2).clamp(0, self.height as i32 - 1) as usize;

            let draw_start_x =
                (sprite_screen_x - sprite_width / 2).clamp(0, self.width as i32 - 1) as usize;
            let draw_end_x =
                (sprite_screen_x + sprite_width / 2).clamp(0, self.width as i32 - 1) as usize;

            for stripe_x in draw_start_x..draw_end_x {
                // Z-buffer occlusion: skip columns behind a closer wall
                if stripe_x >= z_buffer.len() || transform_y >= z_buffer[stripe_x] {
                    continue;
                }

                for y in draw_start_y..draw_end_y {
                    if let Some(color) =
                        get_pixel(&sprite_data, stripe_x, y, draw_start_y, draw_end_y)
                    {
                        let idx = y * self.width + stripe_x;
                        if idx < framebuf.len() {
                            framebuf[idx] = color;
                        }
                    }
                }
            }
        }
    }

    /// Render billboarded 2.5D sprites (enemies, items, projectiles) into a **per-pixel**
    /// `Rgb565` framebuffer with z-buffer occlusion against previously rendered walls.
    pub fn render_sprites<F>(
        &self,
        pos_x: f32,
        pos_y: f32,
        angle: f32,
        head_bob: i32,
        sprites: &[RaycastSprite],
        z_buffer: &[f32],
        framebuf: &mut [Rgb565],
        get_color: F,
    ) where
        F: Fn(&RaycastSprite, f32) -> Option<Rgb565>,
    {
        self.render_sprites_fast(
            pos_x,
            pos_y,
            angle,
            head_bob,
            sprites,
            z_buffer,
            framebuf,
            |sprite, _dist| Some(*sprite),
            |sprite, _stripe_x, y, draw_start_y, draw_end_y| {
                let norm_y = if draw_end_y > draw_start_y {
                    (y - draw_start_y) as f32 / (draw_end_y - draw_start_y) as f32
                } else {
                    0.0
                };
                get_color(sprite, norm_y)
            },
        );
    }
}

/// Apply a Q8 fixed-point integer distance-based shade factor `[0, 256]` to an `Rgb565` colour.
#[inline(always)]
pub fn apply_shade_q8(color: Rgb565, shade_q8: u32) -> Rgb565 {
    let raw = color.into_storage() as u32;
    let r = ((((raw >> 11) & 0x1F) * shade_q8) >> 8).min(31);
    let g = ((((raw >> 5) & 0x3F) * shade_q8) >> 8).min(63);
    let b = (((raw & 0x1F) * shade_q8) >> 8).min(31);
    Rgb565::new(r as u8, g as u8, b as u8)
}

/// Apply a distance-based shade factor `[0.0, 1.0]` to an `Rgb565` colour using fixed-point integer math.
#[inline(always)]
pub fn apply_shade(color: Rgb565, factor: f32) -> Rgb565 {
    let shade_q8 = (factor.clamp(0.05, 1.0) * 256.0) as u32;
    apply_shade_q8(color, shade_q8)
}

/// Pack two identical `Rgb565` pixels into one `u32` for 32-bit-wide framebuffer writes.
///
/// Matches the internal packing used by [`Raycaster2D`] and [`Mode7Renderer`] so
/// callers can fill adjacent pixel pairs in a single store operation.
#[inline(always)]
pub fn pack_rgb565_u32(color: Rgb565) -> u32 {
    let raw = color.into_storage() as u32;
    (raw << 16) | raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode7_renderer_execution() {
        let renderer = Mode7Renderer::new(240, 256);
        let mut framebuf_u32 = [0u32; (240 * 256) / 2];

        renderer.render_floor_and_ceiling(
            2.5,
            2.5,
            0.0,
            0,
            Rgb565::RED,
            Rgb565::GREEN,
            Rgb565::BLUE,
            Rgb565::YELLOW,
            &mut framebuf_u32,
        );

        let non_zero_pixels = framebuf_u32.iter().filter(|&&p| p != 0).count();
        assert!(
            non_zero_pixels > 0,
            "Mode7Renderer should render non-zero pixels"
        );
    }

    #[test]
    fn test_raycaster2d_execution() {
        let raycaster = Raycaster2D::new(240, 256);
        let map = [1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1];
        let wall_colors = [Rgb565::RED, Rgb565::GREEN];
        let mut z_buffer = [0.0f32; 240];
        let mut framebuf_u32 = [0u32; (240 * 256) / 2];

        raycaster.render_walls(
            1.5,
            1.5,
            0.0,
            0,
            &map,
            4,
            &wall_colors,
            &mut z_buffer,
            &mut framebuf_u32,
        );

        assert!(z_buffer[0] > 0.0, "Z-buffer should record wall distances");
    }

    #[test]
    fn test_render_walls_textured_writes_pixels_and_z_buffer() {
        let raycaster = Raycaster2D::new(240, 256);
        // 4×4 map with a wall ring around the inside
        let map = [1u8, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1];
        let mut z_buffer = [0.0f32; 240];
        let mut framebuf_u32 = [0u32; (240 * 256) / 2];

        raycaster.render_walls_textured(
            1.5,
            1.5,
            0.0,
            0,
            &map,
            4,
            &mut z_buffer,
            &mut framebuf_u32,
            |tile, _tex_x, _tex_y| {
                // Tile 1 → green; anything else → red
                if tile == 1 {
                    Rgb565::GREEN
                } else {
                    Rgb565::RED
                }
            },
        );

        assert!(
            z_buffer[0] > 0.0,
            "render_walls_textured must write the z-buffer"
        );
        let written = framebuf_u32.iter().any(|&p| p != 0);
        assert!(
            written,
            "render_walls_textured must write at least one pixel"
        );
    }

    #[test]
    fn test_render_sprites_draws_behind_z_buffer() {
        let raycaster = Raycaster2D::new(240, 256);

        // Place a sprite at (5, 1.5) while camera looks along +X from (1.5, 1.5).
        // Set z_buffer to a large value so the sprite is NOT occluded.
        let sprites = [RaycastSprite {
            x: 5.0,
            y: 1.5,
            texture_id: 0,
            active: true,
        }];
        let z_buffer = [100.0f32; 240];
        let mut framebuf = [Rgb565::BLACK; 240 * 256];

        raycaster.render_sprites(
            1.5,
            1.5,
            0.0, // facing +X
            0,
            &sprites,
            &z_buffer,
            &mut framebuf,
            |_sprite, _norm_y| Some(Rgb565::YELLOW),
        );

        // At least one pixel should have been coloured yellow
        let yellow_pixels = framebuf.iter().filter(|&&p| p == Rgb565::YELLOW).count();
        assert!(
            yellow_pixels > 0,
            "render_sprites should draw the sprite when unoccluded"
        );
    }

    #[test]
    fn test_render_sprites_occluded_by_z_buffer() {
        let raycaster = Raycaster2D::new(240, 256);

        // Same setup but z_buffer has tiny values → sprite is behind walls → nothing drawn
        let sprites = [RaycastSprite {
            x: 5.0,
            y: 1.5,
            texture_id: 0,
            active: true,
        }];
        let z_buffer = [0.1f32; 240]; // all columns show a very close wall
        let mut framebuf = [Rgb565::BLACK; 240 * 256];

        raycaster.render_sprites(
            1.5,
            1.5,
            0.0,
            0,
            &sprites,
            &z_buffer,
            &mut framebuf,
            |_sprite, _norm_y| Some(Rgb565::YELLOW),
        );

        let yellow_pixels = framebuf.iter().filter(|&&p| p == Rgb565::YELLOW).count();
        assert_eq!(
            yellow_pixels, 0,
            "Occluded sprite should not write any pixels"
        );
    }

    #[test]
    fn test_apply_shade_public() {
        // Full brightness (factor = 1.0) should be identity (within rounding)
        let c = Rgb565::new(20, 40, 15);
        let out = apply_shade(c, 1.0);
        assert_eq!(out.r(), 20);
        assert_eq!(out.g(), 40);
        assert_eq!(out.b(), 15);

        // Clamped at 0.05 — channels should still be non-zero for a bright input
        let dark = apply_shade(Rgb565::new(31, 63, 31), 0.0);
        assert!(dark.r() > 0);
    }

    #[test]
    fn test_pack_rgb565_u32_public() {
        let c = Rgb565::new(10, 20, 10);
        let raw = c.into_storage() as u32;
        let packed = pack_rgb565_u32(c);
        assert_eq!(packed, (raw << 16) | raw);
    }
}
