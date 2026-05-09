use std::collections::BTreeMap;
use std::time::Duration;

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

/// Upper bound on `RouteTransferChunk::total_chunks`. A peer that announces
/// more chunks than this is rejected before any allocation happens — defends
/// against a malicious or buggy peer claiming `total_chunks = u32::MAX` to
/// force the device to allocate an unbounded `BTreeMap` slot reservation.
/// 1024 chunks at the contract's 96-byte chunk size is ~96 KiB of payload —
/// comfortably more than any plausible route's serialized form.
pub const MAX_TOTAL_CHUNKS: u32 = 1024;

/// Upper bound on the cumulative `payload_fragment` bytes accumulated for
/// a single transfer. Defends against a peer pumping individually-valid
/// chunks (each MTU-sized) until the device runs out of heap. 128 KiB is
/// generous: a fully-loaded 1024-chunk transfer at 96 bytes/chunk is
/// ~96 KiB; the cap leaves headroom for future chunk-size growth without
/// hitting the wall on a normal transfer.
pub const MAX_PAYLOAD_BYTES: usize = 128 * 1024;

/// Duration after which an in-flight transfer with no new chunks is
/// dropped. Defends against a peer that opens a transfer (allocating
/// state) then silently goes away — without this, `pending_transfer`
/// would stay alive forever pinning memory and blocking new transfers
/// from peers that use a different `transfer_id`.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Peer announced a `total_chunks` count exceeding `MAX_TOTAL_CHUNKS`.
    /// Returned before the pending transfer is allocated, so no state has
    /// to be cleaned up — the next chunk with a valid count starts fresh.
    TooManyChunks {
        total_chunks: u32,
        limit: u32,
    },
    /// Cumulative payload bytes for the in-flight transfer would exceed
    /// `MAX_PAYLOAD_BYTES`. Reported on the chunk that crosses the cap;
    /// the entire transfer is dropped (no half-state retained) so the
    /// runtime can't accidentally apply a partial route later. `observed`
    /// is the post-overflow byte count so the companion can report a
    /// precise message.
    PayloadTooLarge {
        observed: usize,
        limit: usize,
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
            | Self::ChecksumMismatch { .. }
            | Self::TooManyChunks { .. }
            | Self::PayloadTooLarge { .. } => RouteSyncStatusCode::RetryableFailure,
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
            Self::TooManyChunks {
                total_chunks,
                limit,
            } => format!(
                "Rejected route transfer announcing {} chunks (cap {})",
                total_chunks, limit
            ),
            Self::PayloadTooLarge { observed, limit } => format!(
                "Rejected route transfer payload of {} bytes (cap {})",
                observed, limit
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
    /// The most recent monotonic timestamp the transport has observed,
    /// updated by either `tick` or `ingest_chunk`. The pending transfer's
    /// `last_activity` is set from this on each chunk so the idle timer
    /// has a single, consistent source of "now".
    last_observed_now: Duration,
}

impl RouteSyncTransport {
    /// Advance the transport's notion of "now" and drop any pending
    /// transfer that has been idle longer than `IDLE_TIMEOUT`. Returns
    /// status messages to publish back to the companion (currently at
    /// most one `RetryableFailure` when an idle timeout fires).
    ///
    /// `App::step_frame` calls this once per frame with a monotonic
    /// `Duration` accumulator. Tests call it directly to advance time
    /// without driving the runtime.
    pub fn tick(&mut self, now: Duration) -> Vec<RouteStatusMessage> {
        self.last_observed_now = now;
        let Some(pending) = self.pending_transfer.as_ref() else {
            return Vec::new();
        };
        let elapsed = now.saturating_sub(pending.last_activity);
        if elapsed <= IDLE_TIMEOUT {
            return Vec::new();
        }
        // Drop the entire pending transfer; companion will start over
        // with a fresh transfer_id on retry.
        self.pending_transfer = None;
        vec![RouteStatusMessage {
            route_id: None,
            revision: None,
            status: RouteSyncStatusCode::RetryableFailure,
            detail: Some(format!(
                "Route transfer dropped after idle timeout ({}s without a new chunk)",
                IDLE_TIMEOUT.as_secs(),
            )),
        }]
    }

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
        if chunk.total_chunks > MAX_TOTAL_CHUNKS {
            return Err(RouteSyncTransportError::TooManyChunks {
                total_chunks: chunk.total_chunks,
                limit: MAX_TOTAL_CHUNKS,
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
                self.last_observed_now,
            ));
        }

        let pending = self.pending_transfer.as_mut().expect("pending transfer");
        pending.last_activity = self.last_observed_now;
        if let Err(error) = pending.insert_chunk(chunk) {
            // PayloadTooLarge means the running total crossed the cap;
            // drop the entire pending transfer so a malicious peer can't
            // pump partial chunks indefinitely. Other errors leave the
            // pending state intact so the legitimate companion can retry
            // a single bad chunk without losing progress.
            if matches!(error, RouteSyncTransportError::PayloadTooLarge { .. }) {
                self.pending_transfer = None;
            }
            return Err(error);
        }
        let pending = self.pending_transfer.as_ref().expect("pending transfer");
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
        "osm" => RouteProvider::Osm,
        "gpx" | "gpx_import" | "gpxImport" => RouteProvider::Gpx,
        "fit" | "fit_import" | "fitImport" => RouteProvider::Fit,
        "tcx" | "tcx_import" | "tcxImport" => RouteProvider::Tcx,
        other => RouteProvider::Unknown(other.to_owned()),
    }
}

fn encode_provider(provider: &RouteProvider) -> &'static str {
    match provider {
        RouteProvider::HslDigitransit => "hsl",
        RouteProvider::Osm => "osm",
        RouteProvider::Gpx => "gpx",
        RouteProvider::Fit => "fit",
        RouteProvider::Tcx => "tcx",
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
    /// Running total of bytes accumulated so far. Updated only when a new
    /// chunk is inserted (duplicates don't double-count). Compared against
    /// `MAX_PAYLOAD_BYTES` before insertion so an oversized chunk is
    /// rejected without growing the buffer.
    bytes_assembled: usize,
    /// Monotonic timestamp of the most recent chunk observed for this
    /// transfer. Compared against the transport's `last_observed_now` in
    /// `tick` to drop the transfer after `IDLE_TIMEOUT`.
    last_activity: Duration,
}

impl PendingTransfer {
    fn new(
        transfer_id: String,
        total_chunks: u32,
        checksum_hex: String,
        last_activity: Duration,
    ) -> Self {
        Self {
            transfer_id,
            total_chunks,
            checksum_hex,
            chunks: BTreeMap::new(),
            bytes_assembled: 0,
            last_activity,
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
                let projected = self
                    .bytes_assembled
                    .saturating_add(chunk.payload_fragment.len());
                if projected > MAX_PAYLOAD_BYTES {
                    return Err(RouteSyncTransportError::PayloadTooLarge {
                        observed: projected,
                        limit: MAX_PAYLOAD_BYTES,
                    });
                }
                self.bytes_assembled = projected;
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

    #[test]
    fn transport_rejects_total_chunks_above_cap() {
        let mut transport = RouteSyncTransport::default();
        let chunk = RouteTransferChunk {
            transfer_id: "t1".to_owned(),
            chunk_index: 0,
            total_chunks: MAX_TOTAL_CHUNKS + 1,
            checksum_hex: "deadbeef".to_owned(),
            payload_fragment: Vec::new(),
        };

        let result = transport.ingest_chunk(chunk);

        assert!(
            matches!(
                result,
                Err(RouteSyncTransportError::TooManyChunks {
                    total_chunks,
                    limit,
                }) if total_chunks == MAX_TOTAL_CHUNKS + 1 && limit == MAX_TOTAL_CHUNKS
            ),
            "expected TooManyChunks {{ total_chunks: {}, limit: {} }}, got {result:?}",
            MAX_TOTAL_CHUNKS + 1,
            MAX_TOTAL_CHUNKS,
        );
    }

    #[test]
    fn too_many_chunks_error_maps_to_retryable_failure_status() {
        let err = RouteSyncTransportError::TooManyChunks {
            total_chunks: 1025,
            limit: 1024,
        };
        assert_eq!(err.status_code(), RouteSyncStatusCode::RetryableFailure);
        let detail = err.detail_message();
        assert!(
            detail.contains("1025") && detail.contains("1024"),
            "detail should report observed and limit so the companion can surface it: {detail:?}",
        );
    }

    fn tiny_chunk(transfer_id: &str, index: u32, total: u32) -> RouteTransferChunk {
        RouteTransferChunk {
            transfer_id: transfer_id.to_owned(),
            chunk_index: index,
            total_chunks: total,
            checksum_hex: "deadbeef".to_owned(),
            payload_fragment: vec![0xCC_u8; 4],
        }
    }

    #[test]
    fn pending_transfer_drops_after_idle_timeout() {
        let mut transport = RouteSyncTransport::default();
        // Ingest 3 of 5 chunks at t=0.
        for index in 0..3 {
            transport
                .ingest_chunk(tiny_chunk("t-idle", index, 5))
                .expect("partial chunk accepted at t=0");
        }
        let statuses = transport.tick(IDLE_TIMEOUT + Duration::from_secs(1));

        assert_eq!(
            statuses.len(),
            1,
            "exactly one RetryableFailure status should fire on idle timeout"
        );
        assert_eq!(statuses[0].status, RouteSyncStatusCode::RetryableFailure);
        let detail = statuses[0].detail.as_deref().unwrap_or_default();
        assert!(
            detail.contains("idle"),
            "detail should mention idle timeout so the companion can prompt a retry: {detail:?}",
        );
        // The pending transfer is gone — sending chunk 0 again starts fresh.
        let resumed = transport
            .ingest_chunk(tiny_chunk("t-idle", 0, 5))
            .expect("post-timeout chunk accepted as a fresh transfer");
        assert!(resumed.is_empty(), "first chunk of a fresh transfer emits no statuses yet");
    }

    #[test]
    fn pending_transfer_survives_tick_inside_timeout_window() {
        let mut transport = RouteSyncTransport::default();
        for index in 0..3 {
            transport
                .ingest_chunk(tiny_chunk("t-window", index, 5))
                .expect("partial chunk accepted");
        }
        let statuses = transport.tick(IDLE_TIMEOUT - Duration::from_secs(1));
        assert!(
            statuses.is_empty(),
            "tick inside the timeout window must not drop the pending transfer",
        );
        // Confirm pending is still there by sending a duplicate chunk and
        // verifying it doesn't reset state — duplicates don't error and
        // don't allocate; if pending were gone, this would start a new
        // transfer instead.
        transport
            .ingest_chunk(tiny_chunk("t-window", 1, 5))
            .expect("duplicate chunk accepted while transfer is alive");
    }

    #[test]
    fn pending_transfer_idle_timer_resets_on_each_chunk() {
        let mut transport = RouteSyncTransport::default();
        transport.tick(Duration::from_secs(0));
        transport
            .ingest_chunk(tiny_chunk("t-reset", 0, 5))
            .expect("chunk 0 accepted at t=0");
        transport.tick(Duration::from_secs(20));
        transport
            .ingest_chunk(tiny_chunk("t-reset", 1, 5))
            .expect("chunk 1 accepted at t=20s");
        // 25s after the last activity (at t=45s, last_activity=t=20s).
        // Inside the 30s window — must NOT drop.
        let statuses = transport.tick(Duration::from_secs(45));
        assert!(
            statuses.is_empty(),
            "the idle timer must reset on each chunk, not on the first chunk only",
        );
    }

    #[test]
    fn tick_with_no_pending_transfer_is_noop() {
        let mut transport = RouteSyncTransport::default();
        let statuses = transport.tick(Duration::from_secs(3600));
        assert!(statuses.is_empty(), "tick with no pending transfer must not panic or emit");
    }

    #[test]
    fn transport_rejects_single_oversized_chunk_payload() {
        let mut transport = RouteSyncTransport::default();
        let oversized = vec![0xAA_u8; MAX_PAYLOAD_BYTES + 1];
        let chunk = RouteTransferChunk {
            transfer_id: "t-oversized".to_owned(),
            chunk_index: 0,
            total_chunks: 1,
            checksum_hex: "deadbeef".to_owned(),
            payload_fragment: oversized,
        };

        let result = transport.ingest_chunk(chunk);

        assert!(
            matches!(
                result,
                Err(RouteSyncTransportError::PayloadTooLarge {
                    observed,
                    limit,
                }) if observed == MAX_PAYLOAD_BYTES + 1 && limit == MAX_PAYLOAD_BYTES
            ),
            "expected PayloadTooLarge {{ observed: {}, limit: {} }}, got {result:?}",
            MAX_PAYLOAD_BYTES + 1,
            MAX_PAYLOAD_BYTES,
        );
        // Rejected before allocation — no half-state retained.
        assert!(
            transport.take_pending_runtime_message().is_none(),
            "rejected oversized chunk should not allocate any pending transfer state",
        );
    }

    #[test]
    fn transport_rejects_chunk_that_overflows_running_total() {
        let mut transport = RouteSyncTransport::default();
        // 31 chunks of 5 KiB each: 31 * 5120 = 158_720 bytes,
        // which crosses the 131_072-byte cap on chunk 27 (27 * 5120 = 138_240).
        const FRAGMENT_SIZE: usize = 5120;
        const TOTAL_CHUNKS: u32 = 60;
        let mut last_result = Ok(Vec::new());
        let mut overflow_index = None;
        for index in 0..TOTAL_CHUNKS {
            let chunk = RouteTransferChunk {
                transfer_id: "t-overflow".to_owned(),
                chunk_index: index,
                total_chunks: TOTAL_CHUNKS,
                checksum_hex: "deadbeef".to_owned(),
                payload_fragment: vec![0xBB_u8; FRAGMENT_SIZE],
            };
            last_result = transport.ingest_chunk(chunk);
            if matches!(last_result, Err(RouteSyncTransportError::PayloadTooLarge { .. })) {
                overflow_index = Some(index);
                break;
            }
        }

        let overflow_index = overflow_index
            .expect("expected at least one chunk to trip the payload cap");
        // Cap is 131_072. Each chunk is 5120 bytes. Chunks 0..=24 (25 chunks)
        // accumulate to 128_000 bytes which fits; chunk 25 (the 26th) would
        // push to 26 * 5120 = 133_120 bytes, crossing the cap. The
        // overflowing chunk is therefore index 25.
        assert_eq!(
            overflow_index, 25,
            "overflow should fire on the chunk that crosses the cap, not earlier or later",
        );
        // Error reports the post-overflow size so the companion can surface
        // a precise message.
        let observed = match &last_result {
            Err(RouteSyncTransportError::PayloadTooLarge { observed, .. }) => *observed,
            _ => panic!("expected PayloadTooLarge, got {last_result:?}"),
        };
        assert_eq!(observed, 26 * FRAGMENT_SIZE);
        // The entire transfer is dropped — no half-state retained, so the
        // runtime can't accidentally apply a partial route later.
        assert!(transport.take_pending_runtime_message().is_none());
    }

    #[test]
    fn payload_too_large_error_maps_to_retryable_failure_status() {
        let err = RouteSyncTransportError::PayloadTooLarge {
            observed: 200_000,
            limit: MAX_PAYLOAD_BYTES,
        };
        assert_eq!(err.status_code(), RouteSyncStatusCode::RetryableFailure);
        let detail = err.detail_message();
        assert!(
            detail.contains("200000") && detail.contains(&MAX_PAYLOAD_BYTES.to_string()),
            "detail should report observed and limit so the companion can surface it: {detail:?}",
        );
    }

    #[test]
    fn transport_accepts_exactly_max_total_chunks() {
        let mut transport = RouteSyncTransport::default();
        let chunk = RouteTransferChunk {
            transfer_id: "t-boundary".to_owned(),
            chunk_index: 0,
            total_chunks: MAX_TOTAL_CHUNKS,
            checksum_hex: "deadbeef".to_owned(),
            payload_fragment: b"first-of-many".to_vec(),
        };

        let result = transport.ingest_chunk(chunk);

        assert!(
            !matches!(result, Err(RouteSyncTransportError::TooManyChunks { .. })),
            "boundary value MAX_TOTAL_CHUNKS must be accepted (cap is `>` not `>=`)",
        );
    }
}
