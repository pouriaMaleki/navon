use serde::Serialize;
use wasm_bindgen::JsValue;

use route_import_gpx::{GpxImportOptions, import_gpx_route};
use runtime_core::api::{RouteManeuverType, RoutePackage, RouteProvider};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsonRoutePackage {
    version: JsonRoutePackageVersion,
    #[serde(rename = "routeId")]
    route_id: String,
    revision: u64,
    geometry: Vec<JsonGeoPoint>,
    maneuvers: Vec<JsonRouteManeuver>,
    summary: JsonRouteSummary,
    provenance: JsonRouteProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct JsonRoutePackageVersion {
    major: u16,
    minor: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct JsonGeoPoint {
    #[serde(rename = "latDeg")]
    lat_deg: f64,
    #[serde(rename = "lonDeg")]
    lon_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct JsonRouteManeuver {
    id: String,
    #[serde(rename = "maneuverType")]
    maneuver_type: &'static str,
    location: JsonGeoPoint,
    #[serde(rename = "distanceFromStartM")]
    distance_from_start_m: f32,
    #[serde(rename = "distanceToNextM")]
    distance_to_next_m: Option<f32>,
    #[serde(rename = "instructionText")]
    instruction_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct JsonRouteSummary {
    #[serde(rename = "totalDistanceM")]
    total_distance_m: f32,
    #[serde(rename = "estimatedDurationS")]
    estimated_duration_s: u32,
    #[serde(rename = "startLabel")]
    start_label: Option<String>,
    #[serde(rename = "destinationLabel")]
    destination_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct JsonRouteProvenance {
    provider: &'static str,
    #[serde(rename = "sourceRef")]
    source_ref: Option<String>,
    #[serde(rename = "generatedAtUnixMs")]
    generated_at_unix_ms: u64,
}

pub fn import_gpx_route_json(gpx_xml: &str, source_ref: Option<String>) -> Result<String, JsValue> {
    let route = import_gpx_route(
        gpx_xml,
        &GpxImportOptions {
            source_ref,
            generated_at_unix_ms: current_time_ms(),
            ..GpxImportOptions::default()
        },
    )
    .map_err(|error| JsValue::from_str(&error.to_string()))?;

    serde_json::to_string(&route_to_json(&route))
        .map_err(|error| JsValue::from_str(&format!("failed to serialize route package: {error}")))
}

fn current_time_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

fn route_to_json(route: &RoutePackage) -> JsonRoutePackage {
    JsonRoutePackage {
        version: JsonRoutePackageVersion {
            major: route.version.major,
            minor: route.version.minor,
        },
        route_id: route.route_id.clone(),
        revision: route.revision,
        geometry: route.geometry.iter().map(point_to_json).collect(),
        maneuvers: route
            .maneuvers
            .iter()
            .map(|maneuver| JsonRouteManeuver {
                id: maneuver.id.clone(),
                maneuver_type: maneuver_type_to_str(maneuver.maneuver_type),
                location: point_to_json(&maneuver.location),
                distance_from_start_m: maneuver.distance_from_start_m,
                distance_to_next_m: maneuver.distance_to_next_m,
                instruction_text: maneuver.instruction_text.clone(),
            })
            .collect(),
        summary: JsonRouteSummary {
            total_distance_m: route.summary.total_distance_m,
            estimated_duration_s: route.summary.estimated_duration_s,
            start_label: route.summary.start_label.clone(),
            destination_label: route.summary.destination_label.clone(),
        },
        provenance: JsonRouteProvenance {
            provider: provider_to_str(&route.provenance.provider),
            source_ref: route.provenance.source_ref.clone(),
            generated_at_unix_ms: route.provenance.generated_at_unix_ms,
        },
    }
}

fn point_to_json(point: &runtime_core::api::GeoPoint) -> JsonGeoPoint {
    JsonGeoPoint {
        lat_deg: point.lat_deg,
        lon_deg: point.lon_deg,
    }
}

fn maneuver_type_to_str(value: RouteManeuverType) -> &'static str {
    match value {
        RouteManeuverType::Depart => "depart",
        RouteManeuverType::Straight => "straight",
        RouteManeuverType::SlightLeft => "slight_left",
        RouteManeuverType::Left => "left",
        RouteManeuverType::SharpLeft => "sharp_left",
        RouteManeuverType::SlightRight => "slight_right",
        RouteManeuverType::Right => "right",
        RouteManeuverType::SharpRight => "sharp_right",
        RouteManeuverType::Uturn => "uturn",
        RouteManeuverType::Roundabout => "roundabout",
        RouteManeuverType::Merge => "merge",
        RouteManeuverType::Ramp => "ramp",
        RouteManeuverType::Arrive => "arrive",
    }
}

fn provider_to_str(value: &RouteProvider) -> &'static str {
    match value {
        RouteProvider::HslDigitransit => "hsl_digitransit",
        RouteProvider::GoogleIngest => "google_ingest",
        RouteProvider::Osm => "osm",
        RouteProvider::Gpx => "gpx",
        RouteProvider::Fit => "fit",
        RouteProvider::Tcx => "tcx",
        RouteProvider::GarminApi => "garmin_api",
        RouteProvider::GarminFile => "garmin_file",
        RouteProvider::Unknown(_) => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_gpx_into_runtime_route_json() {
        let json = import_gpx_route_json(
            r#"<gpx version="1.1" creator="test"><trk><name>Lunch Loop</name><trkseg><trkpt lat="60.1699" lon="24.9384" /><trkpt lat="60.1704" lon="24.9384" /><trkpt lat="60.1704" lon="24.9390" /></trkseg></trk></gpx>"#,
            Some("lunch-loop.gpx".to_owned()),
        )
        .expect("GPX import should serialize to JSON");
        assert!(json.contains("\"provider\":\"gpx\""));
        assert!(json.contains("\"routeId\":\"lunch-loop\""));
    }
}
