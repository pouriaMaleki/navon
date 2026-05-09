#[allow(unused_imports)]
use num_traits::Float as _;
use core::time::Duration;

use super::input::ViewportSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeedUnit {
    #[default]
    Kph,
    Mph,
}

impl SpeedUnit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kph => "kph",
            Self::Mph => "mph",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Kph => Self::Mph,
            Self::Mph => Self::Kph,
        }
    }

    pub fn rounded_display_value_from_mps(self, speed_mps: f32) -> u16 {
        let speed = speed_mps.max(0.0);
        let factor = match self {
            Self::Kph => 3.6,
            Self::Mph => 2.236_936_3,
        };
        (speed * factor).round().clamp(0.0, u16::MAX as f32) as u16
    }

    pub fn from_storage_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "kph" => Some(Self::Kph),
            "mph" => Some(Self::Mph),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RouteAlertVerbosity {
    Essential,
    #[default]
    Standard,
    Detailed,
}

impl RouteAlertVerbosity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Essential => "essential",
            Self::Standard => "standard",
            Self::Detailed => "detailed",
        }
    }

    pub fn from_storage_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "essential" | "minimal" => Some(Self::Essential),
            "standard" | "default" => Some(Self::Standard),
            "detailed" | "verbose" => Some(Self::Detailed),
            _ => None,
        }
    }

    pub const fn major_turn_enabled(self) -> bool {
        !matches!(self, Self::Essential)
    }

    pub const fn detailed_major_turn_text(self) -> bool {
        matches!(self, Self::Detailed)
    }
}

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
    pub default_speed_unit: SpeedUnit,
    pub route_alert_verbosity: RouteAlertVerbosity,
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
    pub mode_transition_duration: Duration,
    pub heading_acquisition_delay: Duration,
    pub north_preview_timeout: Duration,
    pub compass_double_tap_window: Duration,
    pub compass_ack_duration: Duration,
    pub north_indicator_hit_radius_px: f32,
    pub stopped_north_up_delay: Duration,
    pub stopped_north_up_settle_duration: Duration,
    pub min_heading_displacement_m: f64,
    pub heading_filter_alpha: f32,
    pub off_route_enter_distance_m: f64,
    pub off_route_exit_distance_m: f64,
    pub major_turn_alert_distance_m: f64,
    pub reroute_request_delay: Duration,
    pub diagnostics_enabled: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            viewport_size: ViewportSize::new(480, 480),
            default_speed_unit: SpeedUnit::Kph,
            route_alert_verbosity: RouteAlertVerbosity::Standard,
            riding_rider_anchor: NormalizedScreenPoint::new(0.5, 0.72),
            stopped_rider_anchor: NormalizedScreenPoint::CENTER,
            north_indicator_center: NormalizedScreenPoint::new(0.5, 0.12),
            zoom_bounds: ZoomBounds::default(),
            // Motion thresholds follow the pinned values in
            // `parity-fixtures/data/ux-constants.toml` (enter 0.5 kph, exit
            // 0.3 kph) which `docs/ux-specs.md` line 52 mandates. If this
            // drifts the parity-fixtures tests (`motion.rs` default-config
            // cases) will fail.
            riding_speed_threshold_mps: 0.139,
            stopped_speed_threshold_mps: 0.083,
            gps_loss_stop_timeout: Duration::from_millis(1_000),
            tap_max_duration: Duration::from_millis(260),
            tap_max_travel_px: 10.0,
            pan_deadzone_px: 8.0,
            rotate_deadzone_rad: 0.025,
            pan_recenter_timeout: Duration::from_millis(1_500),
            recenter_duration: Duration::from_millis(420),
            mode_transition_duration: Duration::from_millis(220),
            heading_acquisition_delay: Duration::from_millis(800),
            north_preview_timeout: Duration::from_millis(2_500),
            compass_double_tap_window: Duration::from_millis(400),
            compass_ack_duration: Duration::from_millis(220),
            north_indicator_hit_radius_px: 48.0,
            stopped_north_up_delay: Duration::from_millis(600),
            stopped_north_up_settle_duration: Duration::from_millis(900),
            min_heading_displacement_m: 3.0,
            heading_filter_alpha: 0.25,
            off_route_enter_distance_m: 35.0,
            off_route_exit_distance_m: 22.0,
            major_turn_alert_distance_m: 80.0,
            reroute_request_delay: Duration::from_millis(2_000),
            diagnostics_enabled: true,
        }
    }
}
