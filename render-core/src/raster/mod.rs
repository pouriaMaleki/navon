use runtime_core::api::ScreenPoint;

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
            pixels: vec![0; (width as usize) * (height as usize)],
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels.resize((width as usize) * (height as usize), 0);
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

    pub fn clear(&mut self, value: u8) {
        self.pixels.fill(value);
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, value: u8) {
        if x < 0 || y < 0 {
            return;
        }
        let width = self.width as i32;
        let height = self.height as i32;
        if x >= width || y >= height {
            return;
        }
        let index = (y as usize * self.width as usize) + x as usize;
        self.pixels[index] = self.pixels[index].max(value);
    }

    pub fn draw_line(&mut self, from: ScreenPoint, to: ScreenPoint, value: u8, thickness_px: u8) {
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
            self.stamp_circle(x0, y0, thickness_px.max(1), value);
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

    pub fn stamp_circle(&mut self, cx: i32, cy: i32, radius_px: u8, value: u8) {
        let radius = radius_px.max(1) as i32;
        let radius_sq = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if (dx * dx) + (dy * dy) <= radius_sq {
                    self.set_pixel(cx + dx, cy + dy, value);
                }
            }
        }
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new(0, 0)
    }
}
