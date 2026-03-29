use crate::api::input::GpsSample;
use crate::api::query::WorldPoint;
use crate::api::route::{
    GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage, RoutePackageError, RouteSyncMessage,
    RouteSyncStatusCode, RouteUpdateMessage,
};
use crate::motion::project_gps_to_world;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnAlertKind {
    Left,
    Right,
    Uturn,
    Generic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpcomingTurnAlert {
    pub kind: TurnAlertKind,
    pub distance_remaining_m: f32,
    pub instruction_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RouteRenderState {
    pub route_id: Option<String>,
    pub revision: Option<u64>,
    pub progress_distance_m: Option<f64>,
    pub off_route: bool,
    pub off_route_distance_m: Option<f64>,
    pub upcoming_turn_alert: Option<UpcomingTurnAlert>,
    pub geometry_world: Vec<WorldPoint>,
    pub completed_geometry_world: Vec<WorldPoint>,
    pub remaining_geometry_world: Vec<WorldPoint>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveRouteState {
    active: Option<ActiveRoute>,
}

#[derive(Debug, Clone)]
struct ActiveRoute {
    route_id: String,
    revision: u64,
    total_distance_m: f64,
    progress_distance_m: f64,
    off_route: bool,
    off_route_distance_m: f64,
    upcoming_turn_alert: Option<UpcomingTurnAlert>,
    maneuvers: Vec<StoredManeuver>,
    geometry_world: Vec<WorldPoint>,
}

#[derive(Debug, Clone)]
struct StoredManeuver {
    maneuver_type: RouteManeuverType,
    distance_along_route_m: f32,
    instruction_text: Option<String>,
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
            .map(ActiveRoute::snapshot)
            .unwrap_or_default()
    }

    pub fn advance_progress(
        &mut self,
        rider_world: WorldPoint,
        off_route_enter_distance_m: f64,
        off_route_exit_distance_m: f64,
        major_turn_alert_distance_m: f64,
    ) {
        let Some(active) = &mut self.active else {
            return;
        };
        active.advance_progress(
            rider_world,
            off_route_enter_distance_m,
            off_route_exit_distance_m,
            major_turn_alert_distance_m,
        );
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
        let geometry_world = route.geometry.iter().map(geo_to_world).collect::<Vec<_>>();
        let total_distance_m = polyline_total_distance(&geometry_world);
        Ok(Self {
            route_id: route.route_id.clone(),
            revision: route.revision,
            total_distance_m,
            progress_distance_m: 0.0,
            off_route: false,
            off_route_distance_m: 0.0,
            upcoming_turn_alert: None,
            maneuvers: route
                .maneuvers
                .iter()
                .map(|maneuver| StoredManeuver::from_route_maneuver(maneuver, &geometry_world))
                .collect(),
            geometry_world,
        })
    }

    fn advance_progress(
        &mut self,
        rider_world: WorldPoint,
        off_route_enter_distance_m: f64,
        off_route_exit_distance_m: f64,
        major_turn_alert_distance_m: f64,
    ) {
        let projection = project_onto_polyline(&self.geometry_world, rider_world);
        self.progress_distance_m = self
            .progress_distance_m
            .max(projection.progress_distance_m)
            .clamp(0.0, self.total_distance_m);
        self.off_route_distance_m = projection.distance_to_route_m;
        if self.off_route {
            if projection.distance_to_route_m <= off_route_exit_distance_m {
                self.off_route = false;
            }
        } else if projection.distance_to_route_m >= off_route_enter_distance_m {
            self.off_route = true;
        }
        self.upcoming_turn_alert = compute_upcoming_turn_alert_from_stored(
            &self.maneuvers,
            self.progress_distance_m,
            major_turn_alert_distance_m,
        );
    }

    fn snapshot(&self) -> RouteRenderState {
        let (completed_geometry_world, remaining_geometry_world) =
            split_polyline_at_distance(&self.geometry_world, self.progress_distance_m);
        RouteRenderState {
            route_id: Some(self.route_id.clone()),
            revision: Some(self.revision),
            progress_distance_m: Some(self.progress_distance_m),
            off_route: self.off_route,
            off_route_distance_m: Some(self.off_route_distance_m),
            upcoming_turn_alert: self.upcoming_turn_alert.clone(),
            geometry_world: self.geometry_world.clone(),
            completed_geometry_world,
            remaining_geometry_world,
        }
    }
}

impl StoredManeuver {
    fn from_route_maneuver(maneuver: &RouteManeuver, geometry_world: &[WorldPoint]) -> Self {
        let location_world = geo_to_world(&maneuver.location);
        let projection = project_onto_polyline(geometry_world, location_world);
        Self {
            maneuver_type: maneuver.maneuver_type,
            distance_along_route_m: projection.progress_distance_m as f32,
            instruction_text: maneuver.instruction_text.clone(),
        }
    }
}

fn compute_upcoming_turn_alert_from_stored(
    maneuvers: &[StoredManeuver],
    progress_distance_m: f64,
    threshold_m: f64,
) -> Option<UpcomingTurnAlert> {
    for maneuver in maneuvers {
        let Some(kind) = turn_alert_kind(maneuver.maneuver_type) else {
            continue;
        };
        let distance_remaining_m = f64::from(maneuver.distance_along_route_m) - progress_distance_m;
        if distance_remaining_m < 0.0 {
            continue;
        }
        if distance_remaining_m > threshold_m {
            return None;
        }
        return Some(UpcomingTurnAlert {
            kind,
            distance_remaining_m: distance_remaining_m as f32,
            instruction_text: maneuver.instruction_text.clone(),
        });
    }
    None
}

fn turn_alert_kind(maneuver_type: RouteManeuverType) -> Option<TurnAlertKind> {
    match maneuver_type {
        RouteManeuverType::Left | RouteManeuverType::SharpLeft | RouteManeuverType::SlightLeft => {
            Some(TurnAlertKind::Left)
        }
        RouteManeuverType::Right
        | RouteManeuverType::SharpRight
        | RouteManeuverType::SlightRight => Some(TurnAlertKind::Right),
        RouteManeuverType::Uturn => Some(TurnAlertKind::Uturn),
        RouteManeuverType::Roundabout | RouteManeuverType::Merge | RouteManeuverType::Ramp => {
            Some(TurnAlertKind::Generic)
        }
        RouteManeuverType::Depart | RouteManeuverType::Straight | RouteManeuverType::Arrive => None,
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

fn polyline_total_distance(points: &[WorldPoint]) -> f64 {
    points
        .windows(2)
        .map(|segment| distance(segment[0], segment[1]))
        .sum()
}

struct RouteProjection {
    progress_distance_m: f64,
    distance_to_route_m: f64,
}

fn project_onto_polyline(points: &[WorldPoint], rider_world: WorldPoint) -> RouteProjection {
    let mut best_distance_sq = f64::INFINITY;
    let mut best_progress_distance_m = 0.0;
    let mut traversed_distance_m = 0.0;

    for segment in points.windows(2) {
        let [start, end] = segment else {
            continue;
        };
        let segment_length_m = distance(*start, *end);
        if segment_length_m <= f64::EPSILON {
            continue;
        }

        let (projection, t) = project_point_onto_segment(rider_world, *start, *end);
        let dx = rider_world.x_m - projection.x_m;
        let dy = rider_world.y_m - projection.y_m;
        let distance_sq = (dx * dx) + (dy * dy);
        if distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            best_progress_distance_m = traversed_distance_m + (segment_length_m * t);
        }

        traversed_distance_m += segment_length_m;
    }

    RouteProjection {
        progress_distance_m: best_progress_distance_m,
        distance_to_route_m: best_distance_sq.sqrt(),
    }
}

fn split_polyline_at_distance(
    points: &[WorldPoint],
    progress_distance_m: f64,
) -> (Vec<WorldPoint>, Vec<WorldPoint>) {
    if points.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if points.len() == 1 {
        return (vec![points[0]], vec![points[0]]);
    }

    if progress_distance_m <= 0.0 {
        return (vec![points[0]], points.to_vec());
    }

    let total_distance_m = polyline_total_distance(points);
    if progress_distance_m >= total_distance_m {
        return (
            points.to_vec(),
            vec![*points.last().expect("non-empty route")],
        );
    }

    let mut completed = vec![points[0]];
    let mut traversed_distance_m = 0.0;

    for (segment_index, segment) in points.windows(2).enumerate() {
        let [start, end] = segment else {
            continue;
        };
        let segment_length_m = distance(*start, *end);
        if segment_length_m <= f64::EPSILON {
            continue;
        }

        let next_distance_m = traversed_distance_m + segment_length_m;
        if progress_distance_m >= next_distance_m {
            completed.push(*end);
            traversed_distance_m = next_distance_m;
            continue;
        }

        let local_t =
            ((progress_distance_m - traversed_distance_m) / segment_length_m).clamp(0.0, 1.0);
        let split_point = lerp(*start, *end, local_t);
        if completed.last().copied() != Some(split_point) {
            completed.push(split_point);
        }

        let mut remaining = vec![split_point];
        if split_point != *end {
            remaining.push(*end);
        }
        remaining.extend(points.iter().copied().skip(segment_index + 2));
        return (completed, remaining);
    }

    (
        points.to_vec(),
        vec![*points.last().expect("non-empty route")],
    )
}

fn project_point_onto_segment(
    point: WorldPoint,
    start: WorldPoint,
    end: WorldPoint,
) -> (WorldPoint, f64) {
    let vx = end.x_m - start.x_m;
    let vy = end.y_m - start.y_m;
    let segment_length_sq = (vx * vx) + (vy * vy);
    if segment_length_sq <= f64::EPSILON {
        return (start, 0.0);
    }

    let wx = point.x_m - start.x_m;
    let wy = point.y_m - start.y_m;
    let t = ((wx * vx) + (wy * vy)) / segment_length_sq;
    let t = t.clamp(0.0, 1.0);
    (lerp(start, end, t), t)
}

fn distance(a: WorldPoint, b: WorldPoint) -> f64 {
    let dx = b.x_m - a.x_m;
    let dy = b.y_m - a.y_m;
    dx.hypot(dy)
}

fn lerp(start: WorldPoint, end: WorldPoint, t: f64) -> WorldPoint {
    WorldPoint::new(
        start.x_m + ((end.x_m - start.x_m) * t),
        start.y_m + ((end.y_m - start.y_m) * t),
    )
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
        assert_eq!(snapshot.progress_distance_m, Some(0.0));
        assert!(!snapshot.off_route);
        assert_eq!(snapshot.off_route_distance_m, Some(0.0));
        assert!(snapshot.upcoming_turn_alert.is_none());
        assert_eq!(snapshot.geometry_world.len(), 2);
        assert_eq!(snapshot.completed_geometry_world.len(), 1);
        assert_eq!(snapshot.remaining_geometry_world.len(), 2);
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

    #[test]
    fn progress_projection_advances_completed_geometry_without_regressing() {
        let mut state = ActiveRouteState::default();
        state.apply_sync(&RouteSyncMessage::Set(RouteSetMessage {
            route: RoutePackage {
                geometry: vec![
                    GeoPoint::new(60.1699, 24.9384),
                    GeoPoint::new(60.1708, 24.9384),
                    GeoPoint::new(60.1717, 24.9384),
                ],
                ..sample_route("route-2", 1)
            },
        }));

        let first_progress_world = geo_to_world(&GeoPoint::new(60.1704, 24.9384));
        state.advance_progress(first_progress_world, 18.0, 12.0, 80.0);
        let first_snapshot = state.snapshot();

        let second_progress_world = geo_to_world(&GeoPoint::new(60.1713, 24.9384));
        state.advance_progress(second_progress_world, 18.0, 12.0, 80.0);
        let second_snapshot = state.snapshot();

        let backward_jitter_world = geo_to_world(&GeoPoint::new(60.1702, 24.9384));
        state.advance_progress(backward_jitter_world, 18.0, 12.0, 80.0);
        let jitter_snapshot = state.snapshot();

        assert!(first_snapshot.progress_distance_m.unwrap_or_default() > 0.0);
        assert!(
            second_snapshot.progress_distance_m.unwrap_or_default()
                > first_snapshot.progress_distance_m.unwrap_or_default()
        );
        assert_eq!(
            jitter_snapshot.progress_distance_m,
            second_snapshot.progress_distance_m
        );
        assert!(second_snapshot.completed_geometry_world.len() >= 2);
        assert!(second_snapshot.remaining_geometry_world.len() >= 2);
    }

    #[test]
    fn upcoming_major_turn_alert_advances_between_maneuvers() {
        let mut state = ActiveRouteState::default();
        state.apply_sync(&RouteSyncMessage::Set(RouteSetMessage {
            route: RoutePackage {
                geometry: vec![
                    GeoPoint::new(37.7749, -122.4194),
                    GeoPoint::new(37.7756, -122.4188),
                    GeoPoint::new(37.7763, -122.4179),
                ],
                maneuvers: vec![
                    RouteManeuver {
                        id: "depart".to_owned(),
                        maneuver_type: RouteManeuverType::Depart,
                        location: GeoPoint::new(37.7749, -122.4194),
                        distance_from_start_m: 0.0,
                        distance_to_next_m: Some(60.0),
                        instruction_text: Some("Start".to_owned()),
                    },
                    RouteManeuver {
                        id: "right".to_owned(),
                        maneuver_type: RouteManeuverType::Right,
                        location: GeoPoint::new(37.7752, -122.4190),
                        distance_from_start_m: 60.0,
                        distance_to_next_m: Some(70.0),
                        instruction_text: Some("Turn right".to_owned()),
                    },
                    RouteManeuver {
                        id: "left".to_owned(),
                        maneuver_type: RouteManeuverType::Left,
                        location: GeoPoint::new(37.7758, -122.4185),
                        distance_from_start_m: 130.0,
                        distance_to_next_m: None,
                        instruction_text: Some("Turn left".to_owned()),
                    },
                ],
                summary: RouteSummary {
                    total_distance_m: 200.0,
                    estimated_duration_s: 80,
                    start_label: None,
                    destination_label: None,
                },
                provenance: RouteProvenance {
                    provider: RouteProvider::HslDigitransit,
                    source_ref: None,
                    generated_at_unix_ms: 0,
                },
                version: CURRENT_ROUTE_PACKAGE_VERSION,
                route_id: "route-4".to_owned(),
                revision: 1,
            },
        }));

        let start_world = geo_to_world(&GeoPoint::new(37.7749, -122.4194));
        state.advance_progress(start_world, 35.0, 22.0, 80.0);
        let initial = state.snapshot();
        assert_eq!(
            initial.upcoming_turn_alert.as_ref().map(|alert| alert.kind),
            Some(TurnAlertKind::Right)
        );

        let progressed_world = geo_to_world(&GeoPoint::new(37.77555, -122.41885));
        state.advance_progress(progressed_world, 35.0, 22.0, 80.0);
        let progressed = state.snapshot();

        assert_eq!(
            progressed
                .upcoming_turn_alert
                .as_ref()
                .map(|alert| alert.kind),
            Some(TurnAlertKind::Left)
        );
    }

    #[test]
    fn off_route_hysteresis_requires_returning_within_exit_threshold() {
        let mut state = ActiveRouteState::default();
        state.apply_sync(&RouteSyncMessage::Set(RouteSetMessage {
            route: RoutePackage {
                geometry: vec![
                    GeoPoint::new(60.1699, 24.9384),
                    GeoPoint::new(60.1717, 24.9384),
                ],
                ..sample_route("route-3", 1)
            },
        }));

        let far_from_route = geo_to_world(&GeoPoint::new(60.1708, 24.93875));
        state.advance_progress(far_from_route, 18.0, 12.0, 80.0);
        let off_route = state.snapshot();

        let still_outside_exit = geo_to_world(&GeoPoint::new(60.1708, 24.93858));
        state.advance_progress(still_outside_exit, 18.0, 12.0, 80.0);
        let holding = state.snapshot();

        let back_on_route = geo_to_world(&GeoPoint::new(60.1708, 24.93846));
        state.advance_progress(back_on_route, 18.0, 12.0, 80.0);
        let recovered = state.snapshot();

        assert!(off_route.off_route);
        assert!(off_route.off_route_distance_m.unwrap_or_default() >= 18.0);
        assert!(holding.off_route);
        assert!(holding.off_route_distance_m.unwrap_or_default() > 12.0);
        assert!(!recovered.off_route);
        assert!(recovered.off_route_distance_m.unwrap_or_default() < 12.0);
    }
}
