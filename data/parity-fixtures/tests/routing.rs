//! L2 tests — route projection, off-route detection, rerouting signal,
//! next-turn countdown. Drives the runtime with a baked `RouteSetMessage`
//! plus the helsinki-gravel fixture as the GPS stream.
//!
//! Spec source: `docs/ux-specs.md` lines 41, 82-84 and plan flows
//! `next_turn_shown_when_routing` (#13), `next_turn_distance_trend_is_decreasing`
//! (#56), `off_route_detection_reroute_trigger` (#61).

use std::time::Duration;

use parity_fixtures::{load_gps_stream, FIXTURE_VIEWPORT};
use runtime_core::RuntimeCore;
use runtime_core::api::{
    CURRENT_ROUTE_PACKAGE_VERSION, GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage,
    RouteProvenance, RouteProvider, RouteSetMessage, RouteSummary, RouteSyncMessage,
    RuntimeConfig, RuntimeInputFrame,
};

// Two distinct Helsinki landmarks ~1.8 km apart. Using the fixture endpoints
// directly is unreliable because the sample ride is a loop — first and last
// points can collapse to within metres of each other.
fn route_start() -> GeoPoint {
    GeoPoint::new(60.1699, 24.9328) // Kamppi / Helsinki Central
}

fn route_end() -> GeoPoint {
    GeoPoint::new(60.1836, 24.9253) // Töölö / Mannerheimintie
}

fn haversine_m(a: &GeoPoint, b: &GeoPoint) -> f64 {
    let r = 6_378_137.0_f64;
    let to_rad = std::f64::consts::PI / 180.0;
    let d_lat = (b.lat_deg - a.lat_deg) * to_rad;
    let d_lon = (b.lon_deg - a.lon_deg) * to_rad;
    let a0 = (d_lat / 2.0).sin().powi(2)
        + (a.lat_deg * to_rad).cos()
            * (b.lat_deg * to_rad).cos()
            * (d_lon / 2.0).sin().powi(2);
    2.0 * r * a0.sqrt().atan2((1.0 - a0).sqrt())
}

/// L-shaped route with a Left turn in the middle. Required for tests of
/// `upcoming_turn_alert` — `canonical_turn_alert_kind` in runtime-core returns
/// `None` for Depart/Arrive, so any alert test needs a non-terminal maneuver.
fn baked_route_with_left_turn() -> RoutePackage {
    let start = GeoPoint::new(60.1699, 24.9328);
    let corner = GeoPoint::new(60.1750, 24.9328); // ~570 m due north
    let end = GeoPoint::new(60.1750, 24.9200);    // ~710 m due west of corner
    let seg1 = haversine_m(&start, &corner) as f32;
    let seg2 = haversine_m(&corner, &end) as f32;
    let total = seg1 + seg2;
    RoutePackage {
        version: CURRENT_ROUTE_PACKAGE_VERSION,
        route_id: "helsinki-l-shape-test-route".to_owned(),
        revision: 1,
        geometry: vec![start.clone(), corner.clone(), end.clone()],
        maneuvers: vec![
            RouteManeuver {
                id: "m-depart".to_owned(),
                maneuver_type: RouteManeuverType::Depart,
                location: start,
                distance_from_start_m: 0.0,
                distance_to_next_m: Some(seg1),
                instruction_text: Some("Head north".to_owned()),
            },
            RouteManeuver {
                id: "m-left".to_owned(),
                maneuver_type: RouteManeuverType::Left,
                location: corner,
                distance_from_start_m: seg1,
                distance_to_next_m: Some(seg2),
                instruction_text: Some("Turn left".to_owned()),
            },
            RouteManeuver {
                id: "m-arrive".to_owned(),
                maneuver_type: RouteManeuverType::Arrive,
                location: end,
                distance_from_start_m: total,
                distance_to_next_m: None,
                instruction_text: Some("Arrive".to_owned()),
            },
        ],
        summary: RouteSummary {
            total_distance_m: total,
            estimated_duration_s: (total / 4.0) as u32,
            start_label: Some("Kamppi".to_owned()),
            destination_label: Some("Etu-Töölö".to_owned()),
        },
        provenance: RouteProvenance {
            provider: RouteProvider::Osm,
            source_ref: Some("fixture".to_owned()),
            generated_at_unix_ms: 1_700_000_000_000,
        },
    }
}

fn baked_route_helsinki_two_points() -> RoutePackage {
    let start = route_start();
    let end = route_end();
    let distance = haversine_m(&start, &end) as f32;
    assert!(
        distance > 500.0,
        "test endpoints must be > 500 m apart so the route polyline is non-degenerate (got {distance} m)"
    );

    RoutePackage {
        version: CURRENT_ROUTE_PACKAGE_VERSION,
        route_id: "helsinki-two-points-test-route".to_owned(),
        revision: 1,
        geometry: vec![start.clone(), end.clone()],
        maneuvers: vec![
            RouteManeuver {
                id: "m-depart".to_owned(),
                maneuver_type: RouteManeuverType::Depart,
                location: start,
                distance_from_start_m: 0.0,
                distance_to_next_m: Some(distance),
                instruction_text: Some("Head toward destination".to_owned()),
            },
            RouteManeuver {
                id: "m-arrive".to_owned(),
                maneuver_type: RouteManeuverType::Arrive,
                location: end,
                distance_from_start_m: distance,
                distance_to_next_m: None,
                instruction_text: Some("Arrive".to_owned()),
            },
        ],
        summary: RouteSummary {
            total_distance_m: distance,
            estimated_duration_s: (distance / 4.0) as u32,
            start_label: Some("Kamppi".to_owned()),
            destination_label: Some("Töölö".to_owned()),
        },
        provenance: RouteProvenance {
            provider: RouteProvider::Osm,
            source_ref: Some("fixture".to_owned()),
            generated_at_unix_ms: 1_700_000_000_000,
        },
    }
}

/// Offset a coordinate by approximate metres (east, north). Good enough at
/// Helsinki latitudes to place a rider a known distance off-route.
fn offset_point(origin: &GeoPoint, east_m: f64, north_m: f64) -> runtime_core::api::GpsSample {
    let lat_per_m = 1.0 / 111_320.0;
    let lon_per_m = 1.0 / (111_320.0 * (origin.lat_deg * std::f64::consts::PI / 180.0).cos());
    runtime_core::api::GpsSample {
        lat_deg: origin.lat_deg + north_m * lat_per_m,
        lon_deg: origin.lon_deg + east_m * lon_per_m,
        speed_mps: 4.0,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    }
}

fn apply_route_and_tick(runtime: &mut RuntimeCore, route: RoutePackage) {
    let frame = RuntimeInputFrame::new(Duration::from_millis(16))
        .with_route_sync(RouteSyncMessage::Set(RouteSetMessage { route }))
        .with_viewport(FIXTURE_VIEWPORT);
    runtime.step(frame);
}

#[test]
fn route_set_message_populates_route_render_state() {
    // Flow #13 setup: when a route is applied, RouteRenderState reflects it.
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    apply_route_and_tick(&mut runtime, baked_route_helsinki_two_points());
    let output = runtime.step(RuntimeInputFrame::new(Duration::from_millis(16)));
    assert!(
        output.route.geometry_world.len() >= 2,
        "applied route should expose geometry_world (got {})",
        output.route.geometry_world.len()
    );
}

#[test]
fn off_route_detection_triggers_reroute_request_at_boundary() {
    // Flow #61: sustained deviation *just above* the off-route enter threshold
    // (default 35 m) must flip `reroute_requested` after the reroute delay.
    // A city-scale offset would prove the trivial case — instead we drop the
    // rider 60 m perpendicular to the route start so regressions that raise
    // the threshold are caught.
    let route = baked_route_helsinki_two_points();
    let start = route.geometry[0].clone();
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    apply_route_and_tick(&mut runtime, route);

    // 60 m east of the route start — outside the default 35 m enter distance
    // and outside the 22 m exit distance, so the runtime cannot flip back.
    let off_fix = offset_point(&start, 60.0, 0.0);
    for _ in 0..120 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(100))
                .with_gps(off_fix)
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }
    let output = runtime.step(RuntimeInputFrame::new(Duration::from_millis(100)));
    assert!(
        output.route.off_route,
        "60 m deviation should trigger off_route=true; threshold drift regressions caught here"
    );
    assert!(
        output.route.reroute_requested,
        "sustained off-route beyond the reroute delay should flip reroute_requested"
    );
}

#[test]
fn returning_to_route_clears_reroute_request() {
    let route = baked_route_helsinki_two_points();
    let start = route.geometry[0].clone();
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    apply_route_and_tick(&mut runtime, route);

    // Drift off at 60 m east.
    let off_fix = offset_point(&start, 60.0, 0.0);
    for _ in 0..120 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(100))
                .with_gps(off_fix)
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }
    // Return right onto the route.
    let on_fix = runtime_core::api::GpsSample {
        lat_deg: start.lat_deg,
        lon_deg: start.lon_deg,
        speed_mps: 4.0,
        course_rad: Some(0.0),
        horizontal_accuracy_m: Some(4.0),
    };
    for _ in 0..30 {
        runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(100))
                .with_gps(on_fix)
                .with_viewport(FIXTURE_VIEWPORT),
        );
    }
    let output = runtime.step(RuntimeInputFrame::new(Duration::from_millis(100)));
    assert!(!output.route.off_route, "back on route must clear off_route");
    assert!(
        !output.route.reroute_requested,
        "back on route must clear reroute_requested"
    );
}

/// Straight-north route with a Merge maneuver 300 m in. Merge gets
/// `TurnAlertKind::Generic` without any geometry-derived turn classification,
/// so alerts fire reliably as the rider approaches the maneuver — sidesteps
/// the Web-Mercator projection issue an L-shape route has at Helsinki's
/// latitude.
fn baked_straight_route_with_merge() -> RoutePackage {
    let start = GeoPoint::new(60.1699, 24.9328);
    let merge = GeoPoint::new(60.1726, 24.9328); // ~300 m due north
    let end = GeoPoint::new(60.1753, 24.9328);   // ~600 m due north
    let seg1 = haversine_m(&start, &merge) as f32;
    let seg2 = haversine_m(&merge, &end) as f32;
    let total = seg1 + seg2;
    RoutePackage {
        version: CURRENT_ROUTE_PACKAGE_VERSION,
        route_id: "helsinki-straight-merge-test-route".to_owned(),
        revision: 1,
        geometry: vec![start.clone(), merge.clone(), end.clone()],
        maneuvers: vec![
            RouteManeuver {
                id: "m-depart".to_owned(),
                maneuver_type: RouteManeuverType::Depart,
                location: start,
                distance_from_start_m: 0.0,
                distance_to_next_m: Some(seg1),
                instruction_text: Some("Head north".to_owned()),
            },
            RouteManeuver {
                id: "m-merge".to_owned(),
                maneuver_type: RouteManeuverType::Merge,
                location: merge,
                distance_from_start_m: seg1,
                distance_to_next_m: Some(seg2),
                instruction_text: Some("Merge onto path".to_owned()),
            },
            RouteManeuver {
                id: "m-arrive".to_owned(),
                maneuver_type: RouteManeuverType::Arrive,
                location: end,
                distance_from_start_m: total,
                distance_to_next_m: None,
                instruction_text: Some("Arrive".to_owned()),
            },
        ],
        summary: RouteSummary {
            total_distance_m: total,
            estimated_duration_s: (total / 4.0) as u32,
            start_label: Some("Kamppi".to_owned()),
            destination_label: Some("Töölö".to_owned()),
        },
        provenance: RouteProvenance {
            provider: RouteProvider::Osm,
            source_ref: Some("fixture".to_owned()),
            generated_at_unix_ms: 1_700_000_000_000,
        },
    }
}

#[test]
fn next_turn_distance_decreases_as_rider_approaches_the_next_maneuver() {
    // Flow #13 / #56. Walks rider due north along a straight route toward a
    // Merge maneuver at 300 m. Merge emits a Generic alert unconditionally,
    // so the only thing being tested here is that the rider's progress +
    // distance_remaining trend are computed correctly as the rider closes in.
    let route = baked_straight_route_with_merge();
    let start = route.geometry[0].clone();
    let merge = route.geometry[1].clone();
    let mut runtime = RuntimeCore::new(RuntimeConfig::default());
    apply_route_and_tick(&mut runtime, route);

    let mut distances: Vec<f32> = Vec::new();
    // Walk the last ~90 m of the approach in 20 small steps so every sampled
    // frame is already inside the 80 m alert window.
    for step in 0..20 {
        let t = 0.70 + (step as f64 / 20.0) * 0.29;
        let lat = start.lat_deg + (merge.lat_deg - start.lat_deg) * t;
        let lon = start.lon_deg; // straight north — lon doesn't change
        let output = runtime.step(
            RuntimeInputFrame::new(Duration::from_millis(500))
                .with_gps(runtime_core::api::GpsSample {
                    lat_deg: lat,
                    lon_deg: lon,
                    speed_mps: 3.0,
                    course_rad: Some(0.0),
                    horizontal_accuracy_m: Some(4.0),
                })
                .with_viewport(FIXTURE_VIEWPORT),
        );
        if let Some(alert) = output.route.upcoming_turn_alert.as_ref() {
            distances.push(alert.distance_remaining_m);
        }
    }
    assert!(
        distances.len() >= 2,
        "runtime emitted {} turn alerts across the approach — flow #56 expects at least 2",
        distances.len()
    );
    assert!(
        distances[0] > distances[distances.len() - 1],
        "distance to merge should trend down across the approach (first {:?} → last {:?})",
        distances[0],
        distances[distances.len() - 1]
    );
}
