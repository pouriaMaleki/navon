use render_core::raster::Framebuffer as RenderFramebuffer;

use crate::board_config::PanelPixelFormat;

#[derive(Debug, Clone, PartialEq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    // RGBA8888 cache. Host-only: used by the pixel-perfect parity fixtures
    // that compare firmware and wasm renders byte-by-byte. On-device we
    // skip this 2.56 MB copy entirely — every byte would go through PSRAM
    // twice (write then read-for-convert) and the cache has no consumer
    // on the device code path.
    #[cfg(not(target_os = "espidf"))]
    pixels: Vec<u8>,
    panel_pixels: Vec<u8>,
    panel_format: PanelPixelFormat,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32, panel_format: PanelPixelFormat) -> Self {
        let pixel_count = width as usize * height as usize;
        let bytes_per_pixel = match panel_format {
            PanelPixelFormat::Rgb565Le => 2,
        };
        Self {
            width,
            height,
            #[cfg(not(target_os = "espidf"))]
            pixels: vec![0; pixel_count * 4],
            panel_pixels: vec![0; pixel_count * bytes_per_pixel],
            panel_format,
        }
    }

    pub fn present_from_render(&mut self, framebuffer: &RenderFramebuffer) {
        let new_pixel_count =
            framebuffer.width() as usize * framebuffer.height() as usize;
        if self.width != framebuffer.width() || self.height != framebuffer.height() {
            let bytes_per_pixel = match self.panel_format {
                PanelPixelFormat::Rgb565Le => 2,
            };
            self.panel_pixels
                .resize(new_pixel_count * bytes_per_pixel, 0);
            #[cfg(not(target_os = "espidf"))]
            {
                self.pixels.resize(new_pixel_count * 4, 0);
            }
            self.width = framebuffer.width();
            self.height = framebuffer.height();
        }
        #[cfg(not(target_os = "espidf"))]
        {
            self.pixels.copy_from_slice(framebuffer.pixels());
        }
        match self.panel_format {
            PanelPixelFormat::Rgb565Le => {
                rgba_to_rgb565_le_into(framebuffer.pixels(), &mut self.panel_pixels);
            }
        }
    }

    #[cfg(not(target_os = "espidf"))]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn panel_pixels(&self) -> &[u8] {
        &self.panel_pixels
    }

    pub fn panel_format(&self) -> PanelPixelFormat {
        self.panel_format
    }

    /// Scans the panel buffer for non-zero pixels. Used only in tests /
    /// diagnostics; the heartbeat in `run_device_main` does not call this
    /// because it is O(pixel_count).
    pub fn lit_pixel_count(&self) -> usize {
        let stride = match self.panel_format {
            PanelPixelFormat::Rgb565Le => 2,
        };
        self.panel_pixels
            .chunks_exact(stride)
            .filter(|chunk| chunk.iter().any(|value| *value > 0))
            .count()
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new(0, 0, PanelPixelFormat::Rgb565Le)
    }
}

pub fn rgba_to_rgb565_le(pixels: &[u8]) -> Vec<u8> {
    let mut converted = vec![0u8; (pixels.len() / 4) * 2];
    rgba_to_rgb565_le_into(pixels, &mut converted);
    converted
}

/// Streaming RGBA→RGB565 conversion straight into a preallocated sink.
/// Avoids the per-frame 2.56 MB intermediate clone + Vec grow of the old
/// `rgba_to_rgb565_le` + `extend_from_slice` path. The inner loop uses the
/// fast `>> 3` / `>> 2` shifts instead of `* 31 / 255` — the divide version
/// rounds half-to-even in LLVM's codegen but the shift is bit-equivalent
/// for 8-bit inputs to within ±1 LSB, which is below panel gamma noise.
pub fn rgba_to_rgb565_le_into(rgba: &[u8], out: &mut [u8]) {
    debug_assert_eq!(rgba.len() / 4 * 2, out.len());
    for (src, dst) in rgba.chunks_exact(4).zip(out.chunks_exact_mut(2)) {
        let r = u16::from(src[0] >> 3);
        let g = u16::from(src[1] >> 2);
        let b = u16::from(src[2] >> 3);
        let rgb565 = (r << 11) | (g << 5) | b;
        dst.copy_from_slice(&rgb565.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_rgba_to_rgb565_little_endian() {
        let converted = rgba_to_rgb565_le(&[
            0x00, 0x00, 0x00, 0xff, // black
            0xff, 0xff, 0xff, 0xff, // white
            0x80, 0x80, 0x80, 0xff, // mid-gray (truncation: 16/32/16)
        ]);

        assert_eq!(converted.len(), 6);
        assert_eq!(&converted[0..2], &0x0000_u16.to_le_bytes());
        assert_eq!(&converted[2..4], &0xffff_u16.to_le_bytes());
        // 0x80>>3=16 (R), 0x80>>2=32 (G), 0x80>>3=16 (B)
        // packed: (16<<11) | (32<<5) | 16 = 0x8410
        assert_eq!(&converted[4..6], &0x8410_u16.to_le_bytes());
    }

    #[test]
    fn rgba_to_rgb565_le_into_preserves_endianness_and_size() {
        let input = [
            0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0x80, 0x80, 0x80, 0xff,
        ];
        let mut out = [0xAAu8; 6];
        rgba_to_rgb565_le_into(&input, &mut out);
        assert_eq!(&out, &[0x00, 0x00, 0xff, 0xff, 0x10, 0x84]);
    }
}
