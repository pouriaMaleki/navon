//! L1 tests — GPS-trail-derived camera heading (spec line 110).
//!
//! Spec source: `docs/ux-specs.md` line 110 (authoritative): "camera rotates
//! so that riding direction is towards top of the screen this overrides the
//! camera of routing. Most important camera behaviour is this. (it needs to
//! determine the direction by last few GPS locations it receives)".
//!
//! Runtime-core already filters the motion vector via `heading_filter_alpha`
//! and exposes the smoothed heading as `MotionState.travel_heading_rad`; the
//! camera consumes it into `last_trusted_travel_heading_rad`. These tests
//! pin the observable contract: after a steady eastward leg the camera's
//! `orientation_rad` points east (≈ π/2), and small lateral GPS jitter does
//! not produce large orientation swings.

use std::time::Duration;

use parity_fixtures::{load_ux_constants, FIXTURE_VIEWPORT};
use runtime_core::RuntimeCore;
use runtime_core::api::{
    CameraMode, CameraOrientationMode, GpsSample, RuntimeConfig, RuntimeInputFrame,
};

const METERS_PER_DEG_LAT: f64 = 111_320.0;
const BASE_LAT_DEG: f64 = 60.17;
const BASE_LON_DEG: f64 = 24.94;

fn offset_fix(east_m: f64, north_m: f64, speed_mps: f32) -> GpsSample {
    let mean_lat_rad = BASE_LAT_DEG * std::f64::consts::PI / 180.0;
    let lat = BASE_LAT_DEG + north_m / METERS_PER_DEG_LAT;
    let lon = BASE_LON_DEG + east_m / (METERS_PER_DEG_LAT * mean_lat_rad.cos());
    GpsSample {
        lat_deg: lat,
        lon_deg: lon,
        speed_mps,
        course_rad: None,
        horizontal_accuracy_m: Some(3.0),
    }
}

fn frame(dt_ms: u64, gps: GpsSample) -> RuntimeInputFrame {
    RuntimeInputFrame::new(Duration::from_millis(dt_ms))
        .with_gps(gps)
        .with_viewport(FIXTURE_VIEWPORT)
}

fn test_config() -> RuntimeConfig {
    let ux = load_ux_constants();
    RuntimeConfig {
        riding_speed_threshold_mps: ux.enter_moving_kph / 3.6,
        stopped_speed_threshold_mps: ux.exit_moving_kph / 3.6,
        // Short heading-acquisition delay so the test sees TravelUpAuto
        // within a handful of frames.
        heading_acquisition_delay: Duration::from_millis(80),
        ..RuntimeConfig::default()
    }
}

fn normalize_delta_rad(actual: f32, expected: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    ((actual - expected + tau * 1.5) % tau) - std::f32::consts::PI
}

#[test]
fn camera_heading_tracks_steady_eastward_motion() {
    // Rider moves 5 m/s east for ~1.5s — plenty of time to acquire heading.
    // Expected: camera.orientation_rad ≈ π/2 (east in atan2(dx, dy) terms).
    let mut runtime = RuntimeCore::new(test_config());
    // Prime: one stationary fix so the classifier has a "before".
    runtime.step(frame(16, offset_fix(0.0, 0.0, 0.0)));
    for i in 1..=40 {
        let east = (i as f64) * 2.5; // 2.5 m per 16 ms → ~156 m/s? Simplify: we
        // only care about direction, not speed. Force speed_mps above the riding
        // threshold so the classifier goes into Moving.
        runtime.step(frame(16, offset_fix(east, 0.0, 5.0)));
    }
    let output = runtime.step(frame(16, offset_fix(40.0 * 2.5, 0.0, 5.0)));
    assert_eq!(output.camera.mode, CameraMode::Riding);
    assert!(
        matches!(
            output.camera.orientation_mode,
            CameraOrientationMode::TravelUpAuto | CameraOrientationMode::HeadingAcquisition
        ),
        "orientation mode should have acquired travel-up (spec 110 most important); got {:?}",
        output.camera.orientation_mode
    );
    let delta = normalize_delta_rad(output.camera.orientation_rad, std::f32::consts::FRAC_PI_2);
    assert!(
        delta.abs() < 0.15,
        "steady east motion should yield orientation ≈ π/2, got orientation_rad={} (delta={} rad)",
        output.camera.orientation_rad,
        delta
    );
}

#[test]
fn camera_heading_smooths_lateral_gps_jitter() {
    // Clearly-east rider with ±1.5 m lateral GPS noise on every step. The
    // smoothed travel heading must stay tight to east, not whiplash between
    // steps. This pins the spec's "should not be jumpy" expectation.
    let mut runtime = RuntimeCore::new(test_config());
    runtime.step(frame(16, offset_fix(0.0, 0.0, 0.0)));
    let mut east = 0.0;
    for i in 1..=40 {
        east += 2.5;
        let noise = if i % 2 == 0 { 1.5 } else { -1.5 };
        runtime.step(frame(16, offset_fix(east, noise, 5.0)));
    }
    let output = runtime.step(frame(16, offset_fix(east, 0.0, 5.0)));
    assert_eq!(output.camera.mode, CameraMode::Riding);
    let delta = normalize_delta_rad(output.camera.orientation_rad, std::f32::consts::FRAC_PI_2);
    assert!(
        delta.abs() < 0.35,
        "with ±1.5m lateral jitter the smoothed heading must stay within 20° of east, got delta={} rad",
        delta
    );
}

