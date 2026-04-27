#[allow(unused_imports)]
use num_traits::Float as _;
use core::time::Duration;

use crate::api::{
    CameraMode, CameraOrientationMode, CameraStateSnapshot, NormalizedScreenPoint, RuntimeConfig,
    ScreenPoint, ViewportSize, WorldPoint,
};
use crate::input::staging::DerivedInputState;
use crate::map::meters_per_pixel_for_zoom;
use crate::motion::{MotionState, normalize_bearing_rad};

#[derive(Debug, Clone, PartialEq)]
pub struct CameraState {
    pub mode: CameraMode,
    pub orientation_mode: CameraOrientationMode,
    pub focus_world: WorldPoint,
    pub center_world: WorldPoint,
    pub zoom: f32,
    pub orientation_rad: f32,
    pub rider_anchor: NormalizedScreenPoint,
    pub follow_locked: bool,
    pub recenter_active: bool,
    pub pan_offset_world_m: (f64, f64),
    pub heading_offset_rad: f32,
    pub north_preview_active: bool,
    pub north_preview_remaining: Duration,
    pub north_locked: bool,
    pub compass_ack_remaining: Duration,
    pub last_compass_tap_age: Option<Duration>,
    pub time_since_manual_input: Duration,
    pub recenter_requested: bool,
    pub stopped_heading_reference_rad: f32,
    pub heading_ready_duration: Duration,
    pub heading_acquired: bool,
    pub last_trusted_travel_heading_rad: Option<f32>,
    pub heading_acquisition_reference_rad: Option<f32>,
    pub anchor_transition_remaining: Duration,
    pub anchor_transition_start: NormalizedScreenPoint,
    pub anchor_transition_target: NormalizedScreenPoint,
    pub orientation_transition_remaining: Duration,
    pub orientation_transition_start_orientation_rad: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            mode: CameraMode::Stopped,
            orientation_mode: CameraOrientationMode::StoppedNorthUp,
            focus_world: WorldPoint::ORIGIN,
            center_world: WorldPoint::ORIGIN,
            zoom: RuntimeConfig::default().zoom_bounds.default,
            orientation_rad: 0.0,
            rider_anchor: NormalizedScreenPoint::CENTER,
            follow_locked: false,
            recenter_active: false,
            pan_offset_world_m: (0.0, 0.0),
            heading_offset_rad: 0.0,
            north_preview_active: false,
            north_preview_remaining: Duration::ZERO,
            north_locked: false,
            compass_ack_remaining: Duration::ZERO,
            last_compass_tap_age: None,
            time_since_manual_input: Duration::ZERO,
            recenter_requested: false,
            stopped_heading_reference_rad: 0.0,
            heading_ready_duration: Duration::ZERO,
            heading_acquired: false,
            last_trusted_travel_heading_rad: None,
            heading_acquisition_reference_rad: None,
            anchor_transition_remaining: Duration::ZERO,
            anchor_transition_start: NormalizedScreenPoint::CENTER,
            anchor_transition_target: NormalizedScreenPoint::CENTER,
            orientation_transition_remaining: Duration::ZERO,
            orientation_transition_start_orientation_rad: 0.0,
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
        let previous_mode = self.mode;
        let previous_orientation_mode = self.orientation_mode;

        self.mode = if motion.is_moving {
            CameraMode::Riding
        } else {
            CameraMode::Stopped
        };

        if self.mode == CameraMode::Stopped && previous_mode == CameraMode::Riding {
            self.stopped_heading_reference_rad = self.orientation_rad;
            self.orientation_transition_remaining = Duration::ZERO;
            self.heading_acquisition_reference_rad = None;
        } else if self.mode == CameraMode::Riding && previous_mode == CameraMode::Stopped {
            self.heading_acquisition_reference_rad = Some(0.0);
        }

        self.update_heading_tracking(motion, dt, config);
        self.advance_compass_tap_window(dt, config);
        self.advance_compass_ack(dt);
        self.apply_tap(derived_input.tap.as_ref(), viewport_size, motion, config);
        self.apply_manual_interaction(&derived_input.gesture, dt, viewport_size, config);
        self.advance_preview_timer(&derived_input.gesture, motion, dt);
        self.advance_recenter(dt, config);

        self.zoom = config.zoom_bounds.clamp(self.zoom);
        self.focus_world = motion.rider_world;

        let next_orientation_mode = self.resolve_orientation_mode();
        let target_rider_anchor = target_rider_anchor(next_orientation_mode, config);
        self.start_anchor_transition_if_needed(target_rider_anchor, config);
        let target_orientation =
            self.resolve_target_orientation(motion, next_orientation_mode, dt, config);
        self.start_orientation_transition_if_needed(
            previous_orientation_mode,
            next_orientation_mode,
            target_orientation,
            config,
        );

        self.rider_anchor = self.advance_rider_anchor(target_rider_anchor, dt, config);
        self.orientation_rad =
            self.advance_orientation(target_orientation, dt, config, next_orientation_mode);
        self.orientation_mode = next_orientation_mode;

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

    pub fn snapshot(&self, config: &RuntimeConfig) -> CameraStateSnapshot {
        CameraStateSnapshot {
            mode: self.mode,
            orientation_mode: self.orientation_mode,
            focus_world: self.focus_world,
            center_world: self.center_world,
            zoom: self.zoom,
            orientation_rad: self.orientation_rad,
            north_preview_progress: self.north_preview_active.then_some(progress_ratio(
                self.north_preview_remaining,
                config.north_preview_timeout,
            )),
            compass_ack_progress: progress_ratio(
                self.compass_ack_remaining,
                config.compass_ack_duration,
            ),
            rider_anchor: self.rider_anchor,
            follow_locked: self.follow_locked,
            recenter_active: self.recenter_active,
        }
    }

    fn update_heading_tracking(
        &mut self,
        motion: &MotionState,
        dt: Duration,
        config: &RuntimeConfig,
    ) {
        if self.mode != CameraMode::Riding {
            self.heading_ready_duration = Duration::ZERO;
            self.heading_acquired = false;
            self.heading_acquisition_reference_rad = None;
            self.north_preview_active = false;
            self.north_preview_remaining = Duration::ZERO;
            return;
        }

        if motion.heading_confident && motion.travel_heading_rad.is_some() {
            self.heading_ready_duration += dt;
            self.heading_acquired = self.heading_ready_duration >= config.heading_acquisition_delay;
            if self.heading_ready_duration >= config.heading_acquisition_delay {
                self.heading_acquisition_reference_rad = None;
            }
            self.last_trusted_travel_heading_rad = motion.travel_heading_rad;
        } else {
            if self.heading_acquired {
                self.heading_acquisition_reference_rad = Some(self.trusted_travel_orientation());
            }
            self.heading_ready_duration = Duration::ZERO;
            self.heading_acquired = false;
            if self.heading_acquisition_reference_rad.is_none() {
                self.heading_acquisition_reference_rad = Some(self.orientation_rad);
            }
        }
    }

    fn advance_compass_tap_window(&mut self, dt: Duration, config: &RuntimeConfig) {
        self.last_compass_tap_age = self.last_compass_tap_age.and_then(|age| {
            let next = age + dt;
            (next <= config.compass_double_tap_window).then_some(next)
        });
    }

    fn advance_compass_ack(&mut self, dt: Duration) {
        self.compass_ack_remaining = self
            .compass_ack_remaining
            .saturating_sub(dt.min(self.compass_ack_remaining));
    }

    fn apply_tap(
        &mut self,
        tap: Option<&crate::api::TapEvent>,
        viewport_size: ViewportSize,
        motion: &MotionState,
        config: &RuntimeConfig,
    ) {
        let Some(tap) = tap else {
            return;
        };
        if !north_indicator_hit(tap.position, viewport_size, config) {
            return;
        }

        if self.north_locked {
            self.north_locked = false;
            self.north_preview_active = false;
            self.north_preview_remaining = Duration::ZERO;
            self.last_compass_tap_age = None;
            if !(motion.heading_confident && motion.travel_heading_rad.is_some()) {
                self.heading_acquisition_reference_rad = Some(self.trusted_travel_orientation());
                self.heading_acquired = false;
                self.heading_ready_duration = Duration::ZERO;
            }
            return;
        }

        if self.mode == CameraMode::Stopped && self.has_manual_camera_offset() {
            self.recenter_requested = true;
            return;
        }

        if self.mode != CameraMode::Riding || !self.heading_acquired {
            self.compass_ack_remaining = config.compass_ack_duration;
            return;
        }

        let second_tap_locks = self.north_preview_active
            && self
                .last_compass_tap_age
                .is_some_and(|age| age <= config.compass_double_tap_window);
        if second_tap_locks {
            self.north_locked = true;
            self.north_preview_active = false;
            self.north_preview_remaining = Duration::ZERO;
            self.last_compass_tap_age = None;
            return;
        }

        self.north_preview_active = true;
        self.north_preview_remaining = config.north_preview_timeout;
        self.last_compass_tap_age = Some(Duration::ZERO);
    }

    fn apply_manual_interaction(
        &mut self,
        gesture: &crate::input::gestures::DerivedGesture,
        dt: Duration,
        viewport_size: ViewportSize,
        config: &RuntimeConfig,
    ) {
        let interaction_active = gesture.touch_active;
        if interaction_active {
            self.time_since_manual_input = Duration::ZERO;
            self.recenter_active = false;
            self.recenter_requested = false;
        } else {
            self.time_since_manual_input += dt;
        }

        if gesture.pan_delta_px.x_px != 0.0 || gesture.pan_delta_px.y_px != 0.0 {
            let (dx_m, dy_m) =
                world_pan_delta_from_screen(gesture.pan_delta_px, self.orientation_rad, self.zoom);
            self.pan_offset_world_m.0 += dx_m;
            self.pan_offset_world_m.1 += dy_m;
            self.follow_locked = true;
            self.recenter_requested = false;
        }

        if (gesture.pinch_scale - 1.0).abs() > f32::EPSILON && gesture.pinch_scale > 0.0 {
            self.zoom += gesture.pinch_scale.log2();
        }

        // Spec line 47: "user can pinch to zoom or with two fingers rotate
        // (zoom and rotation work at the same time too)". Apply the rotation
        // delta in every orientation mode, not just TravelUpAuto — otherwise
        // two-finger rotate is silently dropped while stationary.
        //
        // Negate: rotate_delta_rad is positive for clockwise screen gestures
        // (atan2 in Y-down screen coords).  A clockwise gesture should rotate
        // the map clockwise (north goes right), which is a DECREASE in bearing.
        // Subtracting brings the two coordinate systems into agreement.
        if gesture.rotate_delta_rad != 0.0 {
            self.heading_offset_rad =
                normalize_signed_angle(self.heading_offset_rad - gesture.rotate_delta_rad);
        }

        if viewport_size.is_empty() {
            self.follow_locked = false;
        }
        if self.pan_offset_world_m == (0.0, 0.0) {
            self.follow_locked = false;
        }
        self.zoom = config.zoom_bounds.clamp(self.zoom);
    }

    fn advance_preview_timer(
        &mut self,
        gesture: &crate::input::gestures::DerivedGesture,
        motion: &MotionState,
        dt: Duration,
    ) {
        if !self.north_preview_active || self.mode != CameraMode::Riding || self.north_locked {
            return;
        }
        if gesture.touch_active {
            return;
        }

        self.north_preview_remaining = self
            .north_preview_remaining
            .saturating_sub(dt.min(self.north_preview_remaining));
        if self.north_preview_remaining == Duration::ZERO
            && motion.heading_confident
            && motion.travel_heading_rad.is_some()
        {
            self.north_preview_active = false;
            self.last_compass_tap_age = None;
        }
    }

    fn advance_recenter(&mut self, dt: Duration, config: &RuntimeConfig) {
        let has_manual_offset = self.has_manual_camera_offset();
        let idle_timeout_elapsed = self.time_since_manual_input >= config.pan_recenter_timeout;
        let should_recenter = has_manual_offset
            && (self.recenter_requested
                || (self.mode == CameraMode::Riding && idle_timeout_elapsed));

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
            self.recenter_requested = false;
        }
    }

    fn has_manual_camera_offset(&self) -> bool {
        self.pan_offset_world_m.0 != 0.0
            || self.pan_offset_world_m.1 != 0.0
            || self.heading_offset_rad != 0.0
    }

    fn resolve_orientation_mode(&self) -> CameraOrientationMode {
        if self.north_locked {
            CameraOrientationMode::NorthLocked
        } else if self.mode == CameraMode::Stopped {
            CameraOrientationMode::StoppedNorthUp
        } else if self.north_preview_active {
            CameraOrientationMode::NorthPreview
        } else if self.heading_acquired {
            CameraOrientationMode::TravelUpAuto
        } else {
            CameraOrientationMode::HeadingAcquisition
        }
    }

    fn resolve_target_orientation(
        &self,
        motion: &MotionState,
        orientation_mode: CameraOrientationMode,
        dt: Duration,
        config: &RuntimeConfig,
    ) -> f32 {
        match orientation_mode {
            CameraOrientationMode::StoppedNorthUp => {
                // Spec line 47 (ESP) + 94 (companion): the user can rotate
                // the map with two fingers even while stationary. That
                // rotation is held in `heading_offset_rad` and the inactivity
                // recenter animation clears it (see `advance_recenter`). So
                // we add the offset to the target orientation in this mode
                // too, not only in TravelUpAuto.
                let hold_heading = self.stopped_heading_reference_rad;
                let base = if motion.stopped_duration < config.stopped_north_up_delay {
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
                };
                normalize_bearing_rad(base + self.heading_offset_rad)
            }
            CameraOrientationMode::HeadingAcquisition => self
                .heading_acquisition_reference_rad
                .or_else(|| {
                    self.last_trusted_travel_heading_rad
                        .map(|heading| normalize_bearing_rad(heading + self.heading_offset_rad))
                })
                .unwrap_or_else(|| normalize_bearing_rad(self.orientation_rad)),
            CameraOrientationMode::TravelUpAuto => {
                let base_heading = if motion.heading_confident {
                    motion
                        .travel_heading_rad
                        .or(self.last_trusted_travel_heading_rad)
                } else {
                    self.last_trusted_travel_heading_rad
                        .or(motion.travel_heading_rad)
                };
                base_heading
                    .map(|heading| normalize_bearing_rad(heading + self.heading_offset_rad))
                    .unwrap_or_else(|| normalize_bearing_rad(self.orientation_rad))
            }
            CameraOrientationMode::NorthPreview | CameraOrientationMode::NorthLocked => 0.0,
        }
    }

    fn trusted_travel_orientation(&self) -> f32 {
        self.last_trusted_travel_heading_rad
            .map(|heading| normalize_bearing_rad(heading + self.heading_offset_rad))
            .unwrap_or_else(|| normalize_bearing_rad(self.orientation_rad))
    }

    fn start_anchor_transition_if_needed(
        &mut self,
        target_rider_anchor: NormalizedScreenPoint,
        config: &RuntimeConfig,
    ) {
        if self.anchor_transition_target == target_rider_anchor {
            return;
        }
        self.anchor_transition_remaining = config.mode_transition_duration;
        self.anchor_transition_start = self.rider_anchor;
        self.anchor_transition_target = target_rider_anchor;
    }

    fn start_orientation_transition_if_needed(
        &mut self,
        previous_orientation_mode: CameraOrientationMode,
        next_orientation_mode: CameraOrientationMode,
        target_orientation: f32,
        config: &RuntimeConfig,
    ) {
        if previous_orientation_mode == next_orientation_mode {
            return;
        }
        if matches!(next_orientation_mode, CameraOrientationMode::StoppedNorthUp) {
            self.orientation_transition_remaining = Duration::ZERO;
            return;
        }
        if normalize_signed_angle(target_orientation - self.orientation_rad).abs() < 0.001 {
            self.orientation_transition_remaining = Duration::ZERO;
            return;
        }
        self.orientation_transition_remaining = config.mode_transition_duration;
        self.orientation_transition_start_orientation_rad = self.orientation_rad;
    }

    fn advance_rider_anchor(
        &mut self,
        target_rider_anchor: NormalizedScreenPoint,
        dt: Duration,
        config: &RuntimeConfig,
    ) -> NormalizedScreenPoint {
        if self.anchor_transition_remaining == Duration::ZERO {
            self.anchor_transition_target = target_rider_anchor;
            return target_rider_anchor;
        }
        let remaining_after_step = self
            .anchor_transition_remaining
            .saturating_sub(dt.min(self.anchor_transition_remaining));
        let progress = transition_progress(
            self.anchor_transition_remaining,
            remaining_after_step,
            config.mode_transition_duration,
        );
        self.anchor_transition_remaining = remaining_after_step;
        if self.anchor_transition_remaining == Duration::ZERO {
            self.anchor_transition_target
        } else {
            lerp_screen_point(
                self.anchor_transition_start,
                self.anchor_transition_target,
                progress,
            )
        }
    }

    fn advance_orientation(
        &mut self,
        target_orientation: f32,
        dt: Duration,
        config: &RuntimeConfig,
        orientation_mode: CameraOrientationMode,
    ) -> f32 {
        if matches!(orientation_mode, CameraOrientationMode::StoppedNorthUp) {
            return target_orientation;
        }
        if self.orientation_transition_remaining == Duration::ZERO {
            return target_orientation;
        }

        let remaining_after_step = self
            .orientation_transition_remaining
            .saturating_sub(dt.min(self.orientation_transition_remaining));
        let progress = transition_progress(
            self.orientation_transition_remaining,
            remaining_after_step,
            config.mode_transition_duration,
        );
        self.orientation_transition_remaining = remaining_after_step;
        if self.orientation_transition_remaining == Duration::ZERO {
            target_orientation
        } else {
            interpolate_bearing(
                self.orientation_transition_start_orientation_rad,
                target_orientation,
                progress,
            )
        }
    }
}

fn target_rider_anchor(
    orientation_mode: CameraOrientationMode,
    config: &RuntimeConfig,
) -> NormalizedScreenPoint {
    match orientation_mode {
        CameraOrientationMode::TravelUpAuto => config.riding_rider_anchor,
        CameraOrientationMode::StoppedNorthUp
        | CameraOrientationMode::HeadingAcquisition
        | CameraOrientationMode::NorthPreview
        | CameraOrientationMode::NorthLocked => config.stopped_rider_anchor,
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

fn transition_progress(before: Duration, after: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 1.0;
    }
    let elapsed = (total.saturating_sub(before).as_secs_f32()
        + before.saturating_sub(after).as_secs_f32())
    .clamp(0.0, total.as_secs_f32());
    (elapsed / total.as_secs_f32()).clamp(0.0, 1.0)
}

fn progress_ratio(remaining: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 0.0;
    }
    (remaining.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
}

fn lerp_screen_point(
    from: NormalizedScreenPoint,
    to: NormalizedScreenPoint,
    t: f32,
) -> NormalizedScreenPoint {
    NormalizedScreenPoint::new(
        from.x + ((to.x - from.x) * t),
        from.y + ((to.y - from.y) * t),
    )
}

fn interpolate_bearing(from_rad: f32, to_rad: f32, t: f32) -> f32 {
    let delta = normalize_signed_angle(to_rad - from_rad);
    normalize_bearing_rad(from_rad + (delta * t))
}

fn normalize_signed_angle(angle_rad: f32) -> f32 {
    let mut normalized = angle_rad;
    while normalized > core::f32::consts::PI {
        normalized -= core::f32::consts::TAU;
    }
    while normalized < -core::f32::consts::PI {
        normalized += core::f32::consts::TAU;
    }
    normalized
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::api::{RuntimeConfig, ViewportSize};
    use crate::input::gestures::DerivedGesture;
    use crate::input::staging::DerivedInputState;
    use crate::motion::MotionState;

    fn one_frame_with_gesture(rotate_delta_rad: f32) -> CameraState {
        let mut camera = CameraState::default();
        let derived = DerivedInputState {
            gesture: DerivedGesture {
                rotate_delta_rad,
                touch_active: true,
                ..DerivedGesture::default()
            },
            tap: None,
        };
        camera.advance(
            &MotionState::default(),
            &derived,
            Duration::from_millis(16),
            ViewportSize::new(800, 800),
            &RuntimeConfig::default(),
        );
        camera
    }

    #[test]
    fn clockwise_gesture_rotates_map_clockwise() {
        // A clockwise two-finger gesture in screen space (rotate_delta_rad > 0,
        // per Y-down atan2 convention) should make north go toward the right edge
        // of the screen — i.e. heading_offset_rad must DECREASE.
        //
        // heading_offset_rad is the internal signed accumulator; orientation_rad
        // goes through normalize_bearing_rad([0, 2π)) so negative deltas wrap
        // to near-2π.  We test heading_offset_rad, not orientation_rad.
        let camera = one_frame_with_gesture(0.4);
        assert!(
            camera.heading_offset_rad < 0.0,
            "clockwise gesture must decrease heading_offset_rad (north goes right); got {}",
            camera.heading_offset_rad
        );
    }

    #[test]
    fn counterclockwise_gesture_rotates_map_counterclockwise() {
        let camera = one_frame_with_gesture(-0.4);
        assert!(
            camera.heading_offset_rad > 0.0,
            "counterclockwise gesture must increase heading_offset_rad (north goes left); got {}",
            camera.heading_offset_rad
        );
    }

    #[test]
    fn rotation_magnitude_is_preserved() {
        // The magnitude of heading_offset_rad must match the gesture magnitude.
        let delta = 0.3_f32;
        let camera = one_frame_with_gesture(delta);
        assert!(
            (camera.heading_offset_rad.abs() - delta).abs() < 1e-5,
            "rotation magnitude must be preserved; expected {} got {}",
            delta,
            camera.heading_offset_rad.abs()
        );
    }
}
