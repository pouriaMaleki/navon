use runtime_core::api::{
    GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage, RoutePackageError,
    RoutePackageVersion, RouteProvenance, RouteProvider, RouteSummary,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpxImportOptions {
    pub route_id_hint: Option<String>,
    pub revision: u64,
    pub source_ref: Option<String>,
    pub generated_at_unix_ms: u64,
}

impl Default for GpxImportOptions {
    fn default() -> Self {
        Self {
            route_id_hint: None,
            revision: 1,
            source_ref: None,
            generated_at_unix_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GpxImportError {
    Xml(String),
    NoUsableGeometry,
    InvalidCoordinate(String),
    InvalidRoute(RoutePackageError),
}

impl core::fmt::Display for GpxImportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Xml(error) => write!(f, "invalid GPX XML: {error}"),
            Self::NoUsableGeometry => write!(
                f,
                "GPX file did not contain a usable route or track geometry"
            ),
            Self::InvalidCoordinate(detail) => {
                write!(f, "GPX contained an invalid coordinate: {detail}")
            }
            Self::InvalidRoute(error) => {
                write!(f, "normalized route package is invalid: {error:?}")
            }
        }
    }
}

impl core::error::Error for GpxImportError {}

#[derive(Debug, Clone, PartialEq)]
struct GpxPoint {
    point: GeoPoint,
    label: Option<String>,
}

pub fn import_gpx_route(
    xml: &str,
    options: &GpxImportOptions,
) -> Result<RoutePackage, GpxImportError> {
    let document =
        roxmltree::Document::parse(xml).map_err(|error| GpxImportError::Xml(error.to_string()))?;
    let metadata_name = first_text_for_path(&document, &["gpx", "metadata", "name"])
        .or_else(|| first_text_for_path(&document, &["gpx", "rte", "name"]))
        .or_else(|| first_text_for_path(&document, &["gpx", "trk", "name"]));

    let route_points = collect_route_points(&document)?;
    let track_points = collect_track_points(&document)?;

    let (geometry_points, prefer_point_labels) = if route_points.len() >= 2 {
        (dedupe_adjacent(route_points), true)
    } else if track_points.len() >= 2 {
        (dedupe_adjacent(track_points), false)
    } else {
        return Err(GpxImportError::NoUsableGeometry);
    };

    if geometry_points.len() < 2 {
        return Err(GpxImportError::NoUsableGeometry);
    }

    let geometry: Vec<GeoPoint> = geometry_points
        .iter()
        .map(|entry| entry.point.clone())
        .collect();
    let cumulative = cumulative_distances(&geometry);
    let total_distance = *cumulative.last().unwrap_or(&0.0);
    let maneuvers = build_maneuvers(&geometry_points, &cumulative, prefer_point_labels);

    let route_name = metadata_name
        .or_else(|| options.route_id_hint.clone())
        .or_else(|| options.source_ref.clone())
        .unwrap_or_else(|| "GPX import".to_owned());

    let route = RoutePackage {
        version: RoutePackageVersion::new(1, 0),
        route_id: slugify(&route_name),
        revision: options.revision,
        geometry,
        maneuvers,
        summary: RouteSummary {
            total_distance_m: total_distance as f32,
            estimated_duration_s: ((total_distance / 5.0).max(60.0)).round() as u32,
            start_label: geometry_points
                .first()
                .and_then(|point| point.label.clone()),
            destination_label: geometry_points
                .last()
                .and_then(|point| point.label.clone())
                .or(Some(route_name.clone())),
        },
        provenance: RouteProvenance {
            provider: RouteProvider::Gpx,
            source_ref: options.source_ref.clone(),
            generated_at_unix_ms: options.generated_at_unix_ms,
        },
    };

    route.validate().map_err(GpxImportError::InvalidRoute)?;
    Ok(route)
}

fn collect_route_points(
    document: &roxmltree::Document<'_>,
) -> Result<Vec<GpxPoint>, GpxImportError> {
    let mut points = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("rtept"))
    {
        points.push(parse_point(node)?);
    }
    Ok(points)
}

fn collect_track_points(
    document: &roxmltree::Document<'_>,
) -> Result<Vec<GpxPoint>, GpxImportError> {
    let mut points = Vec::new();
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("trkpt"))
    {
        points.push(parse_point(node)?);
    }
    Ok(points)
}

fn parse_point(node: roxmltree::Node<'_, '_>) -> Result<GpxPoint, GpxImportError> {
    let lat = node
        .attribute("lat")
        .ok_or_else(|| GpxImportError::InvalidCoordinate("missing lat attribute".to_owned()))?
        .parse::<f64>()
        .map_err(|_| GpxImportError::InvalidCoordinate("invalid lat attribute".to_owned()))?;
    let lon = node
        .attribute("lon")
        .ok_or_else(|| GpxImportError::InvalidCoordinate("missing lon attribute".to_owned()))?
        .parse::<f64>()
        .map_err(|_| GpxImportError::InvalidCoordinate("invalid lon attribute".to_owned()))?;

    let point = GeoPoint::new(lat, lon);
    if !point.is_valid() {
        return Err(GpxImportError::InvalidCoordinate(format!(
            "lat={lat}, lon={lon}"
        )));
    }

    let label = child_text(node, "name")
        .or_else(|| child_text(node, "desc"))
        .or_else(|| child_text(node, "cmt"));

    Ok(GpxPoint { point, label })
}

fn child_text(node: roxmltree::Node<'_, '_>, tag_name: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(tag_name))
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn first_text_for_path(document: &roxmltree::Document<'_>, path: &[&str]) -> Option<String> {
    let mut current = document.root_element();
    let mut iter = path.iter();
    match iter.next() {
        Some(root) if current.has_tag_name(*root) => {}
        _ => return None,
    }

    for name in iter {
        current = current.children().find(|child| child.has_tag_name(*name))?;
    }

    current
        .text()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn dedupe_adjacent(points: Vec<GpxPoint>) -> Vec<GpxPoint> {
    let mut output = Vec::new();
    for point in points {
        if output
            .last()
            .map(|previous: &GpxPoint| previous.point == point.point)
            .unwrap_or(false)
        {
            continue;
        }
        output.push(point);
    }
    output
}

fn cumulative_distances(geometry: &[GeoPoint]) -> Vec<f64> {
    let mut cumulative = vec![0.0];
    for segment in geometry.windows(2) {
        cumulative.push(
            cumulative.last().copied().unwrap_or(0.0)
                + approximate_distance_m(&segment[0], &segment[1]),
        );
    }
    cumulative
}

fn build_maneuvers(
    points: &[GpxPoint],
    cumulative: &[f64],
    prefer_point_labels: bool,
) -> Vec<RouteManeuver> {
    let mut maneuvers = Vec::new();
    maneuvers.push(RouteManeuver {
        id: "depart".to_owned(),
        maneuver_type: RouteManeuverType::Depart,
        location: points
            .first()
            .map(|point| point.point.clone())
            .unwrap_or_else(|| GeoPoint::new(0.0, 0.0)),
        distance_from_start_m: 0.0,
        distance_to_next_m: cumulative.get(1).copied().map(|value| value as f32),
        instruction_text: Some("Start riding".to_owned()),
    });

    for index in 1..points.len().saturating_sub(1) {
        let turn = classify_turn(
            &points[index - 1].point,
            &points[index].point,
            &points[index + 1].point,
        );
        let point_label = points[index].label.clone();
        if turn.is_none() && !(prefer_point_labels && point_label.is_some()) {
            continue;
        }

        let (maneuver_type, instruction_text) = match turn {
            Some((kind, instruction)) => (kind, point_label.or(Some(instruction))),
            None => (RouteManeuverType::Straight, point_label),
        };

        maneuvers.push(RouteManeuver {
            id: format!("step-{index}"),
            maneuver_type,
            location: points[index].point.clone(),
            distance_from_start_m: cumulative.get(index).copied().unwrap_or_default() as f32,
            distance_to_next_m: cumulative
                .get(index + 1)
                .zip(cumulative.get(index))
                .map(|(next, current)| (next - current) as f32),
            instruction_text,
        });
    }

    maneuvers.push(RouteManeuver {
        id: "arrive".to_owned(),
        maneuver_type: RouteManeuverType::Arrive,
        location: points
            .last()
            .map(|point| point.point.clone())
            .unwrap_or_else(|| GeoPoint::new(0.0, 0.0)),
        distance_from_start_m: cumulative.last().copied().unwrap_or_default() as f32,
        distance_to_next_m: None,
        instruction_text: Some("Arrive at destination".to_owned()),
    });

    maneuvers
}

fn classify_turn(
    previous: &GeoPoint,
    current: &GeoPoint,
    next: &GeoPoint,
) -> Option<(RouteManeuverType, String)> {
    let delta = turn_delta_degrees(previous, current, next);
    let magnitude = delta.abs();
    if magnitude < 25.0 {
        return None;
    }
    if magnitude >= 170.0 {
        return Some((RouteManeuverType::Uturn, "Make a U-turn".to_owned()));
    }
    if magnitude >= 110.0 {
        return Some(if delta > 0.0 {
            (
                RouteManeuverType::SharpRight,
                "Turn sharply right".to_owned(),
            )
        } else {
            (RouteManeuverType::SharpLeft, "Turn sharply left".to_owned())
        });
    }
    if magnitude >= 50.0 {
        return Some(if delta > 0.0 {
            (RouteManeuverType::Right, "Turn right".to_owned())
        } else {
            (RouteManeuverType::Left, "Turn left".to_owned())
        });
    }
    Some(if delta > 0.0 {
        (RouteManeuverType::SlightRight, "Bear right".to_owned())
    } else {
        (RouteManeuverType::SlightLeft, "Bear left".to_owned())
    })
}

fn turn_delta_degrees(previous: &GeoPoint, current: &GeoPoint, next: &GeoPoint) -> f64 {
    let incoming = bearing_degrees(previous, current);
    let outgoing = bearing_degrees(current, next);
    let mut delta = outgoing - incoming;
    while delta <= -180.0 {
        delta += 360.0;
    }
    while delta > 180.0 {
        delta -= 360.0;
    }
    delta
}

fn bearing_degrees(start: &GeoPoint, end: &GeoPoint) -> f64 {
    let lat_m = (end.lat_deg - start.lat_deg) * 111_320.0;
    let lon_m = (end.lon_deg - start.lon_deg)
        * (((start.lat_deg + end.lat_deg) / 2.0).to_radians().cos())
        * 111_320.0;
    lon_m.atan2(lat_m).to_degrees()
}

fn approximate_distance_m(start: &GeoPoint, end: &GeoPoint) -> f64 {
    let lat_m = (end.lat_deg - start.lat_deg) * 111_320.0;
    let lon_m = (end.lon_deg - start.lon_deg)
        * (((start.lat_deg + end.lat_deg) / 2.0).to_radians().cos())
        * 111_320.0;
    (lat_m * lat_m + lon_m * lon_m).sqrt()
}

fn slugify(input: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;
    for character in input.chars() {
        let next = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '-'
        };
        if next == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            output.push(next);
        } else {
            previous_dash = false;
            output.push(next);
        }
    }
    let trimmed = output.trim_matches('-');
    if trimmed.is_empty() {
        "gpx-import".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK_GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test">
  <trk>
    <name>Morning Ride</name>
    <trkseg>
      <trkpt lat="60.1699" lon="24.9384" />
      <trkpt lat="60.1704" lon="24.9384" />
      <trkpt lat="60.1704" lon="24.9390" />
      <trkpt lat="60.1709" lon="24.9390" />
    </trkseg>
  </trk>
</gpx>"#;

    const ROUTE_GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test">
  <metadata>
    <name>Commute In</name>
  </metadata>
  <rte>
    <name>Commute In</name>
    <rtept lat="60.1699" lon="24.9384"><name>Start</name></rtept>
    <rtept lat="60.1705" lon="24.9384"><desc>Head north</desc></rtept>
    <rtept lat="60.1705" lon="24.9391"><desc>Turn right onto lane</desc></rtept>
    <rtept lat="60.1710" lon="24.9391"><name>Office</name></rtept>
  </rte>
</gpx>"#;

    #[test]
    fn imports_track_geometry_into_route_package() {
        let route = import_gpx_route(TRACK_GPX, &GpxImportOptions::default())
            .expect("track GPX should import");
        assert_eq!(route.provenance.provider, RouteProvider::Gpx);
        assert_eq!(route.geometry.len(), 4);
        assert_eq!(route.route_id, "morning-ride");
        assert_eq!(
            route
                .maneuvers
                .first()
                .map(|maneuver| maneuver.maneuver_type),
            Some(RouteManeuverType::Depart)
        );
        assert_eq!(
            route
                .maneuvers
                .last()
                .map(|maneuver| maneuver.maneuver_type),
            Some(RouteManeuverType::Arrive)
        );
        assert!(
            route
                .maneuvers
                .iter()
                .any(|maneuver| maneuver.maneuver_type == RouteManeuverType::Right)
        );
    }

    #[test]
    fn prefers_route_points_and_preserves_labels() {
        let route = import_gpx_route(
            ROUTE_GPX,
            &GpxImportOptions {
                source_ref: Some("commute.gpx".to_owned()),
                generated_at_unix_ms: 1234,
                ..GpxImportOptions::default()
            },
        )
        .expect("route GPX should import");
        assert_eq!(route.geometry.len(), 4);
        assert_eq!(route.summary.start_label.as_deref(), Some("Start"));
        assert_eq!(route.summary.destination_label.as_deref(), Some("Office"));
        assert!(
            route.maneuvers.iter().any(
                |maneuver| maneuver.instruction_text.as_deref() == Some("Turn right onto lane")
            )
        );
        assert_eq!(route.provenance.source_ref.as_deref(), Some("commute.gpx"));
    }

    #[test]
    fn rejects_invalid_gpx() {
        let error = import_gpx_route("<gpx>", &GpxImportOptions::default())
            .expect_err("invalid GPX must fail");
        assert!(matches!(error, GpxImportError::Xml(_)));
    }

    #[test]
    fn rejects_gpx_without_enough_points() {
        let error = import_gpx_route(
            r#"<gpx version="1.1" creator="test"><trk><trkseg><trkpt lat="60.1" lon="24.9" /></trkseg></trk></gpx>"#,
            &GpxImportOptions::default(),
        )
        .expect_err("single-point GPX must fail");
        assert!(matches!(
            error,
            GpxImportError::NoUsableGeometry
                | GpxImportError::InvalidRoute(RoutePackageError::InsufficientGeometryPoints(_))
        ));
    }
}
