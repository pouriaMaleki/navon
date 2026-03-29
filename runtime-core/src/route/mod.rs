use crate::api::input::GpsSample;
use crate::api::query::WorldPoint;
use crate::api::route::{
    GeoPoint, RoutePackage, RoutePackageError, RouteSyncMessage, RouteSyncStatusCode,
    RouteUpdateMessage,
};
use crate::motion::project_gps_to_world;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RouteRenderState {
    pub route_id: Option<String>,
    pub revision: Option<u64>,
    pub geometry_world: Vec<WorldPoint>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveRouteState {
    active: Option<ActiveRoute>,
}

#[derive(Debug, Clone)]
struct ActiveRoute {
    route_id: String,
    revision: u64,
    geometry_world: Vec<WorldPoint>,
}

impl ActiveRouteState {
    pub fn apply_sync(&mut self, message: &RouteSyncMessage) {
        match message {
            RouteSyncMessage::Set(set) => {
                if let Ok(active) = ActiveRoute::from_package(&set.route) {
                    self.active = Some(active);
                }
            }
            RouteSyncMessage::Update(update) => {
                self.apply_update(update);
            }
            RouteSyncMessage::Clear(clear) => {
                if clear
                    .route_id
                    .as_ref()
                    .is_none_or(|route_id| self.active_route_id() == Some(route_id.as_str()))
                {
                    self.active = None;
                }
            }
            RouteSyncMessage::Status(status) => {
                if matches!(
                    status.status,
                    RouteSyncStatusCode::Cleared | RouteSyncStatusCode::FatalFailure
                ) {
                    self.active = None;
                }
            }
            RouteSyncMessage::RerouteRequest(_) => {}
        }
    }

    pub fn snapshot(&self) -> RouteRenderState {
        self.active
            .as_ref()
            .map(|active| RouteRenderState {
                route_id: Some(active.route_id.clone()),
                revision: Some(active.revision),
                geometry_world: active.geometry_world.clone(),
            })
            .unwrap_or_default()
    }

    fn apply_update(&mut self, update: &RouteUpdateMessage) {
        let Some(current_id) = self.active_route_id() else {
            if let Ok(active) = ActiveRoute::from_package(&update.route) {
                self.active = Some(active);
            }
            return;
        };

        if current_id != update.route_id {
            return;
        }

        if let Some(active) = &self.active
            && update.revision < active.revision
        {
            return;
        }

        if let Ok(active) = ActiveRoute::from_package(&update.route) {
            self.active = Some(active);
        }
    }

    fn active_route_id(&self) -> Option<&str> {
        self.active.as_ref().map(|route| route.route_id.as_str())
    }
}

impl ActiveRoute {
    fn from_package(route: &RoutePackage) -> Result<Self, RoutePackageError> {
        route.validate()?;
        Ok(Self {
            route_id: route.route_id.clone(),
            revision: route.revision,
            geometry_world: route.geometry.iter().map(geo_to_world).collect(),
        })
    }
}

fn geo_to_world(point: &GeoPoint) -> WorldPoint {
    project_gps_to_world(GpsSample {
        lat_deg: point.lat_deg,
        lon_deg: point.lon_deg,
        speed_mps: 0.0,
        course_rad: None,
        horizontal_accuracy_m: None,
    })
}

#[cfg(test)]
mod tests {
    use crate::api::route::{
        CURRENT_ROUTE_PACKAGE_VERSION, RouteClearMessage, RouteManeuver, RouteManeuverType,
        RoutePackage, RouteProvenance, RouteProvider, RouteSetMessage, RouteSummary,
    };

    use super::*;

    fn sample_route(route_id: &str, revision: u64) -> RoutePackage {
        RoutePackage {
            version: CURRENT_ROUTE_PACKAGE_VERSION,
            route_id: route_id.to_owned(),
            revision,
            geometry: vec![
                GeoPoint::new(60.1699, 24.9384),
                GeoPoint::new(60.1712, 24.9443),
            ],
            maneuvers: vec![RouteManeuver {
                id: "m1".to_owned(),
                maneuver_type: RouteManeuverType::Depart,
                location: GeoPoint::new(60.1699, 24.9384),
                distance_from_start_m: 0.0,
                distance_to_next_m: None,
                instruction_text: None,
            }],
            summary: RouteSummary {
                total_distance_m: 100.0,
                estimated_duration_s: 30,
                start_label: None,
                destination_label: None,
            },
            provenance: RouteProvenance {
                provider: RouteProvider::HslDigitransit,
                source_ref: None,
                generated_at_unix_ms: 0,
            },
        }
    }

    #[test]
    fn set_populates_route_snapshot() {
        let mut state = ActiveRouteState::default();
        state.apply_sync(&RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route("route-1", 1),
        }));

        let snapshot = state.snapshot();
        assert_eq!(snapshot.route_id.as_deref(), Some("route-1"));
        assert_eq!(snapshot.revision, Some(1));
        assert_eq!(snapshot.geometry_world.len(), 2);
    }

    #[test]
    fn clear_removes_route_snapshot() {
        let mut state = ActiveRouteState::default();
        state.apply_sync(&RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route("route-1", 1),
        }));

        state.apply_sync(&RouteSyncMessage::Clear(RouteClearMessage {
            route_id: Some("route-1".to_owned()),
        }));

        assert!(state.snapshot().route_id.is_none());
    }
}
