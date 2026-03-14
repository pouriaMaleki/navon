use render_core::raster::Framebuffer as RenderFramebuffer;

use crate::framebuffer::Framebuffer;

#[derive(Debug, Clone, Default)]
pub struct Display {
    framebuffer: Framebuffer,
    presented_frames: u64,
}

impl Display {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn present(&mut self, framebuffer: &RenderFramebuffer) {
        self.framebuffer.present_from_render(framebuffer);
        self.presented_frames += 1;
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn presented_frames(&self) -> u64 {
        self.presented_frames
    }
}
