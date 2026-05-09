//! L1 tests — stationary/moving classifier behaviour.
//!
//! Spec source: `docs/ux-specs.md` line 52 ("speed is less than 0.5 kph
//! considered stationary"). The pinned hysteresis pair lives in
//! `parity-fixtures/data/ux-constants.toml`.
//!
//! These tests encode what the spec says, not what runtime-core currently does.
//! Where the current implementation diverges (e.g. uses 0.9 mps ≈ 3.24 kph as
//! the riding threshold), the test fails — that is the red baseline.

use std::time::Duration;

use parity_fixtures::load_ux_constants;
use runtime_core::RuntimeCore;
use runtime_core::api::{
    CameraMode, GpsSample, RuntimeConfig, RuntimeInputFrame, ViewportSize,
};

fn helsinki_fix(speed_mps: f32) -> GpsSample {
    GpsSample {
        lat_deg: 60.174_42,
        lon_deg: 24.941_3,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

fn frame_with(speed_mps: f32, dt: Duration) -> RuntimeInputFrame {
    RuntimeInputFrame::new(dt)
        .with_gps(helsinki_fix(speed_mps))
        .with_viewport(ViewportSize::new(800, 800))
}

fn test_config() -> RuntimeConfig {
    let ux = load_ux_constants();
    RuntimeConfig {
        // Pin thresholds to the values the spec implies via ux-constants.toml.
        // Spec line 52: "speed is less than 0.5 kph considered stationary".
        // 0.5 kph = 0.1389 mps; 0.3 kph = 0.0833 mps.
        riding_speed_threshold_mps: ux.enter_moving_kph / 3.6,
        stopped_speed_threshold_mps: ux.exit_moving_kph / 3.6,
        ..RuntimeConfig::default()
    }
}

#[test]
fn stationary_below_threshold_stays_stopped() {
    let mut runtime = RuntimeCore::new(test_config());
    for _ in 0..10 {
        runtime.step(frame_with(0.05, Duration::from_millis(100)));
    }
    let output = runtime.step(frame_with(0.05, Duration::from_millis(100)));
    assert_eq!(output.camera.mode, CameraMode::Stopped);
    assert!(!output.overlay.speed_panel_visible);
}

#[test]
fn crosses_threshold_enters_riding_mode_at_boundary() {
    // Spec line 52: < 0.5 kph is stationary. At the pinned enter threshold
    // (0.5 kph ≈ 0.139 mps), the runtime must classify the rider as moving.
    // Under `test_config()` the runtime uses the spec value directly.
    let ux = load_ux_constants();
    let mut runtime = RuntimeCore::new(test_config());
    runtime.step(frame_with(0.0, Duration::from_millis(16)));
    // Just above the enter threshold — within 5% so a regression that bumps
    // the threshold by 1 kph would be caught.
    let speed = (ux.enter_moving_kph / 3.6) * 1.05;
    let output = runtime.step(frame_with(speed, Duration::from_millis(16)));
    assert_eq!(
        output.camera.mode,
        CameraMode::Riding,
        "rider at 5% above the 0.5 kph enter threshold must be classified moving"
    );
}

#[test]
fn just_under_exit_threshold_returns_to_stopped_after_dwell() {
    let ux = load_ux_constants();
    let mut runtime = RuntimeCore::new(test_config());
    // Enter riding.
    runtime.step(frame_with(0.0, Duration::from_millis(16)));
    runtime.step(frame_with(2.0, Duration::from_millis(16)));
    assert_eq!(
        runtime.step(frame_with(2.0, Duration::from_millis(16))).camera.mode,
        CameraMode::Riding
    );
    // Drop below exit threshold (0.3 kph = 0.083 mps).
    let low_speed = ux.exit_moving_kph / 3.6 * 0.5;
    for _ in 0..30 {
        runtime.step(frame_with(low_speed, Duration::from_millis(100)));
    }
    let output = runtime.step(frame_with(low_speed, Duration::from_millis(100)));
    assert_eq!(
        output.camera.mode,
        CameraMode::Stopped,
        "sustained low speed should return to stopped (spec: stationary)"
    );
}

#[test]
fn stationary_shows_roughly_500m_radius_viewport() {
    // Spec line 54: stationary mode shows ~500 m around the rider. Translates
    // to a viewport half-width of ~500 m at the rider's latitude. The runtime
    // default zoom is 15.5; at Helsinki (60°N), that covers materially more
    // than 500 m. Expected RED until the stationary camera picks its zoom
    // from a radius target (or the default is tuned).
    use runtime_core::map::meters_per_pixel_for_zoom;
    let ux = load_ux_constants();
    let mut runtime = RuntimeCore::new(test_config());
    for _ in 0..5 {
        runtime.step(frame_with(0.0, Duration::from_millis(100)));
    }
    let output = runtime.step(frame_with(0.0, Duration::from_millis(100)));
    let meters_per_px = meters_per_pixel_for_zoom(output.camera.zoom);
    let viewport_half_px = 400.0_f64; // FIXTURE_VIEWPORT width / 2
    let lat_rad = 60.174_42_f64.to_radians();
    let radius_m = meters_per_px * viewport_half_px * lat_rad.cos();
    let target = ux.stationary_view_radius_m;
    let lower = target * 0.6;
    let upper = target * 1.4;
    assert!(
        radius_m >= lower && radius_m <= upper,
        "stationary viewport radius should be within [{lower}, {upper}] m (got {radius_m:.0} m) — zoom {} covers too much ground",
        output.camera.zoom
    );
}

// ─── Default-config cases ───────────────────────────────────────────────────
// These intentionally use `RuntimeConfig::default()` — the point of the red
// baseline is to catch places where the shipped runtime defaults diverge from
// the spec constants. If these pass today it means either the code matches the
// spec (good) or the test under-constrains (check the assertion).

fn helsinki_moving_fix(lon_deg: f64, speed_mps: f32) -> GpsSample {
    GpsSample {
        lat_deg: 60.174_42,
        lon_deg,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

#[test]
fn spec_rider_at_0_14_mps_should_be_moving_under_default_config() {
    // Spec: 0.5 kph ≈ 0.139 mps is the moving threshold. Runtime default
    // `riding_speed_threshold_mps` is 0.9 (≈ 3.24 kph). This test asserts the
    // spec semantics against the *default* config — expected RED until the
    // default threshold is lowered to match the spec.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(helsinki_moving_fix(24.9413, 0.0))
            .with_viewport(ViewportSize::new(800, 800)),
    );
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(helsinki_moving_fix(24.9414, 0.14))
            .with_viewport(ViewportSize::new(800, 800)),
    );
    assert_eq!(
        output.camera.mode,
        CameraMode::Riding,
        "spec says 0.14 mps (≈0.5 kph) should be moving; runtime default may use a higher threshold"
    );
}

#[test]
fn spec_boundary_just_above_enter_threshold_under_default_config() {
    // 20% above the pinned enter threshold: 0.167 mps ≈ 0.6 kph.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(helsinki_moving_fix(24.9413, 0.0))
            .with_viewport(ViewportSize::new(800, 800)),
    );
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(helsinki_moving_fix(24.9414, 0.167))
            .with_viewport(ViewportSize::new(800, 800)),
    );
    assert_eq!(output.camera.mode, CameraMode::Riding);
}

#[test]
fn spec_boundary_just_below_exit_threshold_under_default_config() {
    // 20% below the pinned exit threshold: 0.067 mps ≈ 0.24 kph.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    for _ in 0..3 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(16))
                .with_gps(helsinki_moving_fix(24.9413, 5.0))
                .with_viewport(ViewportSize::new(800, 800)),
        );
    }
    for _ in 0..30 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(100))
                .with_gps(helsinki_moving_fix(24.9414, 0.067))
                .with_viewport(ViewportSize::new(800, 800)),
        );
    }
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(100))
            .with_gps(helsinki_moving_fix(24.9414, 0.067))
            .with_viewport(ViewportSize::new(800, 800)),
    );
    assert_eq!(output.camera.mode, CameraMode::Stopped);
}
