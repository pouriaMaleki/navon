use std::time::{Duration, Instant};

use crate::framebuffer::RenderFramebuffer;

use crate::board_config::DisplayConfig;
use crate::framebuffer::Framebuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayError {
    Backend(String),
}

pub trait DisplayBackend: std::fmt::Debug {
    fn initialize(&mut self, config: DisplayConfig) -> Result<(), DisplayError>;
    fn present(&mut self, framebuffer: &Framebuffer) -> Result<(), DisplayError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryDisplayBackend {
    uploaded_bytes: usize,
}

impl MemoryDisplayBackend {
    pub fn uploaded_bytes(&self) -> usize {
        self.uploaded_bytes
    }
}

impl DisplayBackend for MemoryDisplayBackend {
    fn initialize(&mut self, _config: DisplayConfig) -> Result<(), DisplayError> {
        Ok(())
    }

    fn present(&mut self, framebuffer: &Framebuffer) -> Result<(), DisplayError> {
        self.uploaded_bytes = framebuffer.panel_pixels().len();
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Display<B = MemoryDisplayBackend>
where
    B: DisplayBackend,
{
    config: DisplayConfig,
    framebuffer: Framebuffer,
    backend: B,
    presented_frames: u64,
}

impl Display<MemoryDisplayBackend> {
    pub fn new(config: DisplayConfig) -> Self {
        Self::with_backend(config, MemoryDisplayBackend::default())
            .expect("memory display backend should initialize")
    }
}

impl<B> Display<B>
where
    B: DisplayBackend,
{
    pub fn with_backend(config: DisplayConfig, mut backend: B) -> Result<Self, DisplayError> {
        backend.initialize(config)?;
        Ok(Self {
            framebuffer: Framebuffer::new(
                config.viewport_size.width_px,
                config.viewport_size.height_px,
                config.pixel_format,
            ),
            config,
            backend,
            presented_frames: 0,
        })
    }

    pub fn present(&mut self, framebuffer: &RenderFramebuffer) -> Result<(), DisplayError> {
        self.present_timed(framebuffer).map(|_| ())
    }

    /// Same as `present`, but returns the elapsed time spent in
    /// (a) the RGBA→panel-format conversion and (b) the backend push.
    /// Used by the device boot loop to break the per-frame budget into
    /// "where did the time go". Always-on rather than feature-gated so
    /// the build output stays single-shape; cost is two `Instant::now()`
    /// calls per frame which is negligible vs. the work being timed.
    pub fn present_timed(
        &mut self,
        framebuffer: &RenderFramebuffer,
    ) -> Result<(Duration, Duration), DisplayError> {
        let t_convert = Instant::now();
        self.framebuffer.present_from_render(framebuffer);
        let convert_elapsed = t_convert.elapsed();

        let t_push = Instant::now();
        self.backend.present(&self.framebuffer)?;
        let push_elapsed = t_push.elapsed();

        self.presented_frames += 1;
        Ok((convert_elapsed, push_elapsed))
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn presented_frames(&self) -> u64 {
        self.presented_frames
    }

    pub fn config(&self) -> DisplayConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_config::{DisplayConfig, PanelPixelFormat};

    #[test]
    fn memory_display_backend_tracks_uploaded_panel_bytes() {
        let mut display = Display::new(DisplayConfig {
            viewport_size: runtime_core::api::ViewportSize::new(2, 2),
            pixel_format: PanelPixelFormat::Rgb565Le,
            reset_gpio: None,
            backlight_gpio: None,
        });
        let mut render = RenderFramebuffer::new(2, 2);
        render.pixels_mut().copy_from_slice(&[
            0, 64, 128, 255, 0, 64, 128, 255, 0, 64, 128, 255, 0, 64, 128, 255,
        ]);

        display.present(&render).expect("display upload");

        assert_eq!(display.presented_frames(), 1);
        assert_eq!(display.backend().uploaded_bytes(), 8);
        assert_eq!(display.framebuffer().panel_pixels().len(), 8);
    }
}
