use std::time::Duration;

use runtime_core::RuntimeCore;
use runtime_core::api::{
    CameraMode, CameraStateSnapshot, MapQuerySpec, NormalizedScreenPoint, RuntimeConfig,
    RuntimeInputFrame, ScreenPoint, TouchContact, TouchContactFrame, TouchContactFrameError,
    TouchPhase, ViewportSize, WorldPoint,
};
use runtime_core::camera::CameraState;
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
    assert!(output.camera.orientation_rad > 1.4 && output.camera.orientation_rad < 1.7);
    assert_eq!(
        output.camera.rider_anchor,
        runtime.config().riding_rider_anchor
    );
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
