use std::time::Duration;

use runtime_core::RuntimeCore;
use runtime_core::api::{
    CameraMode, CameraStateSnapshot, MapQuerySpec, NormalizedScreenPoint, RuntimeConfig,
    RuntimeInputFrame, ScreenPoint, TouchContact, TouchContactFrame, TouchContactFrameError,
    TouchPhase, ViewportSize, WorldPoint,
};
use runtime_core::camera::CameraState;
use runtime_core::input::staging::DerivedInputState;
use runtime_core::map::meters_per_pixel_for_zoom;
use runtime_core::motion::MotionState;
use runtime_core::output;

#[test]
fn touch_contact_frame_rejects_duplicate_contact_ids() {
    let result = TouchContactFrame::new(
        42,
        vec![
            TouchContact {
                id: 7,
                phase: TouchPhase::Started,
                position: ScreenPoint::new(10.0, 10.0),
                pressure: Some(0.5),
            },
            TouchContact {
                id: 7,
                phase: TouchPhase::Moved,
                position: ScreenPoint::new(12.0, 10.0),
                pressure: Some(0.6),
            },
        ],
    );

    assert_eq!(result, Err(TouchContactFrameError::DuplicateContactId(7)));
}

#[test]
fn default_frame_is_centered_and_north_up() {
    let mut runtime = RuntimeCore::default();
    let output = runtime.step(RuntimeInputFrame::new(Duration::from_millis(16)));

    assert_eq!(output.frame_index, 1);
    assert_eq!(output.camera.mode, CameraMode::Stopped);
    assert_eq!(output.camera.rider_anchor, NormalizedScreenPoint::CENTER);
    assert!(output.overlay.north_up_active);
    assert!(output.overlay.rider_heading_rad.is_none());
}

#[test]
fn moving_gps_sample_switches_camera_to_riding() {
    let mut runtime = RuntimeCore::default();
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(runtime_core::api::GpsSample {
                lat_deg: 37.7749,
                lon_deg: -122.4194,
                speed_mps: 0.2,
                course_rad: Some(0.0),
                horizontal_accuracy_m: Some(3.0),
            })
            .with_viewport(ViewportSize::new(480, 480)),
    );

    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(runtime_core::api::GpsSample {
                lat_deg: 37.7749,
                lon_deg: -122.4182,
                speed_mps: 5.5,
                course_rad: None,
                horizontal_accuracy_m: Some(3.0),
            })
            .with_viewport(ViewportSize::new(480, 480)),
    );

    assert_eq!(output.camera.mode, CameraMode::Riding);
    assert!(output.camera.orientation_rad > 0.05 && output.camera.orientation_rad < 1.7);
    assert!(output.camera.rider_anchor.y > NormalizedScreenPoint::CENTER.y);
    assert!(output.camera.rider_anchor.y < runtime.config().riding_rider_anchor.y);
    assert!(output.camera.center_world.x_m > output.camera.focus_world.x_m);
}

#[test]
fn query_bounds_match_viewport_extent_at_current_zoom() {
    let mut runtime = RuntimeCore::default();
    let viewport = ViewportSize::new(320, 240);
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_viewport(viewport)
            .with_touch(
                TouchContactFrame::new(
                    1,
                    vec![
                        TouchContact {
                            id: 1,
                            phase: TouchPhase::Started,
                            position: ScreenPoint::new(100.0, 100.0),
                            pressure: Some(0.4),
                        },
                        TouchContact {
                            id: 2,
                            phase: TouchPhase::Ended,
                            position: ScreenPoint::new(120.0, 100.0),
                            pressure: None,
                        },
                    ],
                )
                .expect("valid touch frame"),
            ),
    );

    let expected_mpp = meters_per_pixel_for_zoom(output.camera.zoom);
    let width_m = output.map_query.bounds.max.x_m - output.map_query.bounds.min.x_m;
    let height_m = output.map_query.bounds.max.y_m - output.map_query.bounds.min.y_m;
    let diagnostics = output.diagnostics.expect("diagnostics enabled");

    assert!((width_m - (f64::from(viewport.width_px) * expected_mpp)).abs() < 1e-6);
    assert!((height_m - (f64::from(viewport.height_px) * expected_mpp)).abs() < 1e-6);
    assert_eq!(diagnostics.touch_contact_count, 2);
    assert_eq!(diagnostics.active_touch_contacts, 1);
}

#[test]
fn rotated_query_bounds_expand_to_cover_heading_up_view() {
    let viewport = ViewportSize::new(320, 160);
    let camera = CameraStateSnapshot {
        center_world: WorldPoint::new(10.0, 20.0),
        zoom: 14.0,
        orientation_rad: std::f32::consts::FRAC_PI_4,
        ..CameraStateSnapshot::default()
    };

    let query = runtime_core::map::build_query(&camera, viewport);
    let width_m = query.bounds.max.x_m - query.bounds.min.x_m;
    let height_m = query.bounds.max.y_m - query.bounds.min.y_m;
    let axis_aligned_width_m =
        f64::from(viewport.width_px) * meters_per_pixel_for_zoom(camera.zoom);
    let axis_aligned_height_m =
        f64::from(viewport.height_px) * meters_per_pixel_for_zoom(camera.zoom);

    assert!(width_m > axis_aligned_width_m);
    assert!(height_m > axis_aligned_height_m);
}

#[test]
fn camera_preserves_zoom_below_default_when_within_bounds() {
    let config = RuntimeConfig::default();
    let mut camera = CameraState {
        zoom: config.zoom_bounds.min + 0.5,
        ..CameraState::default()
    };

    camera.advance(
        &MotionState::default(),
        &DerivedInputState::default(),
        Duration::ZERO,
        ViewportSize::new(320, 320),
        &config,
    );

    assert_eq!(camera.zoom, config.zoom_bounds.min + 0.5);
}

#[test]
fn riding_north_is_not_reported_as_north_up_mode() {
    let output = output::build_frame_output(
        3,
        CameraStateSnapshot {
            mode: CameraMode::Riding,
            orientation_rad: 0.0,
            ..CameraStateSnapshot::default()
        },
        MapQuerySpec::default(),
        None,
        false,
        Some(0.0),
    );

    assert!(!output.overlay.north_up_active);
    assert_eq!(output.overlay.rider_heading_rad, Some(0.0));
}

#[test]
fn gps_dropout_keeps_motion_state_alive_during_grace_period() {
    let mut runtime = RuntimeCore::default();
    let moving_fix = runtime_core::api::GpsSample {
        lat_deg: 37.7749,
        lon_deg: -122.4194,
        speed_mps: 4.0,
        course_rad: Some(0.5),
        horizontal_accuracy_m: Some(3.0),
    };

    let first = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(moving_fix)
            .with_viewport(ViewportSize::new(480, 480)),
    );
    let second = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(runtime_core::api::GpsSample {
                lon_deg: -122.4189,
                ..moving_fix
            })
            .with_viewport(ViewportSize::new(480, 480)),
    );
    let dropout = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(100))
            .with_viewport(ViewportSize::new(480, 480)),
    );

    assert_eq!(first.camera.mode, CameraMode::Riding);
    assert_eq!(second.camera.mode, CameraMode::Riding);
    assert_eq!(dropout.camera.mode, CameraMode::Riding);
    assert_eq!(
        dropout.overlay.rider_heading_rad,
        second.overlay.rider_heading_rad
    );
}

#[test]
fn gps_dropout_eventually_falls_back_to_stopped_after_timeout() {
    let mut runtime = RuntimeCore::default();
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(runtime_core::api::GpsSample {
                lat_deg: 37.7749,
                lon_deg: -122.4194,
                speed_mps: 4.0,
                course_rad: Some(0.5),
                horizontal_accuracy_m: Some(3.0),
            })
            .with_viewport(ViewportSize::new(480, 480)),
    );
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(runtime_core::api::GpsSample {
                lat_deg: 37.7749,
                lon_deg: -122.4188,
                speed_mps: 4.0,
                course_rad: Some(0.5),
                horizontal_accuracy_m: Some(3.0),
            })
            .with_viewport(ViewportSize::new(480, 480)),
    );

    let stopped = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(1_100))
            .with_viewport(ViewportSize::new(480, 480)),
    );

    assert_eq!(stopped.camera.mode, CameraMode::Stopped);
    assert!(stopped.overlay.rider_heading_rad.is_none());
}

#[test]
fn manual_pan_sets_follow_lock_without_moving_rider_world_position() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = moving_fix(-122.4194, 6.0);
    runtime.step(frame_with_gps(16, moving_fix));

    runtime.step(frame_with_touch(
        16,
        moving_fix,
        1,
        vec![touch(1, TouchPhase::Started, 100.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        moving_fix,
        2,
        vec![touch(1, TouchPhase::Moved, 120.0, 100.0)],
    ));
    let panned = runtime.step(frame_with_touch(
        16,
        moving_fix,
        3,
        vec![touch(1, TouchPhase::Moved, 145.0, 110.0)],
    ));

    assert!(panned.camera.follow_locked);
    assert_ne!(panned.camera.center_world, panned.camera.focus_world);
    assert_eq!(
        panned.camera.focus_world,
        runtime
            .step(frame_with_gps(16, moving_fix))
            .camera
            .focus_world
    );
}

#[test]
fn mode_transition_smooths_rider_anchor_between_stopped_and_riding() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    let riding_start = runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    let riding_settled = runtime.step(frame_with_gps(220, moving_fix(-122.4174, 6.0)));
    let stopped_start = runtime.step(frame_with_gps(16, stopped_fix(-122.4174)));

    assert_eq!(riding_start.camera.mode, CameraMode::Riding);
    assert!(riding_start.camera.rider_anchor.y > NormalizedScreenPoint::CENTER.y);
    assert!(riding_start.camera.rider_anchor.y < interaction_config().riding_rider_anchor.y);
    assert_eq!(
        riding_settled.camera.rider_anchor,
        interaction_config().riding_rider_anchor
    );
    assert!(stopped_start.camera.rider_anchor.y < interaction_config().riding_rider_anchor.y);
    assert!(stopped_start.camera.rider_anchor.y > NormalizedScreenPoint::CENTER.y);
}

#[test]
fn stopped_to_riding_orientation_eases_instead_of_snapping() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    let riding_start = runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    let riding_mid = runtime.step(frame_with_gps(100, moving_fix(-122.4174, 6.0)));
    let riding_settled = runtime.step(frame_with_gps(220, moving_fix(-122.4164, 6.0)));

    assert!(riding_start.camera.orientation_rad > 0.0);
    assert!(riding_start.camera.orientation_rad < std::f32::consts::FRAC_PI_2);
    assert!(riding_mid.camera.orientation_rad > riding_start.camera.orientation_rad);
    assert!((riding_settled.camera.orientation_rad - std::f32::consts::FRAC_PI_2).abs() < 0.05);
}

#[test]
fn pan_idle_recenters_and_clears_follow_lock() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = moving_fix(-122.4194, 6.0);
    runtime.step(frame_with_gps(16, moving_fix));
    runtime.step(frame_with_touch(
        16,
        moving_fix,
        1,
        vec![touch(1, TouchPhase::Started, 100.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        moving_fix,
        2,
        vec![touch(1, TouchPhase::Moved, 125.0, 100.0)],
    ));
    let panned = runtime.step(frame_with_touch(
        16,
        moving_fix,
        3,
        vec![touch(1, TouchPhase::Moved, 150.0, 115.0)],
    ));
    let recentering = runtime.step(frame_with_gps(60, moving_fix));
    let recentered = runtime.step(frame_with_gps(400, moving_fix));

    assert!(panned.camera.follow_locked);
    assert!(recentering.camera.recenter_active);
    assert!(!recentered.camera.follow_locked);
    assert!(!recentered.camera.recenter_active);
}

#[test]
fn pinch_can_zoom_below_default_without_exceeding_bounds() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = moving_fix(-122.4194, 6.0);
    runtime.step(frame_with_gps(16, moving_fix));
    runtime.step(frame_with_touch(
        16,
        moving_fix,
        1,
        vec![
            touch(1, TouchPhase::Started, 100.0, 100.0),
            touch(2, TouchPhase::Started, 200.0, 100.0),
        ],
    ));
    let zoomed = runtime.step(frame_with_touch(
        16,
        moving_fix,
        2,
        vec![
            touch(1, TouchPhase::Moved, 125.0, 100.0),
            touch(2, TouchPhase::Moved, 175.0, 100.0),
        ],
    ));

    assert!(zoomed.camera.zoom < RuntimeConfig::default().zoom_bounds.default);
    assert!(zoomed.camera.zoom >= interaction_config().zoom_bounds.min);
}

#[test]
fn riding_rotate_changes_orientation() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = moving_fix(-122.4194, 6.0);
    let baseline = runtime.step(frame_with_gps(16, moving_fix));
    runtime.step(frame_with_touch(
        16,
        moving_fix,
        1,
        vec![
            touch(1, TouchPhase::Started, 100.0, 100.0),
            touch(2, TouchPhase::Started, 200.0, 100.0),
        ],
    ));
    let rotated = runtime.step(frame_with_touch(
        16,
        moving_fix,
        2,
        vec![
            touch(1, TouchPhase::Moved, 120.0, 80.0),
            touch(2, TouchPhase::Moved, 180.0, 120.0),
        ],
    ));

    assert_ne!(
        baseline.camera.orientation_rad,
        rotated.camera.orientation_rad
    );
}

#[test]
fn north_indicator_tap_enables_then_times_out_override() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let first_fix = moving_fix(-122.4194, 6.0);
    let moving_fix = moving_fix(-122.4184, 6.0);
    let viewport = ViewportSize::new(480, 480);
    runtime.step(frame_with_gps(16, first_fix));
    runtime.step(frame_with_gps(16, moving_fix));
    let center = interaction_config().north_indicator_center;
    let tap_x = center.x * viewport.width_px as f32;
    let tap_y = center.y * viewport.height_px as f32;

    runtime.step(frame_with_touch(
        16,
        moving_fix,
        1,
        vec![touch(1, TouchPhase::Started, tap_x, tap_y)],
    ));
    let override_on = runtime.step(frame_with_touch(16, moving_fix, 2, vec![]));
    let override_off = runtime.step(frame_with_gps(700, moving_fix));

    assert!(override_on.overlay.north_up_active);
    assert_eq!(override_on.camera.orientation_rad, 0.0);
    assert_eq!(
        override_on.overlay.rider_heading_rad,
        Some(std::f32::consts::FRAC_PI_2)
    );
    assert!(!override_off.overlay.north_up_active);
}

#[test]
fn stationary_touch_hold_does_not_start_recenter() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = moving_fix(-122.4194, 6.0);
    runtime.step(frame_with_gps(16, moving_fix));
    runtime.step(frame_with_touch(
        16,
        moving_fix,
        1,
        vec![
            touch(1, TouchPhase::Started, 100.0, 100.0),
            touch(2, TouchPhase::Started, 200.0, 100.0),
        ],
    ));
    let rotated = runtime.step(frame_with_touch(
        16,
        moving_fix,
        2,
        vec![
            touch(1, TouchPhase::Moved, 120.0, 80.0),
            touch(2, TouchPhase::Moved, 180.0, 120.0),
        ],
    ));
    let held = runtime.step(frame_with_touch(
        120,
        moving_fix,
        3,
        vec![
            touch(1, TouchPhase::Stationary, 120.0, 80.0),
            touch(2, TouchPhase::Stationary, 180.0, 120.0),
        ],
    ));

    assert!(held.camera.orientation_rad >= rotated.camera.orientation_rad);
    assert!(!held.camera.recenter_active);
}

#[test]
fn stopped_transition_holds_heading_then_settles_to_north_up() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, moving_fix(-122.4194, 6.0)));
    let riding = runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    let stopped_hold = runtime.step(frame_with_gps(16, stopped_fix(-122.4184)));
    let stopped_settled = runtime.step(frame_with_gps(500, stopped_fix(-122.4184)));

    assert!(riding.camera.orientation_rad > 0.0);
    assert!(stopped_hold.camera.orientation_rad >= riding.camera.orientation_rad);
    assert!(stopped_settled.camera.orientation_rad.abs() < 0.05);
    assert!(stopped_settled.overlay.north_up_active);
}

fn interaction_config() -> RuntimeConfig {
    RuntimeConfig {
        pan_deadzone_px: 6.0,
        pan_recenter_timeout: Duration::from_millis(40),
        recenter_duration: Duration::from_millis(320),
        mode_transition_duration: Duration::from_millis(180),
        north_up_override_timeout: Duration::from_millis(600),
        stopped_north_up_delay: Duration::from_millis(120),
        stopped_north_up_settle_duration: Duration::from_millis(240),
        ..RuntimeConfig::default()
    }
}

fn moving_fix(lon_deg: f64, speed_mps: f32) -> runtime_core::api::GpsSample {
    runtime_core::api::GpsSample {
        lat_deg: 37.7749,
        lon_deg,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(3.0),
    }
}

fn stopped_fix(lon_deg: f64) -> runtime_core::api::GpsSample {
    runtime_core::api::GpsSample {
        lat_deg: 37.7749,
        lon_deg,
        speed_mps: 0.0,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(3.0),
    }
}

fn frame_with_gps(dt_ms: u64, gps: runtime_core::api::GpsSample) -> RuntimeInputFrame {
    RuntimeInputFrame::new(Duration::from_millis(dt_ms))
        .with_gps(gps)
        .with_viewport(ViewportSize::new(480, 480))
}

fn frame_with_touch(
    dt_ms: u64,
    gps: runtime_core::api::GpsSample,
    sequence: u64,
    contacts: Vec<TouchContact>,
) -> RuntimeInputFrame {
    let frame = if contacts.is_empty() {
        TouchContactFrame::empty(sequence)
    } else {
        TouchContactFrame::new(sequence, contacts).expect("valid touch frame")
    };
    RuntimeInputFrame::new(Duration::from_millis(dt_ms))
        .with_gps(gps)
        .with_touch(frame)
        .with_viewport(ViewportSize::new(480, 480))
}

fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> TouchContact {
    TouchContact {
        id,
        phase,
        position: ScreenPoint::new(x, y),
        pressure: None,
    }
}
