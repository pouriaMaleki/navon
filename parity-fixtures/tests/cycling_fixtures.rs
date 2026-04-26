//! Canary test: the cycling fixtures shared by web + Android + iOS parsers
//! must keep the shape those parsers rely on. If BRouter or OSRM ever change
//! their JSON layout we want to find out here, BEFORE the regression hits
//! three companion apps.
//!
//! Spec source: `docs/companion-app-architecture.md` "OSM cycling sources"
//! describes the multi-source cycling adapter that consumes these fixtures.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn fixture(name: &str) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("data/cycling");
    path.push(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e))
}

#[test]
fn brouter_fastbike_fixture_has_expected_shape() {
    let v = fixture("brouter-fastbike-helsinki-kallio.json");
    assert_eq!(v["type"], "FeatureCollection");
    let features = v["features"].as_array().expect("features array");
    assert!(!features.is_empty(), "must have at least one feature");
    let f = &features[0];
    assert_eq!(f["type"], "Feature");
    let geom = &f["geometry"];
    assert_eq!(geom["type"], "LineString");
    let coords = geom["coordinates"].as_array().expect("coordinates array");
    assert!(coords.len() > 50, "fastbike route should have many polyline points");
    let props = &f["properties"];
    let track_length: f64 = props["track-length"]
        .as_str()
        .expect("track-length is a string")
        .parse()
        .expect("track-length parses as number");
    assert!(track_length > 1000.0 && track_length < 5000.0);
    let voicehints = props["voicehints"].as_array().expect("voicehints array");
    assert!(!voicehints.is_empty(), "fastbike with timode=2 must have voicehints");
}

#[test]
fn brouter_trekking_fixture_has_voicehints() {
    let v = fixture("brouter-trekking-helsinki-kallio.json");
    let props = &v["features"][0]["properties"];
    assert!(props["track-length"].is_string());
    assert!(props["voicehints"].as_array().is_some_and(|a| !a.is_empty()));
}

#[test]
fn osrm_bike_fixture_uses_geojson_geometry() {
    let v = fixture("osrm-bike-helsinki-kallio.json");
    assert_eq!(v["code"], "Ok");
    let routes = v["routes"].as_array().expect("routes array");
    assert!(!routes.is_empty());
    let r = &routes[0];
    let geom = &r["geometry"];
    assert_eq!(geom["type"], "LineString", "fixture was captured with geometries=geojson");
    let coords = geom["coordinates"].as_array().expect("coordinates array");
    assert!(coords.len() > 50);
    let dist = r["distance"].as_f64().expect("distance number");
    assert!(dist > 1000.0 && dist < 5000.0);
}
