use render_core::raster::Framebuffer as RenderFramebufferGeneric;
#[cfg(target_os = "espidf")]
use render_core::raster::Rgb565Pixel;
#[cfg(not(target_os = "espidf"))]
use render_core::raster::RgbaPixel;

use crate::board_config::PanelPixelFormat;

/// Render-core framebuffer specialization the firmware drives every frame.
///
/// On the ESP32-P4 device the rasterizer writes directly to RGB565 — the
/// panel-native format — so `present_from_render` becomes a memcpy from
/// the render buffer into the panel buffer with no per-pixel conversion.
/// This eliminates the 71 ms RGBA→RGB565 step that dominated the
/// non-geometry frame budget.
///
/// On host / wasm builds the rasterizer keeps writing 8-bit RGBA so the
/// pixel-perfect parity tests in `parity-fixtures` continue to compare
/// byte-identical buffers between firmware and wasm — that contract
/// would silently break if both lanes hashed RGB565 output.
#[cfg(target_os = "espidf")]
pub type RenderFramebuffer = RenderFramebufferGeneric<Rgb565Pixel>;
#[cfg(not(target_os = "espidf"))]
pub type RenderFramebuffer = RenderFramebufferGeneric<RgbaPixel>;

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
    // Owned panel-format storage. On host this is the RGB565 sink that
    // `rgba_to_rgb565_le_into` writes into. On device this Vec is kept
    // around as a fallback (e.g. for `lit_pixel_count` diagnostics), but
    // `present_from_render` does NOT copy into it — instead it stashes a
    // raw pointer to the render fb's RGB565 storage in
    // `panel_pixels_alias` and `panel_pixels()` returns through that ptr,
    // eliminating the 1.28 MB per-frame memcpy.
    panel_pixels: Vec<u8>,
    // Raw pointer to the most recent render fb's pixel slice. Set by
    // `present_from_render` on device, read by `panel_pixels()` between
    // that call and the next one. Outside of that window the pointer is
    // null and `panel_pixels()` falls back to the owned `panel_pixels`
    // Vec. SAFETY: relies on the Display::present_timed contract that
    // the render fb is borrowed for the entire backend.present call and
    // not mutated meanwhile.
    #[cfg(target_os = "espidf")]
    panel_pixels_alias: Option<core::ptr::NonNull<u8>>,
    #[cfg(target_os = "espidf")]
    panel_pixels_alias_len: usize,
    panel_format: PanelPixelFormat,
}

// SAFETY: the raw pointer is only set/read during the synchronous call
// chain inside `Display::present_timed`, which is itself called from a
// single FreeRTOS task. We never share the Framebuffer across threads
// while the alias is active.
#[cfg(target_os = "espidf")]
unsafe impl Send for Framebuffer {}
#[cfg(target_os = "espidf")]
unsafe impl Sync for Framebuffer {}

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
            #[cfg(target_os = "espidf")]
            panel_pixels_alias: None,
            #[cfg(target_os = "espidf")]
            panel_pixels_alias_len: 0,
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
                #[cfg(target_os = "espidf")]
                {
                    // Aliased fast path. Render fb's bytes ARE the
                    // panel bytes (already RGB565). Stash the pointer
                    // and length; the panel backend reads via
                    // `panel_pixels()` immediately after this returns.
                    // No memcpy of 1.28 MB.
                    let bytes = framebuffer.pixels();
                    self.panel_pixels_alias =
                        core::ptr::NonNull::new(bytes.as_ptr() as *mut u8);
                    self.panel_pixels_alias_len = bytes.len();
                }
                #[cfg(not(target_os = "espidf"))]
                {
                    rgba_to_rgb565_le_into(framebuffer.pixels(), &mut self.panel_pixels);
                }
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
        #[cfg(target_os = "espidf")]
        {
            if let Some(ptr) = self.panel_pixels_alias {
                // SAFETY: `present_from_render` set this pointer to the
                // render fb's pixel slice, which is borrowed by the
                // caller of `Display::present_timed` for the entire
                // duration of the backend.present call that immediately
                // follows. We never mutate the firmware Framebuffer in
                // that window, and the alias is reset (or replaced) on
                // the next `present_from_render` call.
                return unsafe {
                    core::slice::from_raw_parts(ptr.as_ptr(), self.panel_pixels_alias_len)
                };
            }
        }
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
