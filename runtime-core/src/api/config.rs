use std::time::Duration;

use super::input::ViewportSize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedScreenPoint {
    pub x: f32,
    pub y: f32,
}

impl NormalizedScreenPoint {
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };

    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
        }
    }
}

impl Default for NormalizedScreenPoint {
    fn default() -> Self {
        Self::CENTER
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomBounds {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ZoomBounds {
    pub fn clamp(self, zoom: f32) -> f32 {
        zoom.clamp(self.min, self.max)
    }
}

impl Default for ZoomBounds {
    fn default() -> Self {
        Self {
            min: 12.0,
            max: 18.0,
            default: 15.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeConfig {
    pub viewport_size: ViewportSize,
    pub riding_rider_anchor: NormalizedScreenPoint,
    pub stopped_rider_anchor: NormalizedScreenPoint,
    pub zoom_bounds: ZoomBounds,
    pub riding_speed_threshold_mps: f32,
    pub stopped_speed_threshold_mps: f32,
    pub gps_loss_stop_timeout: Duration,
    pub pan_recenter_timeout: Duration,
    pub diagnostics_enabled: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            viewport_size: ViewportSize::new(480, 480),
            riding_rider_anchor: NormalizedScreenPoint::new(0.5, 0.72),
            stopped_rider_anchor: NormalizedScreenPoint::CENTER,
            zoom_bounds: ZoomBounds::default(),
            riding_speed_threshold_mps: 1.5,
            stopped_speed_threshold_mps: 0.6,
            gps_loss_stop_timeout: Duration::from_millis(1_000),
            pan_recenter_timeout: Duration::from_millis(1_500),
            diagnostics_enabled: true,
        }
    }
}
