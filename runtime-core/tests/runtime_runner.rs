use std::time::Duration;

use runtime_core::RuntimeCore;
use runtime_core::api::{
    CameraMode, CameraOrientationMode, CameraStateSnapshot, MapQuerySpec, NormalizedScreenPoint,
    RuntimeConfig, RuntimeInputFrame, ScreenPoint, SpeedUnit, TouchContact, TouchContactFrame,
    TouchContactFrameError, TouchPhase, ViewportSize, WorldPoint,
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
    assert_eq!(
        output.camera.orientation_mode,
        CameraOrientationMode::StoppedNorthUp
    );
    assert_eq!(output.camera.rider_anchor, NormalizedScreenPoint::CENTER);
    assert!(output.overlay.north_up_active);
    assert!(output.overlay.rider_heading_rad.is_none());
    assert!(!output.overlay.speed_panel_visible);
    assert_eq!(output.overlay.speed_unit, SpeedUnit::Kph);
}

#[test]
fn moving_gps_sample_enters_heading_acquisition_before_travel_up() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));

    let output = runtime.step(frame_with_gps(16, moving_fix(-122.4184, 5.5)));

    assert_eq!(output.camera.mode, CameraMode::Riding);
    assert_eq!(
        output.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert_eq!(output.camera.rider_anchor, NormalizedScreenPoint::CENTER);
    assert!(output.overlay.north_up_active);
    assert!(output.camera.orientation_rad.abs() < 0.001);
    assert!(output.overlay.speed_panel_visible);
    assert_eq!(output.overlay.speed_display_value, 20);
    assert_eq!(output.overlay.speed_unit, SpeedUnit::Kph);
}

#[test]
fn speed_panel_tap_toggles_units_while_moving() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving = prime_travel_up(&mut runtime);
    let initial = runtime.step(frame_with_gps(16, moving));
    let toggled_once = speed_panel_tap(&mut runtime, moving, 20);
    let toggled_twice = speed_panel_tap(&mut runtime, moving, 22);

    assert_eq!(initial.overlay.speed_unit, SpeedUnit::Kph);
    assert_eq!(initial.overlay.speed_display_value, 22);
    assert_eq!(toggled_once.overlay.speed_unit, SpeedUnit::Mph);
    assert_eq!(toggled_once.overlay.speed_display_value, 13);
    assert_eq!(toggled_twice.overlay.speed_unit, SpeedUnit::Kph);
}

#[test]
fn speed_panel_tap_does_not_change_compass_mode() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving = prime_travel_up(&mut runtime);
    let toggled = speed_panel_tap(&mut runtime, moving, 30);

    assert!(toggled.overlay.speed_panel_visible);
    assert_eq!(toggled.overlay.speed_unit, SpeedUnit::Mph);
    assert!(!matches!(
        toggled.camera.orientation_mode,
        CameraOrientationMode::NorthPreview | CameraOrientationMode::NorthLocked
    ));
    assert!(toggled.overlay.north_preview_progress.is_none());
}

#[test]
fn travel_up_auto_activates_after_heading_delay() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    let activating = runtime.step(frame_with_gps(140, moving_fix(-122.4174, 6.0)));
    let settled = runtime.step(frame_with_gps(220, moving_fix(-122.4164, 6.0)));

    assert_eq!(
        activating.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert!(activating.camera.rider_anchor.y > NormalizedScreenPoint::CENTER.y);
    assert!(activating.camera.rider_anchor.y < interaction_config().riding_rider_anchor.y);
    assert!(activating.camera.orientation_rad > 0.0);
    assert!(activating.camera.orientation_rad < std::f32::consts::FRAC_PI_2);

    assert_eq!(
        settled.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert_eq!(
        settled.camera.rider_anchor,
        interaction_config().riding_rider_anchor
    );
    assert!((settled.camera.orientation_rad - std::f32::consts::FRAC_PI_2).abs() < 0.05);
    assert!(!settled.overlay.north_up_active);
}

#[test]
fn small_continuous_gps_steps_eventually_activate_travel_up() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));

    let mut output = runtime.step(frame_with_gps(16, moving_fix(-122.4193988, 6.0)));
    for index in 1..80 {
        let lon = -122.4193988 + (index as f64 * 0.0000012);
        output = runtime.step(frame_with_gps(16, moving_fix(lon, 6.0)));
        if output.camera.orientation_mode == CameraOrientationMode::TravelUpAuto {
            break;
        }
    }
    let settled = runtime.step(frame_with_gps(
        220,
        moving_fix(-122.4193988 + (81.0 * 0.0000012), 6.0),
    ));

    assert_eq!(output.camera.mode, CameraMode::Riding);
    assert_eq!(
        output.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert!(output.camera.rider_anchor.y > NormalizedScreenPoint::CENTER.y);
    assert_eq!(
        settled.camera.rider_anchor,
        interaction_config().riding_rider_anchor
    );
    assert!(settled.camera.orientation_rad > 0.0);
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
            orientation_mode: CameraOrientationMode::TravelUpAuto,
            orientation_rad: 0.0,
            ..CameraStateSnapshot::default()
        },
        MapQuerySpec::default(),
        None,
        Some(0.0),
        &MotionState {
            is_moving: true,
            speed_mps: 4.0,
            ..MotionState::default()
        },
        &runtime_core::overlay_ui::OverlayUiState::new(SpeedUnit::Kph),
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
    let moving_fix = prime_travel_up(&mut runtime);

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
fn pan_idle_recenters_and_clears_follow_lock() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = prime_travel_up(&mut runtime);
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
fn stopped_pan_idle_does_not_recenter() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        1,
        vec![touch(1, TouchPhase::Started, 100.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        2,
        vec![touch(1, TouchPhase::Moved, 125.0, 100.0)],
    ));
    let panned = runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        3,
        vec![touch(1, TouchPhase::Moved, 150.0, 115.0)],
    ));
    let held = runtime.step(frame_with_gps(400, stopped_fix(-122.4194)));

    assert!(panned.camera.follow_locked);
    assert_ne!(panned.camera.center_world, panned.camera.focus_world);
    assert!(held.camera.follow_locked);
    assert!(!held.camera.recenter_active);
    assert_ne!(held.camera.center_world, held.camera.focus_world);
}

#[test]
fn stopped_pan_recenters_from_north_indicator_tap() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        1,
        vec![touch(1, TouchPhase::Started, 100.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        2,
        vec![touch(1, TouchPhase::Moved, 125.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        3,
        vec![touch(1, TouchPhase::Moved, 150.0, 115.0)],
    ));
    runtime.step(frame_with_touch(16, stopped_fix(-122.4194), 4, vec![]));
    let tapped = compass_tap(
        &mut runtime,
        stopped_fix(-122.4194),
        stopped_fix(-122.4194),
        10,
    );
    let recentered = runtime.step(frame_with_gps(400, stopped_fix(-122.4194)));

    assert_eq!(
        tapped.camera.orientation_mode,
        CameraOrientationMode::StoppedNorthUp
    );
    assert!(!recentered.camera.follow_locked);
    assert!(!recentered.camera.recenter_active);
    assert_eq!(
        recentered.camera.center_world,
        recentered.camera.focus_world
    );
}

#[test]
fn stopped_pan_resumes_follow_when_riding_restarts() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        1,
        vec![touch(1, TouchPhase::Started, 100.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        2,
        vec![touch(1, TouchPhase::Moved, 125.0, 100.0)],
    ));
    runtime.step(frame_with_touch(
        16,
        stopped_fix(-122.4194),
        3,
        vec![touch(1, TouchPhase::Moved, 150.0, 115.0)],
    ));
    runtime.step(frame_with_touch(16, stopped_fix(-122.4194), 4, vec![]));
    runtime.step(frame_with_gps(400, stopped_fix(-122.4194)));
    let resumed = runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    let recentered = runtime.step(frame_with_gps(400, moving_fix(-122.4174, 6.0)));

    assert_eq!(resumed.camera.mode, CameraMode::Riding);
    assert_ne!(resumed.camera.center_world, resumed.camera.focus_world);
    assert!(!recentered.camera.follow_locked);
}

#[test]
fn pinch_can_zoom_below_default_without_exceeding_bounds() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = prime_travel_up(&mut runtime);
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
fn rotate_is_ignored_during_heading_acquisition() {
    let mut runtime = RuntimeCore::new(interaction_config());
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    let baseline = runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4184, 6.0),
        1,
        vec![
            touch(1, TouchPhase::Started, 100.0, 100.0),
            touch(2, TouchPhase::Started, 200.0, 100.0),
        ],
    ));
    let rotated = runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4184, 6.0),
        2,
        vec![
            touch(1, TouchPhase::Moved, 120.0, 80.0),
            touch(2, TouchPhase::Moved, 180.0, 120.0),
        ],
    ));

    assert_eq!(
        rotated.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert!((rotated.camera.orientation_rad - baseline.camera.orientation_rad).abs() < 0.001);
}

#[test]
fn riding_rotate_changes_orientation_in_travel_up_auto() {
    let mut runtime = RuntimeCore::new(interaction_config());
    prime_travel_up(&mut runtime);
    let baseline = runtime.step(frame_with_gps(16, moving_fix(-122.4154, 6.0)));
    runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4144, 6.0),
        1,
        vec![
            touch(1, TouchPhase::Started, 100.0, 100.0),
            touch(2, TouchPhase::Started, 200.0, 100.0),
        ],
    ));
    let rotated = runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4134, 6.0),
        2,
        vec![
            touch(1, TouchPhase::Moved, 120.0, 80.0),
            touch(2, TouchPhase::Moved, 180.0, 120.0),
        ],
    ));

    assert_eq!(
        rotated.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert_ne!(
        baseline.camera.orientation_rad,
        rotated.camera.orientation_rad
    );
}

#[test]
fn heading_confidence_dip_returns_to_acquisition_and_holds_last_trusted_angle() {
    let mut runtime = RuntimeCore::new(interaction_config());
    prime_travel_up(&mut runtime);
    let baseline = runtime.step(frame_with_gps(16, moving_fix(-122.4154, 6.0)));
    let dipped = runtime.step(frame_with_gps(16, moving_fix(-122.4154, 6.0)));

    assert_eq!(
        baseline.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert_eq!(dipped.camera.mode, CameraMode::Riding);
    assert_eq!(
        dipped.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert!((dipped.camera.orientation_rad - baseline.camera.orientation_rad).abs() < 0.001);
    assert!(dipped.camera.rider_anchor.y < interaction_config().riding_rider_anchor.y);
    assert!(dipped.overlay.north_up_active);
}

#[test]
fn north_indicator_single_tap_enters_preview_then_returns_to_auto() {
    let mut runtime = RuntimeCore::new(interaction_config());
    prime_travel_up(&mut runtime);
    let preview_on = compass_tap(
        &mut runtime,
        moving_fix(-122.4154, 6.0),
        moving_fix(-122.4144, 6.0),
        1,
    );
    let preview_holding = runtime.step(frame_with_gps(580, moving_fix(-122.4134, 6.0)));
    let returned = runtime.step(frame_with_gps(32, moving_fix(-122.4124, 6.0)));

    assert_eq!(
        preview_on.camera.orientation_mode,
        CameraOrientationMode::NorthPreview
    );
    assert!(preview_on.overlay.north_up_active);
    assert!(preview_on.camera.rider_anchor.y < interaction_config().riding_rider_anchor.y);
    assert!(preview_on.camera.rider_anchor.y > NormalizedScreenPoint::CENTER.y);
    assert!(preview_on.camera.orientation_rad < std::f32::consts::FRAC_PI_2);
    assert!(preview_on.camera.orientation_rad > 0.0);
    assert_eq!(
        preview_holding.camera.orientation_mode,
        CameraOrientationMode::NorthPreview
    );
    assert_eq!(preview_holding.camera.orientation_rad, 0.0);
    assert_eq!(
        returned.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert!(!returned.overlay.north_up_active);
}

#[test]
fn north_indicator_tap_acknowledges_without_mode_change_when_already_north_up() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let stopped_ack = compass_tap(
        &mut runtime,
        stopped_fix(-122.4194),
        stopped_fix(-122.4194),
        1,
    );

    runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    let acquisition_ack = compass_tap(
        &mut runtime,
        moving_fix(-122.4179, 6.0),
        moving_fix(-122.4174, 6.0),
        3,
    );

    assert_eq!(
        stopped_ack.camera.orientation_mode,
        CameraOrientationMode::StoppedNorthUp
    );
    assert!(stopped_ack.overlay.compass_ack_progress > 0.0);
    assert!(!stopped_ack.camera.recenter_active);
    assert_eq!(
        acquisition_ack.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert!(acquisition_ack.overlay.compass_ack_progress > 0.0);
    assert!(acquisition_ack.overlay.north_preview_progress.is_none());
}

#[test]
fn north_indicator_double_tap_locks_until_unlocked() {
    let mut runtime = RuntimeCore::new(interaction_config());
    prime_travel_up(&mut runtime);
    let preview = compass_tap(
        &mut runtime,
        moving_fix(-122.4154, 6.0),
        moving_fix(-122.4144, 6.0),
        1,
    );
    runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4134, 6.0),
        3,
        vec![compass_touch(TouchPhase::Started)],
    ));
    let locked = runtime.step(frame_with_touch(16, moving_fix(-122.4124, 6.0), 4, vec![]));
    let still_locked = runtime.step(frame_with_gps(700, moving_fix(-122.4114, 6.0)));
    runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4104, 6.0),
        5,
        vec![compass_touch(TouchPhase::Started)],
    ));
    let unlocked = runtime.step(frame_with_touch(16, moving_fix(-122.4094, 6.0), 6, vec![]));

    assert_eq!(
        preview.camera.orientation_mode,
        CameraOrientationMode::NorthPreview
    );
    assert_eq!(
        locked.camera.orientation_mode,
        CameraOrientationMode::NorthLocked
    );
    assert!(locked.overlay.north_up_active);
    assert_eq!(
        still_locked.camera.orientation_mode,
        CameraOrientationMode::NorthLocked
    );
    assert_eq!(
        unlocked.camera.orientation_mode,
        CameraOrientationMode::TravelUpAuto
    );
    assert!(!unlocked.overlay.north_up_active);
}

#[test]
fn unlocking_north_lock_with_weak_heading_returns_to_acquisition() {
    let mut runtime = RuntimeCore::new(interaction_config());
    prime_travel_up(&mut runtime);
    compass_tap(
        &mut runtime,
        moving_fix(-122.4154, 6.0),
        moving_fix(-122.4144, 6.0),
        1,
    );
    runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4134, 6.0),
        3,
        vec![compass_touch(TouchPhase::Started)],
    ));
    let locked = runtime.step(frame_with_touch(16, moving_fix(-122.4124, 6.0), 4, vec![]));
    runtime.step(frame_with_touch(
        16,
        moving_fix(-122.4124, 6.0),
        5,
        vec![compass_touch(TouchPhase::Started)],
    ));
    let unlocked = runtime.step(frame_with_touch(16, moving_fix(-122.4124, 6.0), 6, vec![]));
    let held = runtime.step(frame_with_gps(220, moving_fix(-122.4124, 6.0)));

    assert_eq!(
        locked.camera.orientation_mode,
        CameraOrientationMode::NorthLocked
    );
    assert_eq!(
        unlocked.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert!(unlocked.camera.orientation_rad > 0.0);
    assert_eq!(
        held.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert!((held.camera.orientation_rad - std::f32::consts::FRAC_PI_2).abs() < 0.05);
}

#[test]
fn stationary_touch_hold_does_not_start_recenter() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let moving_fix = prime_travel_up(&mut runtime);
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
fn restart_after_stop_reenters_heading_acquisition_north_up() {
    let mut runtime = RuntimeCore::new(interaction_config());
    settle_travel_up(&mut runtime);
    runtime.step(frame_with_gps(16, stopped_fix(-122.4164)));
    let stopped = runtime.step(frame_with_gps(500, stopped_fix(-122.4164)));
    let restarted = runtime.step(frame_with_gps(16, moving_fix(-122.4154, 6.0)));

    assert_eq!(
        stopped.camera.orientation_mode,
        CameraOrientationMode::StoppedNorthUp
    );
    assert_eq!(
        restarted.camera.orientation_mode,
        CameraOrientationMode::HeadingAcquisition
    );
    assert!(restarted.camera.orientation_rad.abs() < 0.05);
    assert_eq!(restarted.camera.rider_anchor, NormalizedScreenPoint::CENTER);
}

#[test]
fn stopped_transition_holds_heading_then_settles_to_north_up() {
    let mut runtime = RuntimeCore::new(interaction_config());
    let riding = settle_travel_up(&mut runtime);
    let stopped_hold = runtime.step(frame_with_gps(16, stopped_fix(-122.4164)));
    let stopped_settled = runtime.step(frame_with_gps(500, stopped_fix(-122.4164)));

    assert!(riding.camera.orientation_rad > 0.0);
    assert!(stopped_hold.camera.orientation_rad >= riding.camera.orientation_rad);
    assert!(stopped_settled.camera.orientation_rad.abs() < 0.05);
    assert!(stopped_settled.overlay.north_up_active);
    assert_eq!(
        stopped_settled.camera.orientation_mode,
        CameraOrientationMode::StoppedNorthUp
    );
}

fn interaction_config() -> RuntimeConfig {
    RuntimeConfig {
        pan_deadzone_px: 6.0,
        pan_recenter_timeout: Duration::from_millis(40),
        recenter_duration: Duration::from_millis(320),
        mode_transition_duration: Duration::from_millis(180),
        heading_acquisition_delay: Duration::from_millis(120),
        north_preview_timeout: Duration::from_millis(600),
        compass_double_tap_window: Duration::from_millis(240),
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

fn compass_touch(phase: TouchPhase) -> TouchContact {
    let center = interaction_config().north_indicator_center;
    touch(1, phase, center.x * 480.0, center.y * 480.0)
}

fn speed_panel_touch(phase: TouchPhase) -> TouchContact {
    touch(1, phase, 240.0, 430.0)
}

fn compass_tap(
    runtime: &mut RuntimeCore,
    started_gps: runtime_core::api::GpsSample,
    released_gps: runtime_core::api::GpsSample,
    sequence: u64,
) -> runtime_core::api::RuntimeFrameOutput {
    runtime.step(frame_with_touch(
        16,
        started_gps,
        sequence,
        vec![compass_touch(TouchPhase::Started)],
    ));
    runtime.step(frame_with_touch(16, released_gps, sequence + 1, vec![]))
}

fn speed_panel_tap(
    runtime: &mut RuntimeCore,
    gps: runtime_core::api::GpsSample,
    sequence: u64,
) -> runtime_core::api::RuntimeFrameOutput {
    let released_gps = runtime_core::api::GpsSample {
        lon_deg: gps.lon_deg + 0.001,
        ..gps
    };
    runtime.step(frame_with_touch(
        16,
        gps,
        sequence,
        vec![speed_panel_touch(TouchPhase::Started)],
    ));
    runtime.step(frame_with_touch(
        16,
        released_gps,
        sequence + 1,
        vec![speed_panel_touch(TouchPhase::Ended)],
    ))
}

fn prime_travel_up(runtime: &mut RuntimeCore) -> runtime_core::api::GpsSample {
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    runtime.step(frame_with_gps(140, moving_fix(-122.4174, 6.0)));
    let settled_fix = moving_fix(-122.4164, 6.0);
    runtime.step(frame_with_gps(220, settled_fix));
    settled_fix
}

fn settle_travel_up(runtime: &mut RuntimeCore) -> runtime_core::api::RuntimeFrameOutput {
    runtime.step(frame_with_gps(16, stopped_fix(-122.4194)));
    runtime.step(frame_with_gps(16, moving_fix(-122.4184, 6.0)));
    runtime.step(frame_with_gps(140, moving_fix(-122.4174, 6.0)));
    runtime.step(frame_with_gps(220, moving_fix(-122.4164, 6.0)))
}
