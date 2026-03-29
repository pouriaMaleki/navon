use std::collections::HashSet;

use runtime_core::api::{
    CURRENT_ROUTE_PACKAGE_VERSION, GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage,
    RoutePackageError, RoutePackageVersion, RouteProvenance, RouteProvider, RouteSummary,
};

fn canonical_fixture(provider: RouteProvider, index: usize) -> RoutePackage {
    let route_id = format!("route-{}-001", provider_slug(&provider));
    let base_lat = 60.1699 + index as f64 * 0.001;
    let base_lon = 24.9384 + index as f64 * 0.001;

    RoutePackage {
        version: CURRENT_ROUTE_PACKAGE_VERSION,
        route_id,
        revision: 1,
        geometry: vec![
            GeoPoint::new(base_lat, base_lon),
            GeoPoint::new(base_lat + 0.0012, base_lon + 0.0018),
            GeoPoint::new(base_lat + 0.0021, base_lon + 0.0023),
        ],
        maneuvers: vec![
            RouteManeuver {
                id: "depart".to_owned(),
                maneuver_type: RouteManeuverType::Depart,
                location: GeoPoint::new(base_lat, base_lon),
                distance_from_start_m: 0.0,
                distance_to_next_m: Some(110.0),
                instruction_text: Some("Start riding".to_owned()),
            },
            RouteManeuver {
                id: "arrive".to_owned(),
                maneuver_type: RouteManeuverType::Arrive,
                location: GeoPoint::new(base_lat + 0.0021, base_lon + 0.0023),
                distance_from_start_m: 320.0,
                distance_to_next_m: None,
                instruction_text: Some("Arrive at destination".to_owned()),
            },
        ],
        summary: RouteSummary {
            total_distance_m: 320.0,
            estimated_duration_s: 95,
            start_label: Some("Start".to_owned()),
            destination_label: Some("Destination".to_owned()),
        },
        provenance: RouteProvenance {
            provider,
            source_ref: Some(format!("{}:fixture", provider_slug_from_index(index))),
            generated_at_unix_ms: 1_764_113_200_000 + index as u64,
        },
    }
}

fn provider_slug(provider: &RouteProvider) -> &'static str {
    match provider {
        RouteProvider::HslDigitransit => "hsl",
        RouteProvider::GoogleIngest => "google",
        RouteProvider::Osm => "osm",
        RouteProvider::Gpx => "gpx",
        RouteProvider::Fit => "fit",
        RouteProvider::Tcx => "tcx",
        RouteProvider::GarminApi => "garmin-api",
        RouteProvider::GarminFile => "garmin-file",
        RouteProvider::Unknown(_) => "unknown",
    }
}

fn provider_slug_from_index(index: usize) -> &'static str {
    match index {
        0 => "hsl",
        1 => "google",
        2 => "osm",
        3 => "gpx",
        4 => "fit",
        5 => "tcx",
        6 => "garmin-api",
        7 => "garmin-file",
        _ => "unknown",
    }
}

fn providers() -> Vec<RouteProvider> {
    vec![
        RouteProvider::HslDigitransit,
        RouteProvider::GoogleIngest,
        RouteProvider::Osm,
        RouteProvider::Gpx,
        RouteProvider::Fit,
        RouteProvider::Tcx,
        RouteProvider::GarminApi,
        RouteProvider::GarminFile,
    ]
}

#[test]
fn canonical_provider_fixtures_validate_against_route_contract() {
    let mut route_ids = HashSet::new();

    for (index, provider) in providers().into_iter().enumerate() {
        let fixture = canonical_fixture(provider.clone(), index);

        assert!(route_ids.insert(fixture.route_id.clone()));
        assert_eq!(fixture.provenance.provider, provider);
        assert_eq!(fixture.validate(), Ok(()));
    }
}

#[test]
fn canonical_provider_fixtures_reject_incompatible_version() {
    for (index, provider) in providers().into_iter().enumerate() {
        let mut fixture = canonical_fixture(provider, index);
        fixture.version = RoutePackageVersion::new(CURRENT_ROUTE_PACKAGE_VERSION.major + 1, 0);

        assert_eq!(
            fixture.validate(),
            Err(RoutePackageError::IncompatibleVersion {
                incoming: fixture.version,
                supported: CURRENT_ROUTE_PACKAGE_VERSION,
            })
        );
    }
}
