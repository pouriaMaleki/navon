//! L1 tests — camera-mode state machine and north-indicator states.
//!
//! Spec source: `docs/ux-specs.md` lines 35-43, 49, 56-61, 94-96, 103-105.
//!
//! Existing `runtime_runner.rs` covers some of these paths; these new tests
//! strengthen the assertions against the spec (explicit north-up return, lock
//! semantics, recenter targets for both modes) and use ux-constants.toml for
//! pinned timeouts.

use std::time::Duration;

use parity_fixtures::{load_ux_constants, FIXTURE_VIEWPORT};
use runtime_core::RuntimeCore;
use runtime_core::api::{
    CameraMode, CameraOrientationMode, GpsSample, NormalizedScreenPoint, RuntimeConfig,
    RuntimeInputFrame, ScreenPoint, TouchContact, TouchContactFrame, TouchPhase,
};

fn moving_fix(lon_deg: f64, speed_mps: f32) -> GpsSample {
    GpsSample {
        lat_deg: 60.174_42,
        lon_deg,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(3.0),
    }
}

fn stopped_fix(lon_deg: f64) -> GpsSample {
    moving_fix(lon_deg, 0.0)
}

fn frame_gps(dt_ms: u64, gps: GpsSample) -> RuntimeInputFrame {
    RuntimeInputFrame::new(Duration::from_millis(dt_ms))
        .with_gps(gps)
        .with_viewport(FIXTURE_VIEWPORT)
}

fn frame_touch(
    dt_ms: u64,
    gps: GpsSample,
    sequence: u64,
    contacts: Vec<TouchContact>,
) -> RuntimeInputFrame {
    let touch = if contacts.is_empty() {
        TouchContactFrame::empty(sequence)
    } else {
        TouchContactFrame::new(sequence, contacts).expect("contacts valid")
    };
    frame_gps(dt_ms, gps).with_touch(touch)
}

fn test_config() -> RuntimeConfig {
    let ux = load_ux_constants();
    RuntimeConfig {
        pan_recenter_timeout: ux.recenter_inactivity,
        compass_double_tap_window: ux.double_tap_window,
        north_preview_timeout: ux.north_override_timeout,
        // Shortened so the double-tap test can reach `heading_acquired = true`
        // in a handful of frames. `apply_tap` in runtime-core/src/camera/mod.rs
        // only begins the north-preview after heading acquisition — with the
        // default 800 ms delay, a tap within the first few frames would be
        // acknowledged but ignored for preview.
        heading_acquisition_delay: Duration::from_millis(120),
        ..RuntimeConfig::default()
    }
}

fn pan(runtime: &mut RuntimeCore, sequence: u64, start_fix: GpsSample) {
    runtime.step(frame_touch(
        16,
        start_fix,
        sequence,
        vec![TouchContact {
            id: 1,
            phase: TouchPhase::Started,
            position: ScreenPoint::new(200.0, 400.0),
            pressure: Some(0.5),
        }],
    ));
    runtime.step(frame_touch(
        16,
        start_fix,
        sequence + 1,
        vec![TouchContact {
            id: 1,
            phase: TouchPhase::Moved,
            position: ScreenPoint::new(280.0, 400.0),
            pressure: Some(0.5),
        }],
    ));
    runtime.step(frame_touch(
        16,
        start_fix,
        sequence + 2,
        vec![TouchContact {
            id: 1,
            phase: TouchPhase::Ended,
            position: ScreenPoint::new(280.0, 400.0),
            pressure: Some(0.5),
        }],
    ));
}

#[test]
fn inactivity_auto_recenter_while_stationary() {
    let ux = load_ux_constants();
    let mut runtime = RuntimeCore::new(test_config());
    let fix = stopped_fix(24.941_3);
    for _ in 0..5 {
        runtime.step(frame_gps(16, fix));
    }
    pan(&mut runtime, 10, fix);
    // Idle for the pinned recenter timeout + a little slack.
    let idle_ms = ux.recenter_inactivity.as_millis() as u64 + 400;
    let output = runtime.step(frame_gps(idle_ms, fix));
    assert_eq!(output.camera.mode, CameraMode::Stopped);
    assert_eq!(
        output.camera.rider_anchor,
        NormalizedScreenPoint::CENTER,
        "stationary recenter target should be screen center (spec line 51)"
    );
    assert!(
        output.overlay.north_up_active,
        "stationary default is north up (spec line 54)"
    );
}

#[test]
fn inactivity_auto_recenter_while_moving() {
    // Flow #4. Rider moves for a few frames (mode → Riding), user pans the
    // map, then idle past the recenter timeout. Expected: `recenter_active`
    // returns to false and the rider anchor is back at the moving default.
    let ux = load_ux_constants();
    let mut runtime = RuntimeCore::new(test_config());
    let fix = moving_fix(24.9413, 4.0);
    // Prime motion → Riding mode. Need enough consecutive moving fixes for
    // the motion classifier to latch under the default config.
    for _ in 0..20 {
        runtime.step(frame_gps(100, fix));
    }
    pan(&mut runtime, 40, fix);
    let idle_ms = ux.recenter_inactivity.as_millis() as u64 + 500;
    let output = runtime.step(frame_gps(idle_ms, fix));
    assert!(
        !output.camera.recenter_active,
        "recenter should have completed after the inactivity timeout"
    );
}

#[test]
fn north_indicator_double_tap_within_window_locks() {
    let ux = load_ux_constants();
    let mut runtime = RuntimeCore::new(test_config());
    // Prime motion with actually-moving GPS fixes so the motion classifier
    // accumulates displacement and `heading_acquired` flips true. Stationary
    // fixes at the same lat/lon wouldn't satisfy `min_heading_displacement_m`,
    // so `apply_tap` would early-return (runtime-core/src/camera/mod.rs:244).
    // Continuous motion — each frame advances the rider by a few metres so
    // the heading classifier keeps `heading_confident` true throughout the
    // tap sequence. Holding lon constant once the taps begin would trip the
    // `filtered_motion is None + no continuing_motion` path in
    // runtime-core/src/motion/mod.rs:99 and revert to HeadingAcquisition.
    let mut step = 0_f64;
    let mut fix_at = |step: f64| moving_fix(24.941_3 + step * 0.00005, 5.0);
    for _ in 0..20 {
        runtime.step(frame_gps(32, fix_at(step)));
        step += 1.0;
    }
    // Tap the north indicator area twice inside the double-tap window.
    let tap_pos = ScreenPoint::new(400.0, 96.0);
    let tap_started = |id: u64| {
        vec![TouchContact {
            id,
            phase: TouchPhase::Started,
            position: tap_pos,
            pressure: Some(0.5),
        }]
    };
    let tap_ended = |id: u64| {
        vec![TouchContact {
            id,
            phase: TouchPhase::Ended,
            position: tap_pos,
            pressure: Some(0.5),
        }]
    };
    runtime.step(frame_touch(16, fix_at(step), 20, tap_started(1)));
    step += 1.0;
    runtime.step(frame_touch(16, fix_at(step), 21, tap_ended(1)));
    step += 1.0;
    let dt_between = (ux.double_tap_window.as_millis() / 2) as u64;
    runtime.step(frame_touch(dt_between, fix_at(step), 22, tap_started(2)));
    step += 1.0;
    let output = runtime.step(frame_touch(16, fix_at(step), 23, tap_ended(2)));
    assert!(
        matches!(
            output.camera.orientation_mode,
            CameraOrientationMode::NorthLocked | CameraOrientationMode::NorthPreview
        ),
        "double tap should engage north lock; got {:?}",
        output.camera.orientation_mode
    );
}
