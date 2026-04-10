#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutePackageVersion {
    pub major: u16,
    pub minor: u16,
}

impl RoutePackageVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    pub const fn is_runtime_compatible(self) -> bool {
        self.major == CURRENT_ROUTE_PACKAGE_VERSION.major
            && self.minor <= CURRENT_ROUTE_PACKAGE_VERSION.minor
    }
}

pub const CURRENT_ROUTE_PACKAGE_VERSION: RoutePackageVersion = RoutePackageVersion::new(1, 0);

#[derive(Debug, Clone, PartialEq)]
pub struct GeoPoint {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

impl GeoPoint {
    pub const fn new(lat_deg: f64, lon_deg: f64) -> Self {
        Self { lat_deg, lon_deg }
    }

    pub fn is_valid(&self) -> bool {
        self.lat_deg.is_finite()
            && self.lon_deg.is_finite()
            && (-90.0..=90.0).contains(&self.lat_deg)
            && (-180.0..=180.0).contains(&self.lon_deg)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteManeuverType {
    Depart,
    Straight,
    SlightLeft,
    Left,
    SharpLeft,
    SlightRight,
    Right,
    SharpRight,
    Uturn,
    Roundabout,
    Merge,
    Ramp,
    Arrive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteManeuver {
    pub id: String,
    pub maneuver_type: RouteManeuverType,
    pub location: GeoPoint,
    pub distance_from_start_m: f32,
    pub distance_to_next_m: Option<f32>,
    pub instruction_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteSummary {
    pub total_distance_m: f32,
    pub estimated_duration_s: u32,
    pub start_label: Option<String>,
    pub destination_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RouteProvider {
    HslDigitransit,
    Osm,
    Gpx,
    Fit,
    Tcx,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteProvenance {
    pub provider: RouteProvider,
    pub source_ref: Option<String>,
    pub generated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutePackage {
    pub version: RoutePackageVersion,
    pub route_id: String,
    pub revision: u64,
    pub geometry: Vec<GeoPoint>,
    pub maneuvers: Vec<RouteManeuver>,
    pub summary: RouteSummary,
    pub provenance: RouteProvenance,
}

impl RoutePackage {
    pub fn validate(&self) -> Result<(), RoutePackageError> {
        if !self.version.is_runtime_compatible() {
            return Err(RoutePackageError::IncompatibleVersion {
                incoming: self.version,
                supported: CURRENT_ROUTE_PACKAGE_VERSION,
            });
        }

        if self.route_id.trim().is_empty() {
            return Err(RoutePackageError::EmptyRouteId);
        }

        if self.geometry.len() < 2 {
            return Err(RoutePackageError::InsufficientGeometryPoints(
                self.geometry.len(),
            ));
        }

        for (index, point) in self.geometry.iter().enumerate() {
            if !point.is_valid() {
                return Err(RoutePackageError::InvalidGeometryPoint {
                    index,
                    lat_deg: point.lat_deg,
                    lon_deg: point.lon_deg,
                });
            }
        }

        if !self.summary.total_distance_m.is_finite() || self.summary.total_distance_m < 0.0 {
            return Err(RoutePackageError::InvalidTotalDistance(
                self.summary.total_distance_m,
            ));
        }

        let mut previous_distance = None;
        for maneuver in &self.maneuvers {
            if !maneuver.location.is_valid() {
                return Err(RoutePackageError::InvalidManeuverLocation {
                    maneuver_id: maneuver.id.clone(),
                });
            }
            if maneuver.distance_from_start_m.is_nan() || maneuver.distance_from_start_m < 0.0 {
                return Err(RoutePackageError::InvalidManeuverDistance {
                    maneuver_id: maneuver.id.clone(),
                    distance_from_start_m: maneuver.distance_from_start_m,
                });
            }
            if maneuver.distance_from_start_m > self.summary.total_distance_m + 0.5 {
                return Err(RoutePackageError::ManeuverDistanceExceedsRoute {
                    maneuver_id: maneuver.id.clone(),
                    distance_from_start_m: maneuver.distance_from_start_m,
                    route_distance_m: self.summary.total_distance_m,
                });
            }
            if let Some(previous) = previous_distance
                && maneuver.distance_from_start_m + 0.001 < previous
            {
                return Err(RoutePackageError::ManeuversOutOfOrder {
                    previous_distance_m: previous,
                    next_distance_m: maneuver.distance_from_start_m,
                });
            }
            previous_distance = Some(maneuver.distance_from_start_m);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoutePackageError {
    IncompatibleVersion {
        incoming: RoutePackageVersion,
        supported: RoutePackageVersion,
    },
    EmptyRouteId,
    InsufficientGeometryPoints(usize),
    InvalidGeometryPoint {
        index: usize,
        lat_deg: f64,
        lon_deg: f64,
    },
    InvalidTotalDistance(f32),
    InvalidManeuverLocation {
        maneuver_id: String,
    },
    InvalidManeuverDistance {
        maneuver_id: String,
        distance_from_start_m: f32,
    },
    ManeuverDistanceExceedsRoute {
        maneuver_id: String,
        distance_from_start_m: f32,
        route_distance_m: f32,
    },
    ManeuversOutOfOrder {
        previous_distance_m: f32,
        next_distance_m: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RouteSyncMessage {
    Set(RouteSetMessage),
    Update(RouteUpdateMessage),
    Clear(RouteClearMessage),
    Status(RouteStatusMessage),
    RerouteRequest(RouteRerouteRequestMessage),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteSetMessage {
    pub route: RoutePackage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteUpdateMessage {
    pub route_id: String,
    pub revision: u64,
    pub route: RoutePackage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteClearMessage {
    pub route_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSyncStatusCode {
    Accepted,
    Applying,
    Active,
    Cleared,
    Rejected,
    RetryableFailure,
    FatalFailure,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteStatusMessage {
    pub route_id: Option<String>,
    pub revision: Option<u64>,
    pub status: RouteSyncStatusCode,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteRerouteRequestMessage {
    pub route_id: Option<String>,
    pub rider_position: GeoPoint,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_route() -> RoutePackage {
        RoutePackage {
            version: CURRENT_ROUTE_PACKAGE_VERSION,
            route_id: "hsl-test-route-001".to_owned(),
            revision: 1,
            geometry: vec![
                GeoPoint::new(60.1699, 24.9384),
                GeoPoint::new(60.1712, 24.9443),
            ],
            maneuvers: vec![RouteManeuver {
                id: "m1".to_owned(),
                maneuver_type: RouteManeuverType::Depart,
                location: GeoPoint::new(60.1699, 24.9384),
                distance_from_start_m: 0.0,
                distance_to_next_m: Some(180.0),
                instruction_text: Some("Head east on bike lane".to_owned()),
            }],
            summary: RouteSummary {
                total_distance_m: 180.0,
                estimated_duration_s: 45,
                start_label: Some("Kamppi".to_owned()),
                destination_label: Some("Kallio".to_owned()),
            },
            provenance: RouteProvenance {
                provider: RouteProvider::HslDigitransit,
                source_ref: Some("digitransit:itinerary:abc123".to_owned()),
                generated_at_unix_ms: 1_764_113_200_000,
            },
        }
    }

    #[test]
    fn sample_route_package_is_valid() {
        let route = sample_route();
        assert_eq!(route.validate(), Ok(()));
    }

    #[test]
    fn rejects_empty_route_id() {
        let mut route = sample_route();
        route.route_id = "   ".to_owned();

        assert_eq!(route.validate(), Err(RoutePackageError::EmptyRouteId));
    }

    #[test]
    fn rejects_short_geometry() {
        let mut route = sample_route();
        route.geometry = vec![GeoPoint::new(60.1699, 24.9384)];

        assert_eq!(
            route.validate(),
            Err(RoutePackageError::InsufficientGeometryPoints(1))
        );
    }

    #[test]
    fn rejects_invalid_geometry_point() {
        let mut route = sample_route();
        route.geometry[1] = GeoPoint::new(120.0, 24.9443);

        assert_eq!(
            route.validate(),
            Err(RoutePackageError::InvalidGeometryPoint {
                index: 1,
                lat_deg: 120.0,
                lon_deg: 24.9443,
            })
        );
    }

    #[test]
    fn rejects_out_of_order_maneuvers() {
        let mut route = sample_route();
        route.maneuvers.push(RouteManeuver {
            id: "m2".to_owned(),
            maneuver_type: RouteManeuverType::Left,
            location: GeoPoint::new(60.1705, 24.9410),
            distance_from_start_m: 50.0,
            distance_to_next_m: Some(130.0),
            instruction_text: Some("Turn left".to_owned()),
        });
        route.maneuvers[0].distance_from_start_m = 90.0;

        assert_eq!(
            route.validate(),
            Err(RoutePackageError::ManeuversOutOfOrder {
                previous_distance_m: 90.0,
                next_distance_m: 50.0,
            })
        );
    }

    #[test]
    fn version_compatibility_is_major_strict_and_minor_backward_only() {
        assert!(RoutePackageVersion::new(1, 0).is_runtime_compatible());
        assert!(!RoutePackageVersion::new(1, 1).is_runtime_compatible());
        assert!(!RoutePackageVersion::new(2, 0).is_runtime_compatible());
    }
}
