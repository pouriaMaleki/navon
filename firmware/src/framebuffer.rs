use render_core::raster::Framebuffer as RenderFramebuffer;

use crate::board_config::PanelPixelFormat;

#[derive(Debug, Clone, PartialEq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    panel_pixels: Vec<u8>,
    panel_format: PanelPixelFormat,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32, panel_format: PanelPixelFormat) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize],
            panel_pixels: Vec::new(),
            panel_format,
        }
    }

    pub fn present_from_render(&mut self, framebuffer: &RenderFramebuffer) {
        self.width = framebuffer.width();
        self.height = framebuffer.height();
        self.pixels.clear();
        self.pixels.extend_from_slice(framebuffer.pixels());
        self.panel_pixels = match self.panel_format {
            PanelPixelFormat::Rgb565Le => grayscale_to_rgb565_le(framebuffer.pixels()),
        };
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

    pub fn panel_pixels(&self) -> &[u8] {
        &self.panel_pixels
    }

    pub fn panel_format(&self) -> PanelPixelFormat {
        self.panel_format
    }

    pub fn lit_pixel_count(&self) -> usize {
        self.pixels
            .iter()
            .copied()
            .filter(|value| *value > 0)
            .count()
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new(0, 0, PanelPixelFormat::Rgb565Le)
    }
}

pub fn grayscale_to_rgb565_le(pixels: &[u8]) -> Vec<u8> {
    let mut converted = Vec::with_capacity(pixels.len() * 2);
    for pixel in pixels {
        let red = (u16::from(*pixel) * 31 / 255) << 11;
        let green = (u16::from(*pixel) * 63 / 255) << 5;
        let blue = u16::from(*pixel) * 31 / 255;
        let rgb565 = red | green | blue;
        converted.extend_from_slice(&rgb565.to_le_bytes());
    }
    converted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_grayscale_to_rgb565_little_endian() {
        let converted = grayscale_to_rgb565_le(&[0x00, 0xff, 0x80]);

        assert_eq!(converted.len(), 6);
        assert_eq!(&converted[0..2], &0x0000_u16.to_le_bytes());
        assert_eq!(&converted[2..4], &0xffff_u16.to_le_bytes());
        assert_eq!(&converted[4..6], &0x7bef_u16.to_le_bytes());
    }
}
