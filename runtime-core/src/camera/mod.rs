use std::time::Duration;

use crate::api::{
    CameraMode, CameraStateSnapshot, NormalizedScreenPoint, RuntimeConfig, ScreenPoint,
    ViewportSize, WorldPoint,
};
use crate::input::staging::DerivedInputState;
use crate::map::meters_per_pixel_for_zoom;
use crate::motion::{MotionState, normalize_bearing_rad};

#[derive(Debug, Clone, PartialEq)]
pub struct CameraState {
    pub mode: CameraMode,
    pub focus_world: WorldPoint,
    pub center_world: WorldPoint,
    pub zoom: f32,
    pub orientation_rad: f32,
    pub rider_anchor: NormalizedScreenPoint,
    pub follow_locked: bool,
    pub recenter_active: bool,
    pub pan_offset_world_m: (f64, f64),
    pub heading_offset_rad: f32,
    pub north_up_override_remaining: Duration,
    pub time_since_manual_input: Duration,
    pub stopped_heading_reference_rad: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            mode: CameraMode::Stopped,
            focus_world: WorldPoint::ORIGIN,
            center_world: WorldPoint::ORIGIN,
            zoom: RuntimeConfig::default().zoom_bounds.default,
            orientation_rad: 0.0,
            rider_anchor: NormalizedScreenPoint::CENTER,
            follow_locked: false,
            recenter_active: false,
            pan_offset_world_m: (0.0, 0.0),
            heading_offset_rad: 0.0,
            north_up_override_remaining: Duration::ZERO,
            time_since_manual_input: Duration::ZERO,
            stopped_heading_reference_rad: 0.0,
        }
    }
}

impl CameraState {
    pub fn advance(
        &mut self,
        motion: &MotionState,
        derived_input: &DerivedInputState,
        dt: Duration,
        viewport_size: ViewportSize,
        config: &RuntimeConfig,
    ) {
        let next_mode = if motion.is_moving {
            CameraMode::Riding
        } else {
            CameraMode::Stopped
        };
        if next_mode == CameraMode::Stopped && self.mode == CameraMode::Riding {
            self.stopped_heading_reference_rad = self.orientation_rad;
            self.north_up_override_remaining = Duration::ZERO;
        }
        self.mode = next_mode;

        self.apply_tap(derived_input.tap.as_ref(), viewport_size, config);
        self.apply_manual_interaction(&derived_input.gesture, dt, viewport_size, motion, config);
        self.advance_recenter(dt, config);

        self.zoom = config.zoom_bounds.clamp(self.zoom);
        self.focus_world = motion.rider_world;
        self.rider_anchor = match self.mode {
            CameraMode::Riding => config.riding_rider_anchor,
            CameraMode::Stopped => config.stopped_rider_anchor,
        };
        self.orientation_rad = self.resolve_orientation(motion, dt, config);

        let anchored_center = center_world_for_focus(
            self.focus_world,
            self.orientation_rad,
            self.rider_anchor,
            viewport_size,
            self.zoom,
        );
        self.center_world =
            anchored_center.translate(self.pan_offset_world_m.0, self.pan_offset_world_m.1);
    }

    pub fn snapshot(&self) -> CameraStateSnapshot {
        CameraStateSnapshot {
            mode: self.mode,
            focus_world: self.focus_world,
            center_world: self.center_world,
            zoom: self.zoom,
            orientation_rad: self.orientation_rad,
            rider_anchor: self.rider_anchor,
            follow_locked: self.follow_locked,
            recenter_active: self.recenter_active,
        }
    }

    pub fn north_up_override_active(&self) -> bool {
        self.mode == CameraMode::Riding && self.north_up_override_remaining > Duration::ZERO
    }

    fn apply_tap(
        &mut self,
        tap: Option<&crate::api::TapEvent>,
        viewport_size: ViewportSize,
        config: &RuntimeConfig,
    ) {
        let Some(tap) = tap else {
            return;
        };
        if self.mode != CameraMode::Riding {
            return;
        }
        if !north_indicator_hit(tap.position, viewport_size, config) {
            return;
        }

        self.north_up_override_remaining = if self.north_up_override_active() {
            Duration::ZERO
        } else {
            config.north_up_override_timeout
        };
    }

    fn apply_manual_interaction(
        &mut self,
        gesture: &crate::input::gestures::DerivedGesture,
        dt: Duration,
        viewport_size: ViewportSize,
        motion: &MotionState,
        config: &RuntimeConfig,
    ) {
        let interaction_active = gesture.touch_active;
        if interaction_active {
            self.time_since_manual_input = Duration::ZERO;
            self.recenter_active = false;
        } else {
            self.time_since_manual_input += dt;
        }

        if gesture.pan_delta_px.x_px != 0.0 || gesture.pan_delta_px.y_px != 0.0 {
            let orientation_rad = if self.north_up_override_active() {
                0.0
            } else {
                motion.travel_heading_rad.unwrap_or(self.orientation_rad) + self.heading_offset_rad
            };
            let (dx_m, dy_m) =
                world_pan_delta_from_screen(gesture.pan_delta_px, orientation_rad, self.zoom);
            self.pan_offset_world_m.0 += dx_m;
            self.pan_offset_world_m.1 += dy_m;
            self.follow_locked = true;
        }

        if (gesture.pinch_scale - 1.0).abs() > f32::EPSILON && gesture.pinch_scale > 0.0 {
            self.zoom += gesture.pinch_scale.log2();
        }

        if gesture.rotate_delta_rad != 0.0
            && self.mode == CameraMode::Riding
            && !self.north_up_override_active()
        {
            self.heading_offset_rad =
                normalize_signed_angle(self.heading_offset_rad + gesture.rotate_delta_rad);
        }

        if interaction_active {
            self.north_up_override_remaining = self
                .north_up_override_remaining
                .saturating_sub(dt.min(self.north_up_override_remaining));
        } else if self.north_up_override_active() {
            self.north_up_override_remaining = self
                .north_up_override_remaining
                .saturating_sub(dt.min(self.north_up_override_remaining));
        }

        if viewport_size.is_empty() {
            self.follow_locked = false;
        }
        if self.pan_offset_world_m == (0.0, 0.0) {
            self.follow_locked = false;
        }
        self.zoom = config.zoom_bounds.clamp(self.zoom);
    }

    fn advance_recenter(&mut self, dt: Duration, config: &RuntimeConfig) {
        let should_recenter = self.time_since_manual_input >= config.pan_recenter_timeout
            && (self.pan_offset_world_m.0 != 0.0
                || self.pan_offset_world_m.1 != 0.0
                || self.heading_offset_rad != 0.0);

        if !should_recenter {
            return;
        }

        self.recenter_active = true;
        let progress = (dt.as_secs_f64() / config.recenter_duration.as_secs_f64()).clamp(0.0, 1.0);
        self.pan_offset_world_m.0 *= 1.0 - progress;
        self.pan_offset_world_m.1 *= 1.0 - progress;
        self.heading_offset_rad *= (1.0 - progress) as f32;

        if self.pan_offset_world_m.0.hypot(self.pan_offset_world_m.1) < 0.25
            && self.heading_offset_rad.abs() < 0.001
        {
            self.pan_offset_world_m = (0.0, 0.0);
            self.heading_offset_rad = 0.0;
            self.follow_locked = false;
            self.recenter_active = false;
        }
    }

    fn resolve_orientation(
        &self,
        motion: &MotionState,
        dt: Duration,
        config: &RuntimeConfig,
    ) -> f32 {
        match self.mode {
            CameraMode::Riding => {
                if self.north_up_override_active() {
                    0.0
                } else {
                    normalize_bearing_rad(
                        motion.travel_heading_rad.unwrap_or(self.orientation_rad)
                            + self.heading_offset_rad,
                    )
                }
            }
            CameraMode::Stopped => {
                let hold_heading = self.stopped_heading_reference_rad;
                if motion.stopped_duration < config.stopped_north_up_delay {
                    normalize_bearing_rad(hold_heading)
                } else {
                    let settle_elapsed = motion
                        .stopped_duration
                        .saturating_sub(config.stopped_north_up_delay)
                        .as_secs_f32();
                    let settle_duration = config
                        .stopped_north_up_settle_duration
                        .as_secs_f32()
                        .max(f32::EPSILON);
                    let t = (settle_elapsed / settle_duration).clamp(0.0, 1.0);
                    let min_step = (dt.as_secs_f32() / settle_duration).clamp(0.0, 1.0);
                    interpolate_bearing(hold_heading, 0.0, t.max(min_step).clamp(0.0, 1.0))
                }
            }
        }
    }
}

fn center_world_for_focus(
    focus_world: WorldPoint,
    orientation_rad: f32,
    rider_anchor: NormalizedScreenPoint,
    viewport_size: ViewportSize,
    zoom: f32,
) -> WorldPoint {
    if viewport_size.is_empty() {
        return focus_world;
    }

    let meters_per_pixel = meters_per_pixel_for_zoom(zoom);
    let screen_dx_px = (0.5 - f64::from(rider_anchor.x)) * f64::from(viewport_size.width_px);
    let screen_dy_px = (0.5 - f64::from(rider_anchor.y)) * f64::from(viewport_size.height_px);
    let local_east_m = screen_dx_px * meters_per_pixel;
    let local_north_m = -screen_dy_px * meters_per_pixel;
    let sin_theta = f64::from(orientation_rad).sin();
    let cos_theta = f64::from(orientation_rad).cos();
    let world_dx = (local_east_m * cos_theta) + (local_north_m * sin_theta);
    let world_dy = (-local_east_m * sin_theta) + (local_north_m * cos_theta);
    focus_world.translate(world_dx, world_dy)
}

fn world_pan_delta_from_screen(
    pan_delta_px: ScreenPoint,
    orientation_rad: f32,
    zoom: f32,
) -> (f64, f64) {
    let meters_per_pixel = meters_per_pixel_for_zoom(zoom);
    let local_east_m = -f64::from(pan_delta_px.x_px) * meters_per_pixel;
    let local_north_m = f64::from(pan_delta_px.y_px) * meters_per_pixel;
    let sin_theta = f64::from(orientation_rad).sin();
    let cos_theta = f64::from(orientation_rad).cos();
    (
        (local_east_m * cos_theta) + (local_north_m * sin_theta),
        (-local_east_m * sin_theta) + (local_north_m * cos_theta),
    )
}

fn north_indicator_hit(
    tap_position: ScreenPoint,
    viewport_size: ViewportSize,
    config: &RuntimeConfig,
) -> bool {
    let indicator_center = ScreenPoint::new(
        config.north_indicator_center.x * viewport_size.width_px as f32,
        config.north_indicator_center.y * viewport_size.height_px as f32,
    );
    (tap_position.x_px - indicator_center.x_px).hypot(tap_position.y_px - indicator_center.y_px)
        <= config.north_indicator_hit_radius_px
}

fn interpolate_bearing(from_rad: f32, to_rad: f32, t: f32) -> f32 {
    let delta = normalize_signed_angle(to_rad - from_rad);
    normalize_bearing_rad(from_rad + (delta * t))
}

fn normalize_signed_angle(angle_rad: f32) -> f32 {
    let mut normalized = angle_rad;
    while normalized > std::f32::consts::PI {
        normalized -= std::f32::consts::TAU;
    }
    while normalized < -std::f32::consts::PI {
        normalized += std::f32::consts::TAU;
    }
    normalized
}
