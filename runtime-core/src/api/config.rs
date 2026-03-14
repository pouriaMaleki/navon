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
    pub north_indicator_center: NormalizedScreenPoint,
    pub zoom_bounds: ZoomBounds,
    pub riding_speed_threshold_mps: f32,
    pub stopped_speed_threshold_mps: f32,
    pub gps_loss_stop_timeout: Duration,
    pub tap_max_duration: Duration,
    pub tap_max_travel_px: f32,
    pub pan_deadzone_px: f32,
    pub rotate_deadzone_rad: f32,
    pub pan_recenter_timeout: Duration,
    pub recenter_duration: Duration,
    pub north_up_override_timeout: Duration,
    pub north_indicator_hit_radius_px: f32,
    pub stopped_north_up_delay: Duration,
    pub stopped_north_up_settle_duration: Duration,
    pub min_heading_displacement_m: f64,
    pub heading_filter_alpha: f32,
    pub diagnostics_enabled: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            viewport_size: ViewportSize::new(480, 480),
            riding_rider_anchor: NormalizedScreenPoint::new(0.5, 0.72),
            stopped_rider_anchor: NormalizedScreenPoint::CENTER,
            north_indicator_center: NormalizedScreenPoint::new(0.88, 0.12),
            zoom_bounds: ZoomBounds::default(),
            riding_speed_threshold_mps: 1.5,
            stopped_speed_threshold_mps: 0.6,
            gps_loss_stop_timeout: Duration::from_millis(1_000),
            tap_max_duration: Duration::from_millis(260),
            tap_max_travel_px: 10.0,
            pan_deadzone_px: 8.0,
            rotate_deadzone_rad: 0.025,
            pan_recenter_timeout: Duration::from_millis(1_500),
            recenter_duration: Duration::from_millis(420),
            north_up_override_timeout: Duration::from_millis(2_200),
            north_indicator_hit_radius_px: 48.0,
            stopped_north_up_delay: Duration::from_millis(600),
            stopped_north_up_settle_duration: Duration::from_millis(900),
            min_heading_displacement_m: 3.0,
            heading_filter_alpha: 0.25,
            diagnostics_enabled: true,
        }
    }
}
