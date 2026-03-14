use runtime_core::api::ViewportSize;

pub const DISPLAY_WIDTH_PX: u32 = 800;
pub const DISPLAY_HEIGHT_PX: u32 = 800;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardConfig {
    pub viewport_size: ViewportSize,
}

impl BoardConfig {
    pub const fn new(viewport_size: ViewportSize) -> Self {
        Self { viewport_size }
    }
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self::new(ViewportSize::new(DISPLAY_WIDTH_PX, DISPLAY_HEIGHT_PX))
    }
}
