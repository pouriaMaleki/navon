//! L1 tests — user gestures: pan exploration, pinch+rotate simultaneously.
//!
//! Spec lines 46-48 on ESP: pinch-zoom, two-finger rotate (simultaneous), pan.

use std::time::Duration;

use parity_fixtures::FIXTURE_VIEWPORT;
use runtime_core::RuntimeCore;
use runtime_core::api::{
    GpsSample, RuntimeConfig, RuntimeInputFrame, ScreenPoint, TouchContact, TouchContactFrame,
    TouchPhase,
};

fn moving_fix(speed_mps: f32) -> GpsSample {
    GpsSample {
        lat_deg: 60.17442,
        lon_deg: 24.9413,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

fn frame(dt_ms: u64, sequence: u64, contacts: Vec<TouchContact>) -> RuntimeInputFrame {
    let touch = if contacts.is_empty() {
        TouchContactFrame::empty(sequence)
    } else {
        TouchContactFrame::new(sequence, contacts).expect("valid contacts")
    };
    RuntimeInputFrame::new(Duration::from_millis(dt_ms))
        .with_gps(moving_fix(0.0))
        .with_touch(touch)
        .with_viewport(FIXTURE_VIEWPORT)
}

fn prime_moving(runtime: &mut RuntimeCore) {
    // Drive the rider into Riding mode so pan behaviour matches spec line 49
    // ("after a timeout camera resets" only applies while actually moving).
    for _ in 0..3 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(16))
                .with_gps(moving_fix(6.0))
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }
}

fn pan_recenter_config() -> RuntimeConfig {
    // Match the shorter-timeout config used by runtime_runner.rs so we can
    // assert the pan → idle → recenter sequence in a handful of frames
    // instead of driving a real 1.5 s timer.
    RuntimeConfig {
        pan_recenter_timeout: Duration::from_millis(40),
        recenter_duration: Duration::from_millis(320),
        heading_acquisition_delay: Duration::from_millis(120),
        ..RuntimeConfig::default()
    }
}

#[test]
fn pan_single_contact_sets_recenter_active_after_gesture_and_idle() {
    // Flow #2: after the user pans and goes idle, the camera must enter the
    // "recentering" animation (spec line 49). The flag is `recenter_active`,
    // which the runtime sets once the idle timer exceeds `pan_recenter_timeout`
    // — not during the drag itself. Mirrors `pan_idle_recenters_and_clears_follow_lock`
    // in `runtime_runner.rs`.
    let mut runtime = RuntimeCore::new(pan_recenter_config());
    prime_moving(&mut runtime);
    let pan_fix = moving_fix(6.0);

    // Drag: Started + Moved + Moved (no Ended needed — runtime_runner.rs
    // asserts recenter from this exact 3-frame pattern).
    for (sequence, (phase, x)) in [
        (TouchPhase::Started, 100.0),
        (TouchPhase::Moved, 125.0),
        (TouchPhase::Moved, 150.0),
    ]
    .iter()
    .enumerate()
    {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(16))
                .with_gps(pan_fix)
                .with_touch(
                    TouchContactFrame::new(
                        (sequence + 1) as u64,
                        vec![TouchContact {
                            id: 1,
                            phase: *phase,
                            position: ScreenPoint::new(*x, 100.0),
                            pressure: Some(0.5),
                        }],
                    )
                    .unwrap(),
                )
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }

    // One idle frame (60 ms) past the 40 ms timeout — runtime should be mid-recenter.
    let recentering = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(60))
            .with_gps(pan_fix)
            .with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(
        recentering.camera.recenter_active,
        "idle past pan_recenter_timeout must flip recenter_active so the auto-recenter animation can run (spec line 49)"
    );
    // After the full recenter_duration: back to false.
    let recentered = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(400))
            .with_gps(pan_fix)
            .with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(
        !recentered.camera.recenter_active,
        "recenter_active must clear once the recenter animation completes"
    );
}

#[test]
fn pan_honours_spec_pinned_recenter_inactivity_timeout() {
    // Companion assertion to `pan_single_contact_sets_recenter_active_…`:
    // that test uses a short 40 ms override for speed. This one verifies the
    // runtime actually *respects* its configured timeout — specifically the
    // spec-pinned `recenter_inactivity` from ux-constants.toml — by checking
    // that recenter does NOT fire before the configured duration elapses.
    let ux = parity_fixtures::load_ux_constants();
    let mut runtime = RuntimeCore::new(RuntimeConfig {
        pan_recenter_timeout: ux.recenter_inactivity,
        ..RuntimeConfig::default()
    });
    prime_moving(&mut runtime);
    let pan_fix = moving_fix(6.0);

    // Pan (Started + Moved + Moved), same shape as the happy-path test.
    for (sequence, (phase, x)) in [
        (TouchPhase::Started, 100.0),
        (TouchPhase::Moved, 125.0),
        (TouchPhase::Moved, 150.0),
    ]
    .iter()
    .enumerate()
    {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(16))
                .with_gps(pan_fix)
                .with_touch(
                    TouchContactFrame::new(
                        (sequence + 1) as u64,
                        vec![TouchContact {
                            id: 1,
                            phase: *phase,
                            position: ScreenPoint::new(*x, 100.0),
                            pressure: Some(0.5),
                        }],
                    )
                    .unwrap(),
                )
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }

    // Advance to just below the pinned timeout: recenter must still be pending.
    let just_under = ux.recenter_inactivity.as_millis() as u64 - 50;
    let mid = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(just_under))
            .with_gps(pan_fix)
            .with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(
        !mid.camera.recenter_active,
        "recenter must not fire before the pinned recenter_inactivity ({} ms) elapses",
        ux.recenter_inactivity.as_millis()
    );

    // Cross the timeout boundary with a small step so the recenter animation
    // is still in flight on the same frame we assert. A large `dt` would
    // complete the whole animation in one frame (runtime scales progress by
    // `dt / recenter_duration`, clamped to 1.0) and race the flag back to
    // false before the assertion.
    let observed = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(100))
            .with_gps(pan_fix)
            .with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(
        observed.camera.recenter_active,
        "recenter must fire on the frame that crosses the pinned recenter_inactivity ({} ms)",
        ux.recenter_inactivity.as_millis()
    );
}

#[test]
fn pinch_and_rotate_simultaneously_changes_zoom_and_orientation() {
    // Flow #1. Two contacts moving apart AND rotating should both zoom the
    // camera AND change its orientation in the same tick.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    // Prime.
    runtime.step(frame(16, 1, vec![]));
    runtime.step(frame(16, 2, vec![]));
    let baseline = runtime.step(frame(16, 3, vec![]));

    // Two contacts start close together.
    runtime.step(frame(
        16,
        10,
        vec![
            TouchContact {
                id: 1,
                phase: TouchPhase::Started,
                position: ScreenPoint::new(300.0, 400.0),
                pressure: Some(0.5),
            },
            TouchContact {
                id: 2,
                phase: TouchPhase::Started,
                position: ScreenPoint::new(500.0, 400.0),
                pressure: Some(0.5),
            },
        ],
    ));
    // Move them apart AND rotate (both y-coordinates change asymmetrically).
    runtime.step(frame(
        16,
        11,
        vec![
            TouchContact {
                id: 1,
                phase: TouchPhase::Moved,
                position: ScreenPoint::new(200.0, 320.0),
                pressure: Some(0.5),
            },
            TouchContact {
                id: 2,
                phase: TouchPhase::Moved,
                position: ScreenPoint::new(600.0, 480.0),
                pressure: Some(0.5),
            },
        ],
    ));
    let output = runtime.step(frame(
        16,
        12,
        vec![
            TouchContact {
                id: 1,
                phase: TouchPhase::Moved,
                position: ScreenPoint::new(150.0, 260.0),
                pressure: Some(0.5),
            },
            TouchContact {
                id: 2,
                phase: TouchPhase::Moved,
                position: ScreenPoint::new(650.0, 540.0),
                pressure: Some(0.5),
            },
        ],
    ));

    let zoom_changed = (output.camera.zoom - baseline.camera.zoom).abs() > 0.01;
    let orientation_changed =
        (output.camera.orientation_rad - baseline.camera.orientation_rad).abs() > 0.01;
    assert!(
        zoom_changed && orientation_changed,
        "simultaneous pinch+rotate must change BOTH zoom and orientation in the same gesture \
         (zoom Δ {}, orientation Δ {})",
        (output.camera.zoom - baseline.camera.zoom).abs(),
        (output.camera.orientation_rad - baseline.camera.orientation_rad).abs()
    );
}
