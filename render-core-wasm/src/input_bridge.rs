use std::time::Duration;

use serde::Deserialize;

use runtime_core::api::{
    GeoPoint, GpsSample, RouteClearMessage, RouteManeuver, RouteManeuverType, RoutePackage,
    RoutePackageVersion, RouteProvenance, RouteProvider, RouteSetMessage, RouteSummary,
    RouteSyncMessage, RouteUpdateMessage, RuntimeInputFrame, ScreenPoint, TouchContact,
    TouchContactFrame, TouchContactFrameError, TouchPhase, ViewportSize,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserTouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub x_px: f32,
    pub y_px: f32,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputBridge;

#[derive(Debug, Clone, Deserialize)]
struct JsonFrameInput {
    viewport: JsonViewportSize,
    #[serde(default)]
    gps: Option<JsonGpsSample>,
    #[serde(default)]
    touch: Option<JsonTouchFrame>,
    #[serde(default, rename = "routeSync")]
    route_sync: Option<JsonRouteSyncMessage>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonViewportSize {
    #[serde(rename = "widthPx")]
    width_px: u32,
    #[serde(rename = "heightPx")]
    height_px: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonGpsSample {
    #[serde(rename = "latDeg")]
    lat_deg: f64,
    #[serde(rename = "lonDeg")]
    lon_deg: f64,
    #[serde(rename = "speedMps")]
    speed_mps: f32,
    #[serde(rename = "courseRad")]
    course_rad: Option<f32>,
    #[serde(rename = "horizontalAccuracyM")]
    horizontal_accuracy_m: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonTouchFrame {
    sequence: u64,
    contacts: Vec<JsonTouchContact>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonTouchContact {
    id: u64,
    phase: String,
    #[serde(rename = "xPx")]
    x_px: f32,
    #[serde(rename = "yPx")]
    y_px: f32,
    pressure: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JsonRouteSyncMessage {
    Set {
        route: JsonRoutePackage,
    },
    Update {
        #[serde(rename = "routeId")]
        route_id: String,
        revision: u64,
        route: JsonRoutePackage,
    },
    Clear {
        #[serde(default, rename = "routeId")]
        route_id: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRoutePackage {
    version: JsonRoutePackageVersion,
    #[serde(rename = "routeId")]
    route_id: String,
    revision: u64,
    geometry: Vec<JsonGeoPoint>,
    maneuvers: Vec<JsonRouteManeuver>,
    summary: JsonRouteSummary,
    provenance: JsonRouteProvenance,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRoutePackageVersion {
    major: u16,
    minor: u16,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonGeoPoint {
    #[serde(rename = "latDeg")]
    lat_deg: f64,
    #[serde(rename = "lonDeg")]
    lon_deg: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRouteManeuver {
    id: String,
    #[serde(rename = "maneuverType")]
    maneuver_type: String,
    location: JsonGeoPoint,
    #[serde(rename = "distanceFromStartM")]
    distance_from_start_m: f32,
    #[serde(rename = "distanceToNextM")]
    distance_to_next_m: Option<f32>,
    #[serde(default, rename = "instructionText")]
    instruction_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRouteSummary {
    #[serde(rename = "totalDistanceM")]
    total_distance_m: f32,
    #[serde(rename = "estimatedDurationS")]
    estimated_duration_s: u32,
    #[serde(default, rename = "startLabel")]
    start_label: Option<String>,
    #[serde(default, rename = "destinationLabel")]
    destination_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRouteProvenance {
    provider: String,
    #[serde(default, rename = "sourceRef")]
    source_ref: Option<String>,
    #[serde(rename = "generatedAtUnixMs")]
    generated_at_unix_ms: u64,
}

impl InputBridge {
    pub fn frame_from_json(
        &self,
        dt_ms: f64,
        frame_json: &str,
    ) -> Result<RuntimeInputFrame, String> {
        let json_frame: JsonFrameInput = serde_json::from_str(frame_json)
            .map_err(|error| format!("invalid frame json: {error}"))?;
        let viewport =
            ViewportSize::new(json_frame.viewport.width_px, json_frame.viewport.height_px);
        let gps = json_frame.gps.map(|gps| GpsSample {
            lat_deg: gps.lat_deg,
            lon_deg: gps.lon_deg,
            speed_mps: gps.speed_mps,
            course_rad: gps.course_rad,
            horizontal_accuracy_m: gps.horizontal_accuracy_m,
        });
        let route_sync = json_frame
            .route_sync
            .map(route_sync_from_json)
            .transpose()?;

        let contacts = json_frame
            .touch
            .map(|touch| {
                touch
                    .contacts
                    .into_iter()
                    .map(|contact| {
                        Ok(BrowserTouchContact {
                            id: contact.id,
                            phase: parse_touch_phase(&contact.phase)?,
                            x_px: contact.x_px,
                            y_px: contact.y_px,
                            pressure: contact.pressure,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map(|contacts| (touch.sequence, contacts))
            })
            .transpose()?;

        let mut frame = if let Some((sequence, contacts)) = contacts {
            self.frame_from_browser(
                Duration::from_secs_f64((dt_ms.max(0.0)) / 1000.0),
                viewport,
                gps,
                sequence,
                contacts,
            )
            .map_err(|error| format!("invalid touch frame: {error:?}"))?
        } else {
            let frame = RuntimeInputFrame::new(Duration::from_secs_f64((dt_ms.max(0.0)) / 1000.0))
                .with_viewport(viewport);
            if let Some(gps) = gps {
                frame.with_gps(gps)
            } else {
                frame
            }
        };

        if let Some(route_sync) = route_sync {
            frame = frame.with_route_sync(route_sync);
        }

        Ok(frame)
    }

    pub fn frame_from_browser(
        &self,
        dt: Duration,
        viewport_size: ViewportSize,
        gps: Option<GpsSample>,
        sequence: u64,
        contacts: Vec<BrowserTouchContact>,
    ) -> Result<RuntimeInputFrame, TouchContactFrameError> {
        let touch = TouchContactFrame::new(
            sequence,
            contacts
                .into_iter()
                .map(|contact| TouchContact {
                    id: contact.id,
                    phase: contact.phase,
                    position: ScreenPoint::new(contact.x_px, contact.y_px),
                    pressure: contact.pressure,
                })
                .collect(),
        )?;

        let frame = RuntimeInputFrame::new(dt).with_viewport(viewport_size);
        let frame = if let Some(gps) = gps {
            frame.with_gps(gps)
        } else {
            frame
        };
        Ok(frame.with_touch(touch))
    }
}

fn route_sync_from_json(message: JsonRouteSyncMessage) -> Result<RouteSyncMessage, String> {
    match message {
        JsonRouteSyncMessage::Set { route } => Ok(RouteSyncMessage::Set(RouteSetMessage {
            route: route_from_json(route)?,
        })),
        JsonRouteSyncMessage::Update {
            route_id,
            revision,
            route,
        } => Ok(RouteSyncMessage::Update(RouteUpdateMessage {
            route_id,
            revision,
            route: route_from_json(route)?,
        })),
        JsonRouteSyncMessage::Clear { route_id } => {
            Ok(RouteSyncMessage::Clear(RouteClearMessage { route_id }))
        }
    }
}

fn route_from_json(raw: JsonRoutePackage) -> Result<RoutePackage, String> {
    let maneuvers = raw
        .maneuvers
        .into_iter()
        .map(maneuver_from_json)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RoutePackage {
        version: RoutePackageVersion::new(raw.version.major, raw.version.minor),
        route_id: raw.route_id,
        revision: raw.revision,
        geometry: raw.geometry.into_iter().map(geo_point_from_json).collect(),
        maneuvers,
        summary: RouteSummary {
            total_distance_m: raw.summary.total_distance_m,
            estimated_duration_s: raw.summary.estimated_duration_s,
            start_label: raw.summary.start_label,
            destination_label: raw.summary.destination_label,
        },
        provenance: RouteProvenance {
            provider: route_provider_from_str(&raw.provenance.provider),
            source_ref: raw.provenance.source_ref,
            generated_at_unix_ms: raw.provenance.generated_at_unix_ms,
        },
    })
}

fn geo_point_from_json(point: JsonGeoPoint) -> GeoPoint {
    GeoPoint::new(point.lat_deg, point.lon_deg)
}

fn maneuver_from_json(raw: JsonRouteManeuver) -> Result<RouteManeuver, String> {
    Ok(RouteManeuver {
        id: raw.id,
        maneuver_type: route_maneuver_type_from_str(&raw.maneuver_type)?,
        location: geo_point_from_json(raw.location),
        distance_from_start_m: raw.distance_from_start_m,
        distance_to_next_m: raw.distance_to_next_m,
        instruction_text: raw.instruction_text,
    })
}

fn route_maneuver_type_from_str(raw: &str) -> Result<RouteManeuverType, String> {
    let maneuver_type = match raw {
        "depart" => RouteManeuverType::Depart,
        "straight" => RouteManeuverType::Straight,
        "slight_left" => RouteManeuverType::SlightLeft,
        "left" => RouteManeuverType::Left,
        "sharp_left" => RouteManeuverType::SharpLeft,
        "slight_right" => RouteManeuverType::SlightRight,
        "right" => RouteManeuverType::Right,
        "sharp_right" => RouteManeuverType::SharpRight,
        "uturn" => RouteManeuverType::Uturn,
        "roundabout" => RouteManeuverType::Roundabout,
        "merge" => RouteManeuverType::Merge,
        "ramp" => RouteManeuverType::Ramp,
        "arrive" => RouteManeuverType::Arrive,
        _ => return Err(format!("unsupported maneuver type: {raw}")),
    };
    Ok(maneuver_type)
}

fn route_provider_from_str(raw: &str) -> RouteProvider {
    match raw {
        "hsl_digitransit" => RouteProvider::HslDigitransit,
        "osm" => RouteProvider::Osm,
        "gpx" => RouteProvider::Gpx,
        "fit" => RouteProvider::Fit,
        "tcx" => RouteProvider::Tcx,
        _ => RouteProvider::Unknown(raw.to_owned()),
    }
}

fn parse_touch_phase(raw: &str) -> Result<TouchPhase, String> {
    match raw {
        "started" => Ok(TouchPhase::Started),
        "moved" => Ok(TouchPhase::Moved),
        "stationary" => Ok(TouchPhase::Stationary),
        "ended" => Ok(TouchPhase::Ended),
        "cancelled" => Ok(TouchPhase::Cancelled),
        _ => Err(format!("unsupported touch phase: {raw}")),
    }
}
