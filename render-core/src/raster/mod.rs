use runtime_core::api::ScreenPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    const fn scale(self, alpha: u8) -> Self {
        Self {
            r: ((self.r as u16 * alpha as u16) / 255) as u8,
            g: ((self.g as u16 * alpha as u16) / 255) as u8,
            b: ((self.b as u16 * alpha as u16) / 255) as u8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaMask {
    width: u32,
    height: u32,
    pixels: &'static [u8],
}

impl AlphaMask {
    pub const fn new(width: u32, height: u32, pixels: &'static [u8]) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    fn alpha_at(self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.pixels[(y as usize * self.width as usize) + x as usize]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels
            .resize((width as usize) * (height as usize) * 4, 0);
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    pub fn clear(&mut self, color: Color) {
        for rgba in self.pixels.chunks_exact_mut(4) {
            rgba[0] = color.r;
            rgba[1] = color.g;
            rgba[2] = color.b;
            rgba[3] = 255;
        }
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 {
            return;
        }
        let width = self.width as i32;
        let height = self.height as i32;
        if x >= width || y >= height {
            return;
        }
        let index = ((y as usize * self.width as usize) + x as usize) * 4;
        self.pixels[index] = self.pixels[index].max(color.r);
        self.pixels[index + 1] = self.pixels[index + 1].max(color.g);
        self.pixels[index + 2] = self.pixels[index + 2].max(color.b);
        self.pixels[index + 3] = 255;
    }

    pub fn draw_line(
        &mut self,
        from: ScreenPoint,
        to: ScreenPoint,
        color: Color,
        thickness_px: u8,
    ) {
        let mut x0 = from.x_px.round() as i32;
        let mut y0 = from.y_px.round() as i32;
        let x1 = to.x_px.round() as i32;
        let y1 = to.y_px.round() as i32;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            self.stamp_circle(x0, y0, thickness_px.max(1), color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled_error = error * 2;
            if doubled_error >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled_error <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    pub fn stamp_circle(&mut self, cx: i32, cy: i32, radius_px: u8, color: Color) {
        let radius = radius_px.max(1) as i32;
        let radius_sq = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if (dx * dx) + (dy * dy) <= radius_sq {
                    self.set_pixel(cx + dx, cy + dy, color);
                }
            }
        }
    }

    pub fn draw_mask(&mut self, center: ScreenPoint, mask: AlphaMask, color: Color) {
        self.draw_rotated_mask(center, mask, 0.0, color);
    }

    pub fn draw_rotated_mask(
        &mut self,
        center: ScreenPoint,
        mask: AlphaMask,
        angle_rad: f32,
        color: Color,
    ) {
        self.draw_rotated_mask_with_filter(center, mask, angle_rad, color, |_x, _y| true);
    }

    pub fn draw_rotated_mask_radial_progress(
        &mut self,
        center: ScreenPoint,
        mask: AlphaMask,
        angle_rad: f32,
        color: Color,
        progress: f32,
        start_angle_rad: f32,
    ) {
        let clamped_progress = progress.clamp(0.0, 1.0);
        if clamped_progress <= 0.0 {
            return;
        }
        if clamped_progress >= 1.0 {
            self.draw_rotated_mask(center, mask, angle_rad, color);
            return;
        }

        let sweep_rad = std::f32::consts::TAU * clamped_progress;
        self.draw_rotated_mask_with_filter(center, mask, angle_rad, color, |local_x, local_y| {
            if local_x == 0.0 && local_y == 0.0 {
                return true;
            }
            let mut delta = local_y.atan2(local_x) - start_angle_rad;
            while delta < 0.0 {
                delta += std::f32::consts::TAU;
            }
            while delta >= std::f32::consts::TAU {
                delta -= std::f32::consts::TAU;
            }
            delta <= sweep_rad
        });
    }

    fn draw_rotated_mask_with_filter(
        &mut self,
        center: ScreenPoint,
        mask: AlphaMask,
        angle_rad: f32,
        color: Color,
        mut include_pixel: impl FnMut(f32, f32) -> bool,
    ) {
        let half_width = mask.width() as f32 / 2.0;
        let half_height = mask.height() as f32 / 2.0;
        let sin_theta = angle_rad.sin();
        let cos_theta = angle_rad.cos();
        let min_x = (center.x_px - half_width - 1.0).floor() as i32;
        let max_x = (center.x_px + half_width + 1.0).ceil() as i32;
        let min_y = (center.y_px - half_height - 1.0).floor() as i32;
        let max_y = (center.y_px + half_height + 1.0).ceil() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let local_x = x as f32 + 0.5 - center.x_px;
                let local_y = y as f32 + 0.5 - center.y_px;
                if !include_pixel(local_x, local_y) {
                    continue;
                }
                let source_x = (local_x * cos_theta) + (local_y * sin_theta) + half_width;
                let source_y = (-local_x * sin_theta) + (local_y * cos_theta) + half_height;
                let alpha = mask.alpha_at(source_x.floor() as i32, source_y.floor() as i32);
                if alpha == 0 {
                    continue;
                }
                self.set_pixel(x, y, color.scale(alpha));
            }
        }
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new(0, 0)
    }
}
