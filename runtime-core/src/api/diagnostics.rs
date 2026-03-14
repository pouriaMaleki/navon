use super::input::ViewportSize;
use super::output::CameraMode;
use super::query::ZoomBucket;

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticsSnapshot {
    pub frame_index: u64,
    pub viewport_size: ViewportSize,
    pub has_gps_fix: bool,
    pub touch_contact_count: usize,
    pub active_touch_contacts: usize,
    pub camera_mode: CameraMode,
    pub zoom_bucket: ZoomBucket,
    pub meters_per_pixel: f64,
}

impl Default for DiagnosticsSnapshot {
    fn default() -> Self {
        Self {
            frame_index: 0,
            viewport_size: ViewportSize::default(),
            has_gps_fix: false,
            touch_contact_count: 0,
            active_touch_contacts: 0,
            camera_mode: CameraMode::Stopped,
            zoom_bucket: ZoomBucket::Neighborhood,
            meters_per_pixel: 1.0,
        }
    }
}
