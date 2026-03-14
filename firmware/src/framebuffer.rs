use render_core::raster::Framebuffer as RenderFramebuffer;

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
            pixels: vec![0; width as usize * height as usize],
        }
    }

    pub fn present_from_render(&mut self, framebuffer: &RenderFramebuffer) {
        self.width = framebuffer.width();
        self.height = framebuffer.height();
        self.pixels.clear();
        self.pixels.extend_from_slice(framebuffer.pixels());
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
        Self::new(0, 0)
    }
}
