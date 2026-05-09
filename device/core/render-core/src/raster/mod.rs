#[allow(unused_imports)]
use alloc::{vec, vec::Vec, string::{String, ToString}, boxed::Box, format, borrow::ToOwned};
#[allow(unused_imports)]
use num_traits::Float as _;

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

/// Pixel format abstraction. Each `Framebuffer<P>` is parameterized over one
/// concrete `Pixel` impl, so the rasterizer's hot path (set_pixel,
/// set_pixel_overwrite, clear) compiles down to format-specific code with
/// no runtime branches.
///
/// Two impls ship today:
/// * [`RgbaPixel`] — 8/8/8/8, the historical default. Used by host parity
///   tests, the wasm emulator, and any downstream consumer that wants
///   byte-exact reproducibility.
/// * [`Rgb565Pixel`] — 5/6/5 packed little-endian. Used on the
///   ESP32-P4 firmware so the rasterizer writes panel-format bytes
///   directly, eliminating a per-frame RGBA→RGB565 conversion pass.
pub trait Pixel: Copy + Default + core::fmt::Debug + PartialEq + Eq {
    /// Bytes consumed in the framebuffer per pixel.
    const BYTES_PER_PIXEL: usize;

    /// Lower a 24-bit `Color` into this pixel format.
    fn from_color(color: Color) -> Self;

    /// Channel-wise max-blend, matching the historical `set_pixel`
    /// semantic — the rasterizer treats brighter samples as winners
    /// without alpha compositing. For RGB565 the max is performed in
    /// 5/6/5 space; the small precision delta (±1 LSB per channel
    /// vs RGBA8) is intentional and within panel gamma noise.
    fn blend_max(self, other: Self) -> Self;

    /// Write `BYTES_PER_PIXEL` bytes into `dst[..BYTES_PER_PIXEL]`.
    fn write_to(self, dst: &mut [u8]);

    /// Read `BYTES_PER_PIXEL` bytes from `src[..BYTES_PER_PIXEL]`.
    fn read_from(src: &[u8]) -> Self;

    /// Fast-fill a whole framebuffer with `self`. Default impl loops
    /// per-pixel; format-specific impls override with vectorizable
    /// stride fills (e.g. `align_to_mut::<u32>()` for RgbaPixel).
    fn fill_buffer(self, buf: &mut [u8]) {
        for chunk in buf.chunks_exact_mut(Self::BYTES_PER_PIXEL) {
            self.write_to(chunk);
        }
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct RgbaPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel for RgbaPixel {
    const BYTES_PER_PIXEL: usize = 4;

    fn from_color(color: Color) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: 255,
        }
    }

    fn blend_max(self, other: Self) -> Self {
        Self {
            r: self.r.max(other.r),
            g: self.g.max(other.g),
            b: self.b.max(other.b),
            a: 255,
        }
    }

    fn write_to(self, dst: &mut [u8]) {
        dst[0] = self.r;
        dst[1] = self.g;
        dst[2] = self.b;
        dst[3] = self.a;
    }

    fn read_from(src: &[u8]) -> Self {
        Self {
            r: src[0],
            g: src[1],
            b: src[2],
            a: src[3],
        }
    }

    fn fill_buffer(self, buf: &mut [u8]) {
        // Pack the 4-byte pixel into a single u32 and stride-fill via
        // `align_to_mut::<u32>` so the inner store is one word per
        // pixel. Preserves the perf trick from the pre-generic code
        // path on the 800x800 RGBA framebuffer used by host fixtures
        // and the wasm emulator.
        let packed = u32::from_le_bytes([self.r, self.g, self.b, self.a]);
        let packed_bytes = packed.to_le_bytes();
        // SAFETY: align_to_mut returns prefix/body/suffix without
        // overlap; head/tail are byte-fallback paths and the body is
        // u32-aligned.
        let (head, body, tail) = unsafe { buf.align_to_mut::<u32>() };
        for (i, byte) in head.iter_mut().enumerate() {
            *byte = packed_bytes[i & 3];
        }
        body.fill(packed);
        for (i, byte) in tail.iter_mut().enumerate() {
            *byte = packed_bytes[i & 3];
        }
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct Rgb565Pixel(pub u16);

impl Pixel for Rgb565Pixel {
    const BYTES_PER_PIXEL: usize = 2;

    fn from_color(color: Color) -> Self {
        let r = u16::from(color.r >> 3);
        let g = u16::from(color.g >> 2);
        let b = u16::from(color.b >> 3);
        Self((r << 11) | (g << 5) | b)
    }

    fn blend_max(self, other: Self) -> Self {
        let r1 = (self.0 >> 11) & 0x1F;
        let g1 = (self.0 >> 5) & 0x3F;
        let b1 = self.0 & 0x1F;
        let r2 = (other.0 >> 11) & 0x1F;
        let g2 = (other.0 >> 5) & 0x3F;
        let b2 = other.0 & 0x1F;
        Self((r1.max(r2) << 11) | (g1.max(g2) << 5) | b1.max(b2))
    }

    fn write_to(self, dst: &mut [u8]) {
        let bytes = self.0.to_le_bytes();
        dst[0] = bytes[0];
        dst[1] = bytes[1];
    }

    fn read_from(src: &[u8]) -> Self {
        Self(u16::from_le_bytes([src[0], src[1]]))
    }

    fn fill_buffer(self, buf: &mut [u8]) {
        // Stride-fill via `align_to_mut::<u16>` so the inner store is
        // one halfword per pixel — same perf trick as RgbaPixel's u32
        // fill, sized for the 2-bytes-per-pixel format.
        let packed = self.0;
        let packed_bytes = packed.to_le_bytes();
        let (head, body, tail) = unsafe { buf.align_to_mut::<u16>() };
        for (i, byte) in head.iter_mut().enumerate() {
            *byte = packed_bytes[i & 1];
        }
        body.fill(packed);
        for (i, byte) in tail.iter_mut().enumerate() {
            *byte = packed_bytes[i & 1];
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
pub struct Framebuffer<P: Pixel = RgbaPixel> {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    _marker: core::marker::PhantomData<P>,
}

impl<P: Pixel> Framebuffer<P> {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * P::BYTES_PER_PIXEL],
            _marker: core::marker::PhantomData,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels
            .resize((width as usize) * (height as usize) * P::BYTES_PER_PIXEL, 0);
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

    /// Bytes per pixel for this framebuffer's format. Useful for callers
    /// that need to size adjacent buffers (panel side-uploads, parity
    /// snapshots) without hardcoding 4.
    pub const fn bytes_per_pixel(&self) -> usize {
        P::BYTES_PER_PIXEL
    }

    fn pixel_offset(&self, x: i32, y: i32) -> usize {
        ((y as usize * self.width as usize) + x as usize) * P::BYTES_PER_PIXEL
    }

    pub fn clear(&mut self, color: Color) {
        P::from_color(color).fill_buffer(&mut self.pixels);
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
        let index = self.pixel_offset(x, y);
        let bytes = &mut self.pixels[index..index + P::BYTES_PER_PIXEL];
        let blended = P::read_from(bytes).blend_max(P::from_color(color));
        blended.write_to(bytes);
    }

    pub fn set_pixel_overwrite(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 {
            return;
        }
        let width = self.width as i32;
        let height = self.height as i32;
        if x >= width || y >= height {
            return;
        }
        let index = self.pixel_offset(x, y);
        P::from_color(color).write_to(&mut self.pixels[index..index + P::BYTES_PER_PIXEL]);
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

    pub fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: Color) {
        if width == 0 || height == 0 {
            return;
        }
        for y_offset in 0..height as i32 {
            for x_offset in 0..width as i32 {
                self.set_pixel(x + x_offset, y + y_offset, color);
            }
        }
    }

    pub fn fill_rect_overwrite(&mut self, x: i32, y: i32, width: u32, height: u32, color: Color) {
        if width == 0 || height == 0 {
            return;
        }
        for y_offset in 0..height as i32 {
            for x_offset in 0..width as i32 {
                self.set_pixel_overwrite(x + x_offset, y + y_offset, color);
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

        let sweep_rad = core::f32::consts::TAU * clamped_progress;
        self.draw_rotated_mask_with_filter(center, mask, angle_rad, color, |local_x, local_y| {
            if local_x == 0.0 && local_y == 0.0 {
                return true;
            }
            let mut delta = local_y.atan2(local_x) - start_angle_rad;
            while delta < 0.0 {
                delta += core::f32::consts::TAU;
            }
            while delta >= core::f32::consts::TAU {
                delta -= core::f32::consts::TAU;
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

impl<P: Pixel> Default for Framebuffer<P> {
    fn default() -> Self {
        Self::new(0, 0)
    }
}
