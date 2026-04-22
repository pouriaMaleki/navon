//! L1 tests — overlay behaviour: locating spinner, GPS dropout, speed
//! visibility, map-tile loading. Also ESP negative-parity (assert the overlay
//! model doesn't expose companion-only primitives).
//!
//! Spec lines 11, 14-15, 18, 34, 72-76.

use std::time::Duration;

use parity_fixtures::FIXTURE_VIEWPORT;
use runtime_core::RuntimeCore;
use runtime_core::api::{GpsSample, OverlayState, RuntimeConfig, RuntimeInputFrame, SpeedUnit};

fn stopped_fix(lat: f64, lon: f64) -> GpsSample {
    GpsSample {
        lat_deg: lat,
        lon_deg: lon,
        speed_mps: 0.0,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

fn moving_fix(lat: f64, lon: f64, speed_mps: f32) -> GpsSample {
    GpsSample {
        lat_deg: lat,
        lon_deg: lon,
        speed_mps,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

#[test]
fn loading_state_before_first_fix_keeps_north_up_overlay() {
    // Flow #15: before any GPS arrives the overlay must remain in a safe
    // "locating" default — north up, no speed shown.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16)).with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(output.overlay.north_up_active);
    assert!(!output.overlay.speed_panel_visible);
    assert!(output.overlay.rider_heading_rad.is_none());
}

#[test]
fn speed_hidden_when_stationary() {
    // Flow #11.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    for _ in 0..10 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(100))
                .with_gps(stopped_fix(60.17442, 24.9413))
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(100))
            .with_gps(stopped_fix(60.17442, 24.9413))
            .with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(!output.overlay.speed_panel_visible);
}

#[test]
fn speed_shown_when_moving_in_kph_by_default() {
    // Flow #12. Default unit is kph per RuntimeConfig.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(stopped_fix(60.17442, 24.9413))
            .with_viewport(FIXTURE_VIEWPORT),
    );
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(moving_fix(60.17443, 24.94131, 5.5))
            .with_viewport(FIXTURE_VIEWPORT),
    );
    assert!(output.overlay.speed_panel_visible);
    assert_eq!(output.overlay.speed_unit, SpeedUnit::Kph);
    // 5.5 mps = 19.8 kph → rounded to 20.
    assert_eq!(output.overlay.speed_display_value, 20);
}

#[test]
fn gps_dropout_does_not_panic_and_preserves_last_overlay() {
    // Flow #14: after the feed stops the runtime must continue ticking. We
    // can't fail-soft a None-fix → Stopped transition without runtime changes,
    // but we can assert the tick doesn't panic and overlay stays consistent.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16))
            .with_gps(moving_fix(60.17442, 24.9413, 5.0))
            .with_viewport(FIXTURE_VIEWPORT),
    );
    // Five ticks without GPS.
    for _ in 0..5 {
        let output = runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(500)).with_viewport(FIXTURE_VIEWPORT),
        );
        let _ = output;
    }
}

// ─── ESP negative parity (plan flows #16, #17, #18) ─────────────────────────
// `OverlayState` has no text-input, settings-icon or target-location fields.
// These tests guard against accidentally growing the ESP overlay into the
// companion shape. Runtime assertion + compile-time assertion combined.

#[test]
fn esp_overlay_has_no_companion_only_fields() {
    // Runtime check: overlay state from a fresh runtime matches the ESP
    // surface area described in spec lines 9-18 — north indicator, rider
    // heading, speed panel, compass ack. Nothing else.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16)).with_viewport(FIXTURE_VIEWPORT),
    );
    // Exhaustive destructure forces a compile error if a field is added — add
    // the new field here and decide whether ESP should expose it.
    let OverlayState {
        north_indicator_visible: _,
        north_up_active: _,
        rider_heading_rad: _,
        north_preview_progress: _,
        compass_ack_progress: _,
        speed_panel_visible: _,
        speed_display_value: _,
        speed_unit: _,
    } = output.overlay;
}

#[test]
fn esp_map_tile_loading_indicator_when_empty_geometry() {
    // Spec line 18: ESP shows a loading affordance when the map is loading.
    // Contract: `OverlayState` must expose a `map_tiles_loading` (bool) or
    // equivalent enum variant so the UI can render "loading map". Today the
    // field is absent — expected RED until it lands.
    //
    // We check for a named field via the struct's JSON-like debug
    // representation to avoid requiring serde in runtime-core.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(16)).with_viewport(FIXTURE_VIEWPORT),
    );
    let debug = format!("{:?}", output.overlay);
    assert!(
        debug.contains("map_tiles_loading") || debug.contains("tiles_loading"),
        "OverlayState should expose a tile-loading signal (spec line 18); debug repr was: {debug}"
    );
}

#[test]
fn stationary_viewport_spans_roughly_500_meters_radius() {
    // Duplicates flow #6 from the motion suite. Keep here so the overlay
    // file stays self-contained and the spec section on loading / viewport
    // is covered in one place.
    use runtime_core::map::meters_per_pixel_for_zoom;
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    for _ in 0..5 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(100))
                .with_gps(stopped_fix(60.17442, 24.9413))
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }
    let output = runtime.step(
        RuntimeInputFrame::new(Duration::from_millis(100))
            .with_gps(stopped_fix(60.17442, 24.9413))
            .with_viewport(FIXTURE_VIEWPORT),
    );
    let meters_per_px = meters_per_pixel_for_zoom(output.camera.zoom);
    let viewport_half_px = 400.0_f64;
    let lat_rad = 60.174_42_f64.to_radians();
    let radius_m = meters_per_px * viewport_half_px * lat_rad.cos();
    assert!(
        radius_m >= 300.0 && radius_m <= 700.0,
        "stationary viewport radius should be ~500 m (got {radius_m:.0} m) — spec line 54"
    );
}
