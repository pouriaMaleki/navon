use std::collections::BTreeMap;

use runtime_core::api::{
    GeoPoint, RouteClearMessage, RouteManeuver, RouteManeuverType, RoutePackage, RoutePackageError,
    RoutePackageVersion, RouteProvenance, RouteProvider, RouteRerouteRequestMessage,
    RouteSetMessage, RouteStatusMessage, RouteSummary, RouteSyncMessage, RouteSyncStatusCode,
    RouteUpdateMessage,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTransferChunk {
    pub transfer_id: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub checksum_hex: String,
    pub payload_fragment: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RouteSyncTransportError {
    EmptyTransferId,
    InvalidChunkIndex {
        chunk_index: u32,
        total_chunks: u32,
    },
    ConflictingChunkData {
        transfer_id: String,
        chunk_index: u32,
    },
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    Utf8Payload,
    MissingField(&'static str),
    InvalidField {
        field: &'static str,
        value: String,
    },
    InvalidMessageKind(String),
    RoutePackage(RoutePackageError),
}

impl From<RoutePackageError> for RouteSyncTransportError {
    fn from(value: RoutePackageError) -> Self {
        Self::RoutePackage(value)
    }
}

impl RouteSyncTransportError {
    pub fn status_code(&self) -> RouteSyncStatusCode {
        match self {
            Self::EmptyTransferId
            | Self::InvalidChunkIndex { .. }
            | Self::ConflictingChunkData { .. }
            | Self::ChecksumMismatch { .. } => RouteSyncStatusCode::RetryableFailure,
            Self::Utf8Payload
            | Self::MissingField(_)
            | Self::InvalidField { .. }
            | Self::InvalidMessageKind(_)
            | Self::RoutePackage(_) => RouteSyncStatusCode::FatalFailure,
        }
    }

    pub fn detail_message(&self) -> String {
        match self {
            Self::EmptyTransferId => {
                "Rejected route transfer chunk with an empty transfer id".to_owned()
            }
            Self::InvalidChunkIndex {
                chunk_index,
                total_chunks,
            } => format!(
                "Rejected route transfer chunk index {} for total chunk count {}",
                chunk_index, total_chunks
            ),
            Self::ConflictingChunkData {
                transfer_id,
                chunk_index,
            } => format!(
                "Conflicting payload received for transfer {} chunk {}",
                transfer_id, chunk_index
            ),
            Self::ChecksumMismatch { expected, actual } => format!(
                "Route transfer checksum mismatch: expected {}, got {}",
                expected, actual
            ),
            Self::Utf8Payload => "Route transfer payload was not valid UTF-8".to_owned(),
            Self::MissingField(field) => {
                format!("Route transfer payload is missing required field {field}")
            }
            Self::InvalidField { field, value } => {
                format!("Route transfer payload field {field} had invalid value {value}")
            }
            Self::InvalidMessageKind(kind) => {
                format!("Route transfer payload uses unsupported message kind {kind}")
            }
            Self::RoutePackage(error) => {
                format!("Route package validation failed: {error:?}")
            }
        }
    }

    pub fn as_status_message(&self) -> RouteStatusMessage {
        RouteStatusMessage {
            route_id: None,
            revision: None,
            status: self.status_code(),
            detail: Some(self.detail_message()),
        }
    }
}

#[derive(Debug, Default)]
pub struct RouteSyncTransport {
    active_route_id: Option<String>,
    active_route_revision: Option<u64>,
    active_route_checksum_hex: Option<String>,
    pending_transfer: Option<PendingTransfer>,
    pending_apply: Option<PendingApply>,
}

impl RouteSyncTransport {
    pub fn ingest_chunk(
        &mut self,
        chunk: RouteTransferChunk,
    ) -> Result<Vec<RouteStatusMessage>, RouteSyncTransportError> {
        if chunk.transfer_id.trim().is_empty() {
            return Err(RouteSyncTransportError::EmptyTransferId);
        }
        if chunk.total_chunks == 0 || chunk.chunk_index >= chunk.total_chunks {
            return Err(RouteSyncTransportError::InvalidChunkIndex {
                chunk_index: chunk.chunk_index,
                total_chunks: chunk.total_chunks,
            });
        }

        let needs_reset = self
            .pending_transfer
            .as_ref()
            .map(|pending| pending.transfer_id != chunk.transfer_id)
            .unwrap_or(true);
        if needs_reset {
            self.pending_transfer = Some(PendingTransfer::new(
                chunk.transfer_id.clone(),
                chunk.total_chunks,
                chunk.checksum_hex.clone(),
            ));
        }

        let pending = self.pending_transfer.as_mut().expect("pending transfer");
        pending.insert_chunk(chunk)?;
        if !pending.is_complete() {
            return Ok(Vec::new());
        }

        let pending = self.pending_transfer.take().expect("complete transfer");
        let payload = pending.assembled_payload();
        let actual_checksum = checksum_hex(&payload);
        if actual_checksum != pending.checksum_hex {
            return Err(RouteSyncTransportError::ChecksumMismatch {
                expected: pending.checksum_hex,
                actual: actual_checksum,
            });
        }
        let message = decode_sync_message(&payload)?;
        self.accept_message(message, actual_checksum)
    }

    pub fn take_pending_runtime_message(&mut self) -> Option<RouteSyncMessage> {
        self.pending_apply
            .as_ref()
            .map(|pending| pending.message.clone())
    }

    pub fn complete_applied_message(&mut self) -> Option<RouteStatusMessage> {
        let pending = self.pending_apply.take()?;
        match &pending.message {
            RouteSyncMessage::Set(set) => {
                self.active_route_id = Some(set.route.route_id.clone());
                self.active_route_revision = Some(set.route.revision);
                self.active_route_checksum_hex = Some(pending.checksum_hex.clone());
                Some(RouteStatusMessage {
                    route_id: Some(set.route.route_id.clone()),
                    revision: Some(set.route.revision),
                    status: RouteSyncStatusCode::Active,
                    detail: Some(format!(
                        "Route revision {} applied on device",
                        set.route.revision
                    )),
                })
            }
            RouteSyncMessage::Update(update) => {
                self.active_route_id = Some(update.route.route_id.clone());
                self.active_route_revision = Some(update.route.revision);
                self.active_route_checksum_hex = Some(pending.checksum_hex.clone());
                Some(RouteStatusMessage {
                    route_id: Some(update.route.route_id.clone()),
                    revision: Some(update.route.revision),
                    status: RouteSyncStatusCode::Active,
                    detail: Some(format!(
                        "Route revision {} applied on device",
                        update.route.revision
                    )),
                })
            }
            RouteSyncMessage::Clear(clear) => {
                self.active_route_id = None;
                self.active_route_revision = None;
                self.active_route_checksum_hex = None;
                Some(RouteStatusMessage {
                    route_id: clear.route_id.clone(),
                    revision: None,
                    status: RouteSyncStatusCode::Cleared,
                    detail: Some("Device cleared active route".to_owned()),
                })
            }
            RouteSyncMessage::Status(_) | RouteSyncMessage::RerouteRequest(_) => {
                Some(RouteStatusMessage {
                    route_id: route_id_of_message(&pending.message),
                    revision: route_revision_of_message(&pending.message),
                    status: RouteSyncStatusCode::FatalFailure,
                    detail: Some("Unsupported inbound route sync message".to_owned()),
                })
            }
        }
    }

    pub fn active_route_id(&self) -> Option<&str> {
        self.active_route_id.as_deref()
    }

    pub fn active_route_revision(&self) -> Option<u64> {
        self.active_route_revision
    }

    pub fn active_route_checksum_hex(&self) -> Option<&str> {
        self.active_route_checksum_hex.as_deref()
    }

    fn accept_message(
        &mut self,
        message: RouteSyncMessage,
        checksum_hex: String,
    ) -> Result<Vec<RouteStatusMessage>, RouteSyncTransportError> {
        match message.clone() {
            RouteSyncMessage::Set(set) => {
                self.accept_route_message(&set.route, message, checksum_hex, "set")
            }
            RouteSyncMessage::Update(update) => {
                self.accept_route_message(&update.route, message, checksum_hex, "update")
            }
            RouteSyncMessage::Clear(clear) => {
                self.pending_apply = Some(PendingApply {
                    message,
                    checksum_hex,
                });
                Ok(vec![
                    RouteStatusMessage {
                        route_id: clear.route_id.clone(),
                        revision: None,
                        status: RouteSyncStatusCode::Accepted,
                        detail: Some("Clear request accepted".to_owned()),
                    },
                    RouteStatusMessage {
                        route_id: clear.route_id.clone(),
                        revision: None,
                        status: RouteSyncStatusCode::Applying,
                        detail: Some("Clearing active route on device".to_owned()),
                    },
                ])
            }
            RouteSyncMessage::Status(status) => Ok(vec![RouteStatusMessage {
                route_id: status.route_id.clone(),
                revision: status.revision,
                status: RouteSyncStatusCode::FatalFailure,
                detail: Some("Device does not accept inbound status messages".to_owned()),
            }]),
            RouteSyncMessage::RerouteRequest(request) => Ok(vec![RouteStatusMessage {
                route_id: request.route_id.clone(),
                revision: None,
                status: RouteSyncStatusCode::FatalFailure,
                detail: Some("Device does not accept inbound reroute requests".to_owned()),
            }]),
        }
    }

    fn accept_route_message(
        &mut self,
        route: &RoutePackage,
        message: RouteSyncMessage,
        checksum_hex: String,
        kind_label: &str,
    ) -> Result<Vec<RouteStatusMessage>, RouteSyncTransportError> {
        route.validate()?;

        if self.active_route_id.as_deref() == Some(route.route_id.as_str()) {
            if let Some(active_revision) = self.active_route_revision {
                if route.revision < active_revision {
                    return Ok(vec![RouteStatusMessage {
                        route_id: Some(route.route_id.clone()),
                        revision: Some(route.revision),
                        status: RouteSyncStatusCode::Rejected,
                        detail: Some(format!(
                            "Rejected stale route revision {}; device already has rev {}",
                            route.revision, active_revision
                        )),
                    }]);
                }

                if route.revision == active_revision {
                    if self.active_route_checksum_hex.as_deref() == Some(checksum_hex.as_str()) {
                        return Ok(vec![RouteStatusMessage {
                            route_id: Some(route.route_id.clone()),
                            revision: Some(route.revision),
                            status: RouteSyncStatusCode::Active,
                            detail: Some(format!(
                                "Duplicate {} replay deduped; existing route kept active",
                                kind_label
                            )),
                        }]);
                    }
                    return Ok(vec![RouteStatusMessage {
                        route_id: Some(route.route_id.clone()),
                        revision: Some(route.revision),
                        status: RouteSyncStatusCode::FatalFailure,
                        detail: Some(format!(
                            "Revision conflict: route {} rev {} has a different checksum",
                            route.route_id, route.revision
                        )),
                    }]);
                }
            }
        }

        self.pending_apply = Some(PendingApply {
            message,
            checksum_hex,
        });
        Ok(vec![
            RouteStatusMessage {
                route_id: Some(route.route_id.clone()),
                revision: Some(route.revision),
                status: RouteSyncStatusCode::Accepted,
                detail: Some(format!("{} payload accepted", kind_label)),
            },
            RouteStatusMessage {
                route_id: Some(route.route_id.clone()),
                revision: Some(route.revision),
                status: RouteSyncStatusCode::Applying,
                detail: Some(format!(
                    "Applying route revision {} on device",
                    route.revision
                )),
            },
        ])
    }
}

pub const DEFAULT_TRANSFER_CHUNK_SIZE: usize = 96;

pub fn chunk_sync_message(
    message: &RouteSyncMessage,
    transfer_id: &str,
    chunk_size: usize,
) -> Vec<RouteTransferChunk> {
    let payload = encode_sync_message(message).into_bytes();
    let checksum = checksum_hex(&payload);
    let chunk_size = chunk_size.max(1);
    let total_chunks = payload.len().div_ceil(chunk_size) as u32;
    payload
        .chunks(chunk_size)
        .enumerate()
        .map(|(index, fragment)| RouteTransferChunk {
            transfer_id: transfer_id.to_owned(),
            chunk_index: index as u32,
            total_chunks,
            checksum_hex: checksum.clone(),
            payload_fragment: fragment.to_vec(),
        })
        .collect()
}

pub fn decode_sync_message(payload: &[u8]) -> Result<RouteSyncMessage, RouteSyncTransportError> {
    let payload_text =
        String::from_utf8(payload.to_vec()).map_err(|_| RouteSyncTransportError::Utf8Payload)?;
    parse_sync_message(&payload_text)
}

pub fn encode_sync_message(message: &RouteSyncMessage) -> String {
    match message {
        RouteSyncMessage::Set(set) => encode_route_message("set", &set.route),
        RouteSyncMessage::Update(update) => encode_route_message("update", &update.route),
        RouteSyncMessage::Clear(clear) => [
            "kind=clear".to_owned(),
            format!(
                "route_id={}",
                clear
                    .route_id
                    .clone()
                    .unwrap_or_else(|| "current".to_owned())
            ),
        ]
        .join("\n"),
        RouteSyncMessage::Status(status) => [
            "kind=status".to_owned(),
            format!(
                "route_id={}",
                status.route_id.clone().unwrap_or_else(|| "none".to_owned())
            ),
            format!(
                "revision={}",
                status
                    .revision
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_owned())
            ),
            format!("status={}", encode_status(status.status)),
            format!("detail={}", status.detail.clone().unwrap_or_default()),
        ]
        .join("\n"),
        RouteSyncMessage::RerouteRequest(request) => [
            "kind=reroute_request".to_owned(),
            format!(
                "route_id={}",
                request
                    .route_id
                    .clone()
                    .unwrap_or_else(|| "none".to_owned())
            ),
            format!(
                "rider={:.6},{:.6}",
                request.rider_position.lat_deg, request.rider_position.lon_deg
            ),
            format!("reason={}", request.reason),
        ]
        .join("\n"),
    }
}

fn encode_route_message(kind: &str, route: &RoutePackage) -> String {
    let geometry = route
        .geometry
        .iter()
        .map(|point| format!("{:.6},{:.6}", point.lat_deg, point.lon_deg))
        .collect::<Vec<_>>()
        .join(";");
    let maneuvers = route
        .maneuvers
        .iter()
        .map(|maneuver| {
            [
                maneuver.id.clone(),
                encode_maneuver_type(maneuver.maneuver_type).to_owned(),
                format!("{:.1}", maneuver.distance_from_start_m),
                maneuver
                    .distance_to_next_m
                    .map(|distance| format!("{distance:.1}"))
                    .unwrap_or_default(),
                format!(
                    "{:.6},{:.6}",
                    maneuver.location.lat_deg, maneuver.location.lon_deg
                ),
                maneuver.instruction_text.clone().unwrap_or_default(),
            ]
            .join("|")
        })
        .collect::<Vec<_>>()
        .join(";");

    [
        format!("kind={kind}"),
        format!("route_id={}", route.route_id),
        format!("revision={}", route.revision),
        format!("version={}.{}", route.version.major, route.version.minor),
        format!(
            "summary={:.1}|{}|{}|{}",
            route.summary.total_distance_m,
            route.summary.estimated_duration_s,
            route.summary.start_label.clone().unwrap_or_default(),
            route.summary.destination_label.clone().unwrap_or_default()
        ),
        format!("geometry={geometry}"),
        format!("maneuvers={maneuvers}"),
        format!(
            "provenance={}|{}|{}",
            encode_provider(&route.provenance.provider),
            route.provenance.source_ref.clone().unwrap_or_default(),
            route.provenance.generated_at_unix_ms
        ),
    ]
    .join("\n")
}

fn parse_sync_message(payload: &str) -> Result<RouteSyncMessage, RouteSyncTransportError> {
    let fields = parse_fields(payload);
    let kind = required_field(&fields, "kind")?;
    match kind {
        "set" => Ok(RouteSyncMessage::Set(RouteSetMessage {
            route: parse_route_package(&fields)?,
        })),
        "update" => {
            let route = parse_route_package(&fields)?;
            Ok(RouteSyncMessage::Update(RouteUpdateMessage {
                route_id: route.route_id.clone(),
                revision: route.revision,
                route,
            }))
        }
        "clear" => Ok(RouteSyncMessage::Clear(RouteClearMessage {
            route_id: optional_string_field(&fields, "route_id"),
        })),
        "status" => Ok(RouteSyncMessage::Status(RouteStatusMessage {
            route_id: optional_string_field(&fields, "route_id"),
            revision: optional_u64_field(&fields, "revision")?,
            status: parse_status(required_field(&fields, "status")?)?,
            detail: optional_string_field(&fields, "detail"),
        })),
        "reroute_request" => {
            let rider_position = parse_geo_point(required_field(&fields, "rider")?)?;
            Ok(RouteSyncMessage::RerouteRequest(
                RouteRerouteRequestMessage {
                    route_id: optional_string_field(&fields, "route_id"),
                    rider_position,
                    reason: required_field(&fields, "reason")?.to_owned(),
                },
            ))
        }
        other => Err(RouteSyncTransportError::InvalidMessageKind(
            other.to_owned(),
        )),
    }
}

fn parse_route_package(
    fields: &BTreeMap<String, String>,
) -> Result<RoutePackage, RouteSyncTransportError> {
    let version = required_field(fields, "version")?;
    let (major, minor) =
        version
            .split_once('.')
            .ok_or_else(|| RouteSyncTransportError::InvalidField {
                field: "version",
                value: version.to_owned(),
            })?;
    let route_id = required_field(fields, "route_id")?.to_owned();
    let revision = parse_u64(required_field(fields, "revision")?, "revision")?;
    let (distance_m, duration_s, start_label, destination_label) =
        parse_summary(required_field(fields, "summary")?)?;
    let geometry = parse_geometry(required_field(fields, "geometry")?)?;
    let maneuvers = parse_maneuvers(required_field(fields, "maneuvers")?)?;
    let provenance = parse_provenance(required_field(fields, "provenance")?)?;

    Ok(RoutePackage {
        version: RoutePackageVersion::new(
            parse_u16(major, "version.major")?,
            parse_u16(minor, "version.minor")?,
        ),
        route_id,
        revision,
        geometry,
        maneuvers,
        summary: RouteSummary {
            total_distance_m: distance_m,
            estimated_duration_s: duration_s,
            start_label,
            destination_label,
        },
        provenance,
    })
}

fn parse_fields(payload: &str) -> BTreeMap<String, String> {
    payload
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.to_owned()))
        .collect()
}

fn required_field<'a>(
    fields: &'a BTreeMap<String, String>,
    key: &'static str,
) -> Result<&'a str, RouteSyncTransportError> {
    fields
        .get(key)
        .map(|value| value.as_str())
        .ok_or(RouteSyncTransportError::MissingField(key))
}

fn optional_string_field(fields: &BTreeMap<String, String>, key: &'static str) -> Option<String> {
    fields.get(key).and_then(|value| match value.as_str() {
        "" | "none" | "current" => None,
        other => Some(other.to_owned()),
    })
}

fn optional_u64_field(
    fields: &BTreeMap<String, String>,
    key: &'static str,
) -> Result<Option<u64>, RouteSyncTransportError> {
    match fields.get(key).map(String::as_str) {
        None | Some("") | Some("none") => Ok(None),
        Some(value) => Ok(Some(parse_u64(value, key)?)),
    }
}

fn parse_summary(
    value: &str,
) -> Result<(f32, u32, Option<String>, Option<String>), RouteSyncTransportError> {
    let mut parts = value.splitn(4, '|');
    let total_distance_m = parse_f32(parts.next().unwrap_or_default(), "summary.total_distance_m")?;
    let estimated_duration_s = parse_u32(
        parts.next().unwrap_or_default(),
        "summary.estimated_duration_s",
    )?;
    let start_label = non_empty(parts.next().unwrap_or_default());
    let destination_label = non_empty(parts.next().unwrap_or_default());
    Ok((
        total_distance_m,
        estimated_duration_s,
        start_label,
        destination_label,
    ))
}

fn parse_geometry(value: &str) -> Result<Vec<GeoPoint>, RouteSyncTransportError> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(';').map(parse_geo_point).collect()
}

fn parse_maneuvers(value: &str) -> Result<Vec<RouteManeuver>, RouteSyncTransportError> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(';')
        .map(|entry| {
            let parts = entry.splitn(6, '|').collect::<Vec<_>>();
            let id = parts
                .first()
                .ok_or(RouteSyncTransportError::MissingField("maneuver.id"))?
                .to_string();
            let maneuver_type = parse_maneuver_type(
                parts
                    .get(1)
                    .copied()
                    .ok_or(RouteSyncTransportError::MissingField("maneuver.type"))?,
            )?;
            let distance_from_start_m = parse_f32(
                parts
                    .get(2)
                    .copied()
                    .ok_or(RouteSyncTransportError::MissingField(
                        "maneuver.distance_from_start_m",
                    ))?,
                "maneuver.distance_from_start_m",
            )?;
            let (distance_to_next_m, location_value, instruction_value) = match parts.as_slice() {
                [_, _, _, location, instruction] => (None, *location, *instruction),
                [_, _, _, distance_to_next, location, instruction] => (
                    non_empty(distance_to_next)
                        .map(|value| parse_f32(value.as_str(), "maneuver.distance_to_next_m"))
                        .transpose()?,
                    *location,
                    *instruction,
                ),
                _ => return Err(RouteSyncTransportError::MissingField("maneuver.location")),
            };
            let location = parse_geo_point(location_value)?;
            let instruction_text = non_empty(instruction_value);
            Ok(RouteManeuver {
                id,
                maneuver_type,
                location,
                distance_from_start_m,
                distance_to_next_m,
                instruction_text,
            })
        })
        .collect()
}

fn parse_provenance(value: &str) -> Result<RouteProvenance, RouteSyncTransportError> {
    let mut parts = value.splitn(3, '|');
    let provider = parse_provider(parts.next().unwrap_or_default());
    let source_ref = non_empty(parts.next().unwrap_or_default());
    let generated_at_unix_ms = parse_u64(
        parts.next().unwrap_or_default(),
        "provenance.generated_at_unix_ms",
    )?;
    Ok(RouteProvenance {
        provider,
        source_ref,
        generated_at_unix_ms,
    })
}

fn parse_geo_point(value: &str) -> Result<GeoPoint, RouteSyncTransportError> {
    let (lat, lon) =
        value
            .split_once(',')
            .ok_or_else(|| RouteSyncTransportError::InvalidField {
                field: "geo_point",
                value: value.to_owned(),
            })?;
    Ok(GeoPoint::new(
        parse_f64(lat, "geo_point.lat")?,
        parse_f64(lon, "geo_point.lon")?,
    ))
}

fn parse_provider(value: &str) -> RouteProvider {
    match value.trim() {
        "hsl" | "hsl_digitransit" => RouteProvider::HslDigitransit,
        "google_ingest" | "googleIngest" => RouteProvider::GoogleIngest,
        "osm" => RouteProvider::Osm,
        "gpx" | "gpx_import" | "gpxImport" => RouteProvider::Gpx,
        "fit" | "fit_import" | "fitImport" => RouteProvider::Fit,
        "tcx" | "tcx_import" | "tcxImport" => RouteProvider::Tcx,
        "garmin_api" | "garminApi" => RouteProvider::GarminApi,
        "garmin_file" | "garminFile" => RouteProvider::GarminFile,
        other => RouteProvider::Unknown(other.to_owned()),
    }
}

fn encode_provider(provider: &RouteProvider) -> &'static str {
    match provider {
        RouteProvider::HslDigitransit => "hsl",
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

fn parse_maneuver_type(value: &str) -> Result<RouteManeuverType, RouteSyncTransportError> {
    match value.trim() {
        "depart" => Ok(RouteManeuverType::Depart),
        "straight" => Ok(RouteManeuverType::Straight),
        "slightLeft" | "slight_left" => Ok(RouteManeuverType::SlightLeft),
        "left" => Ok(RouteManeuverType::Left),
        "sharpLeft" | "sharp_left" => Ok(RouteManeuverType::SharpLeft),
        "slightRight" | "slight_right" => Ok(RouteManeuverType::SlightRight),
        "right" => Ok(RouteManeuverType::Right),
        "sharpRight" | "sharp_right" => Ok(RouteManeuverType::SharpRight),
        "uturn" | "u_turn" => Ok(RouteManeuverType::Uturn),
        "roundabout" => Ok(RouteManeuverType::Roundabout),
        "merge" => Ok(RouteManeuverType::Merge),
        "ramp" => Ok(RouteManeuverType::Ramp),
        "arrive" => Ok(RouteManeuverType::Arrive),
        other => Err(RouteSyncTransportError::InvalidField {
            field: "maneuver.type",
            value: other.to_owned(),
        }),
    }
}

fn encode_maneuver_type(maneuver_type: RouteManeuverType) -> &'static str {
    match maneuver_type {
        RouteManeuverType::Depart => "depart",
        RouteManeuverType::Straight => "straight",
        RouteManeuverType::SlightLeft => "slightLeft",
        RouteManeuverType::Left => "left",
        RouteManeuverType::SharpLeft => "sharpLeft",
        RouteManeuverType::SlightRight => "slightRight",
        RouteManeuverType::Right => "right",
        RouteManeuverType::SharpRight => "sharpRight",
        RouteManeuverType::Uturn => "uturn",
        RouteManeuverType::Roundabout => "roundabout",
        RouteManeuverType::Merge => "merge",
        RouteManeuverType::Ramp => "ramp",
        RouteManeuverType::Arrive => "arrive",
    }
}

fn parse_status(value: &str) -> Result<RouteSyncStatusCode, RouteSyncTransportError> {
    match value.trim() {
        "accepted" => Ok(RouteSyncStatusCode::Accepted),
        "applying" => Ok(RouteSyncStatusCode::Applying),
        "active" => Ok(RouteSyncStatusCode::Active),
        "cleared" => Ok(RouteSyncStatusCode::Cleared),
        "rejected" => Ok(RouteSyncStatusCode::Rejected),
        "retryablefailure" | "retryable_failure" => Ok(RouteSyncStatusCode::RetryableFailure),
        "fatalfailure" | "fatal_failure" => Ok(RouteSyncStatusCode::FatalFailure),
        other => Err(RouteSyncTransportError::InvalidField {
            field: "status",
            value: other.to_owned(),
        }),
    }
}

fn encode_status(status: RouteSyncStatusCode) -> &'static str {
    match status {
        RouteSyncStatusCode::Accepted => "accepted",
        RouteSyncStatusCode::Applying => "applying",
        RouteSyncStatusCode::Active => "active",
        RouteSyncStatusCode::Cleared => "cleared",
        RouteSyncStatusCode::Rejected => "rejected",
        RouteSyncStatusCode::RetryableFailure => "retryable_failure",
        RouteSyncStatusCode::FatalFailure => "fatal_failure",
    }
}

fn parse_f32(value: &str, field: &'static str) -> Result<f32, RouteSyncTransportError> {
    value
        .parse::<f32>()
        .map_err(|_| RouteSyncTransportError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

fn parse_f64(value: &str, field: &'static str) -> Result<f64, RouteSyncTransportError> {
    value
        .parse::<f64>()
        .map_err(|_| RouteSyncTransportError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

fn parse_u16(value: &str, field: &'static str) -> Result<u16, RouteSyncTransportError> {
    value
        .parse::<u16>()
        .map_err(|_| RouteSyncTransportError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

fn parse_u32(value: &str, field: &'static str) -> Result<u32, RouteSyncTransportError> {
    value
        .parse::<u32>()
        .map_err(|_| RouteSyncTransportError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

fn parse_u64(value: &str, field: &'static str) -> Result<u64, RouteSyncTransportError> {
    value
        .parse::<u64>()
        .map_err(|_| RouteSyncTransportError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn route_id_of_message(message: &RouteSyncMessage) -> Option<String> {
    match message {
        RouteSyncMessage::Set(set) => Some(set.route.route_id.clone()),
        RouteSyncMessage::Update(update) => Some(update.route.route_id.clone()),
        RouteSyncMessage::Clear(clear) => clear.route_id.clone(),
        RouteSyncMessage::Status(status) => status.route_id.clone(),
        RouteSyncMessage::RerouteRequest(request) => request.route_id.clone(),
    }
}

fn route_revision_of_message(message: &RouteSyncMessage) -> Option<u64> {
    match message {
        RouteSyncMessage::Set(set) => Some(set.route.revision),
        RouteSyncMessage::Update(update) => Some(update.route.revision),
        RouteSyncMessage::Clear(_) => None,
        RouteSyncMessage::Status(status) => status.revision,
        RouteSyncMessage::RerouteRequest(_) => None,
    }
}

pub(crate) fn checksum_hex(payload: &[u8]) -> String {
    let mut hash: u32 = 2_166_136_261;
    for byte in payload {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:08x}")
}

#[derive(Debug)]
struct PendingTransfer {
    transfer_id: String,
    total_chunks: u32,
    checksum_hex: String,
    chunks: BTreeMap<u32, Vec<u8>>,
}

impl PendingTransfer {
    fn new(transfer_id: String, total_chunks: u32, checksum_hex: String) -> Self {
        Self {
            transfer_id,
            total_chunks,
            checksum_hex,
            chunks: BTreeMap::new(),
        }
    }

    fn insert_chunk(&mut self, chunk: RouteTransferChunk) -> Result<(), RouteSyncTransportError> {
        match self.chunks.get(&chunk.chunk_index) {
            Some(existing) if existing != &chunk.payload_fragment => {
                Err(RouteSyncTransportError::ConflictingChunkData {
                    transfer_id: chunk.transfer_id,
                    chunk_index: chunk.chunk_index,
                })
            }
            Some(_) => Ok(()),
            None => {
                self.chunks
                    .insert(chunk.chunk_index, chunk.payload_fragment);
                Ok(())
            }
        }
    }

    fn is_complete(&self) -> bool {
        self.chunks.len() == self.total_chunks as usize
    }

    fn assembled_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        for index in 0..self.total_chunks {
            if let Some(chunk) = self.chunks.get(&index) {
                payload.extend_from_slice(chunk);
            }
        }
        payload
    }
}

#[derive(Debug)]
struct PendingApply {
    message: RouteSyncMessage,
    checksum_hex: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_route(revision: u64) -> RoutePackage {
        RoutePackage {
            version: RoutePackageVersion::new(1, 0),
            route_id: "hsl:kamppi->kallio:alt-0".to_owned(),
            revision,
            geometry: vec![
                GeoPoint::new(60.1699, 24.9384),
                GeoPoint::new(60.1712, 24.9443),
            ],
            maneuvers: vec![
                RouteManeuver {
                    id: "depart".to_owned(),
                    maneuver_type: RouteManeuverType::Depart,
                    location: GeoPoint::new(60.1699, 24.9384),
                    distance_from_start_m: 0.0,
                    distance_to_next_m: Some(120.0),
                    instruction_text: Some("Start riding".to_owned()),
                },
                RouteManeuver {
                    id: "arrive".to_owned(),
                    maneuver_type: RouteManeuverType::Arrive,
                    location: GeoPoint::new(60.1712, 24.9443),
                    distance_from_start_m: 120.0,
                    distance_to_next_m: None,
                    instruction_text: Some("Arrive".to_owned()),
                },
            ],
            summary: RouteSummary {
                total_distance_m: 120.0,
                estimated_duration_s: 45,
                start_label: Some("Kamppi".to_owned()),
                destination_label: Some("Kallio".to_owned()),
            },
            provenance: RouteProvenance {
                provider: RouteProvider::HslDigitransit,
                source_ref: Some("digitransit:test".to_owned()),
                generated_at_unix_ms: 1,
            },
        }
    }

    fn chunk_message(message: RouteSyncMessage, chunk_size: usize) -> Vec<RouteTransferChunk> {
        chunk_sync_message(&message, "transfer-1", chunk_size)
    }

    #[test]
    fn transport_reassembles_set_message_and_marks_active() {
        let mut transport = RouteSyncTransport::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });

        let mut statuses = Vec::new();
        for chunk in chunk_message(message, 40) {
            statuses.extend(transport.ingest_chunk(chunk).expect("chunk accepted"));
        }

        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].status, RouteSyncStatusCode::Accepted);
        assert_eq!(statuses[1].status, RouteSyncStatusCode::Applying);
        assert!(transport.take_pending_runtime_message().is_some());

        let final_status = transport.complete_applied_message().expect("active status");
        assert_eq!(final_status.status, RouteSyncStatusCode::Active);
        assert_eq!(transport.active_route_revision(), Some(1));
    }

    #[test]
    fn duplicate_replay_is_deduped() {
        let mut transport = RouteSyncTransport::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        for chunk in chunk_message(message.clone(), 64) {
            let _ = transport.ingest_chunk(chunk).expect("chunk accepted");
        }
        let _ = transport.complete_applied_message().expect("active status");

        let mut statuses = Vec::new();
        for chunk in chunk_message(message, 64) {
            statuses.extend(transport.ingest_chunk(chunk).expect("deduped chunk"));
        }

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, RouteSyncStatusCode::Active);
        assert!(transport.take_pending_runtime_message().is_none());
    }

    #[test]
    fn stale_revision_is_rejected() {
        let mut transport = RouteSyncTransport::default();
        let first = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(2),
        });
        for chunk in chunk_message(first, 64) {
            let _ = transport.ingest_chunk(chunk).expect("chunk accepted");
        }
        let _ = transport.complete_applied_message().expect("active status");

        let stale = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let mut statuses = Vec::new();
        for chunk in chunk_message(stale, 64) {
            statuses.extend(transport.ingest_chunk(chunk).expect("chunk processed"));
        }

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, RouteSyncStatusCode::Rejected);
    }

    #[test]
    fn clear_message_clears_active_route() {
        let mut transport = RouteSyncTransport::default();
        let set = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        for chunk in chunk_message(set, 64) {
            let _ = transport.ingest_chunk(chunk).expect("chunk accepted");
        }
        let _ = transport.complete_applied_message().expect("active status");

        let clear = RouteSyncMessage::Clear(RouteClearMessage {
            route_id: Some("hsl:kamppi->kallio:alt-0".to_owned()),
        });
        let mut statuses = Vec::new();
        for chunk in chunk_message(clear, 64) {
            statuses.extend(transport.ingest_chunk(chunk).expect("clear accepted"));
        }
        let final_status = transport.complete_applied_message().expect("cleared");

        assert_eq!(statuses.len(), 2);
        assert_eq!(final_status.status, RouteSyncStatusCode::Cleared);
        assert_eq!(transport.active_route_id(), None);
    }

    #[test]
    fn interrupted_transfer_resumes_when_remaining_chunks_arrive_later() {
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let chunks = chunk_sync_message(&message, "transfer-1", 20);
        let mut transport = RouteSyncTransport::default();

        for chunk in chunks.iter().take(2).cloned() {
            let statuses = transport
                .ingest_chunk(chunk)
                .expect("partial chunk ingestion");
            assert!(statuses.is_empty());
        }
        assert!(transport.take_pending_runtime_message().is_none());

        for chunk in chunks.iter().skip(2).cloned() {
            let is_final_chunk = chunk.chunk_index + 1 == chunk.total_chunks;
            let statuses = transport
                .ingest_chunk(chunk)
                .expect("resumed chunk ingestion");
            if is_final_chunk {
                assert_eq!(statuses.len(), 2);
                assert_eq!(statuses[0].status, RouteSyncStatusCode::Accepted);
                assert_eq!(statuses[1].status, RouteSyncStatusCode::Applying);
            } else {
                assert!(statuses.is_empty());
            }
        }

        let applied = transport
            .complete_applied_message()
            .expect("active status after resumed transfer");
        assert_eq!(applied.status, RouteSyncStatusCode::Active);
        assert_eq!(transport.active_route_revision(), Some(1));
    }

    #[test]
    fn out_of_order_chunks_are_reassembled_successfully() {
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let mut chunks = chunk_sync_message(&message, "transfer-1", 18);
        chunks.reverse();
        let mut transport = RouteSyncTransport::default();
        let mut final_statuses = Vec::new();

        for chunk in chunks {
            let statuses = transport
                .ingest_chunk(chunk)
                .expect("out of order chunk ingestion");
            if !statuses.is_empty() {
                final_statuses = statuses;
            }
        }

        assert_eq!(final_statuses.len(), 2);
        assert_eq!(final_statuses[0].status, RouteSyncStatusCode::Accepted);
        assert_eq!(final_statuses[1].status, RouteSyncStatusCode::Applying);
        assert_eq!(
            transport
                .complete_applied_message()
                .expect("active status")
                .status,
            RouteSyncStatusCode::Active
        );
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let mut transport = RouteSyncTransport::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let mut chunks = chunk_message(message, 64);
        chunks
            .iter_mut()
            .for_each(|chunk| chunk.checksum_hex = "deadbeef".to_owned());

        let mut result = Ok(Vec::new());
        for chunk in chunks {
            result = transport.ingest_chunk(chunk);
        }
        assert!(matches!(
            result,
            Err(RouteSyncTransportError::ChecksumMismatch { .. })
        ));
    }
}
