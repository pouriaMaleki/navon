//! L2 rider-lifecycle replay — drives the runtime with the full helsinki-gravel
//! fixture and captures key transitions. Produces a snapshot of per-platform
//! "golden transitions" (see plan flow #64).
//!
//! This is a heavy test; CI runs it on the slow lane (nightly/main).

use std::time::Duration;

use parity_fixtures::{load_gps_stream, FIXTURE_VIEWPORT};
use runtime_core::RuntimeCore;
use runtime_core::api::{CameraMode, RuntimeConfig, RuntimeInputFrame};

#[test]
fn full_ride_playback_helsinki_gravel_records_mode_transitions() {
    let scenario = load_gps_stream("helsinki-gravel");
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    let mut transitions: Vec<(usize, CameraMode)> = Vec::new();
    let mut last_mode: Option<CameraMode> = None;

    for (index, ride_sample) in scenario.samples.iter().enumerate() {
        let dt = if index == 0 {
            Duration::from_millis(16)
        } else {
            let prev_ms = scenario.samples[index - 1].time_offset_ms;
            Duration::from_millis((ride_sample.time_offset_ms - prev_ms).max(1) as u64)
        };
        let frame = RuntimeInputFrame::new(dt)
            .with_gps(ride_sample.sample)
            .with_viewport(FIXTURE_VIEWPORT);
        let output = runtime.step(frame);

        if last_mode != Some(output.camera.mode) {
            transitions.push((index, output.camera.mode));
            last_mode = Some(output.camera.mode);
        }
    }

    // Sanity — a 44-minute Helsinki ride must cross the stationary/moving
    // boundary at least once.
    assert!(
        transitions.len() >= 2,
        "expected ride to cross stationary/moving at least once, got {transitions:?}"
    );
    // The first mode must be Stopped (rider starts stationary).
    assert_eq!(transitions[0].1, CameraMode::Stopped);
    // At least one transition into Riding.
    assert!(
        transitions.iter().any(|(_, mode)| *mode == CameraMode::Riding),
        "ride never entered Riding mode; fixture speed may be below runtime threshold"
    );
}
