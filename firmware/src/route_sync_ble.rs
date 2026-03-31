use std::collections::BTreeMap;

use crate::route_sync::{
    RouteSyncTransportError, RouteTransferChunk, decode_sync_message, encode_sync_message,
};
use runtime_core::api::RouteSyncMessage;

pub const ROUTE_SYNC_SERVICE_UUID: &str = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001";
pub const ROUTE_SYNC_CHUNK_WRITE_UUID: &str = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002";
pub const ROUTE_SYNC_EVENT_NOTIFY_UUID: &str = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003";
const BLE_PACKET_VERSION: &str = "1";

#[derive(Debug, Clone, PartialEq)]
pub enum BleRouteSyncPacket {
    Chunk(RouteTransferChunk),
    SyncMessage(RouteSyncMessage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BleRouteSyncPacketError {
    MissingHeaderSeparator,
    InvalidUtf8Header,
    MissingField(&'static str),
    InvalidField { field: &'static str, value: String },
    PayloadLengthMismatch { expected: usize, actual: usize },
    UnsupportedPacketType(String),
    UnsupportedVersion(String),
    SyncTransport(String),
}

impl From<RouteSyncTransportError> for BleRouteSyncPacketError {
    fn from(value: RouteSyncTransportError) -> Self {
        Self::SyncTransport(format!("{value:?}"))
    }
}

pub fn encode_ble_packet(packet: &BleRouteSyncPacket) -> Vec<u8> {
    match packet {
        BleRouteSyncPacket::Chunk(chunk) => encode_packet(
            &[
                ("v", BLE_PACKET_VERSION.to_owned()),
                ("type", "chunk".to_owned()),
                ("transfer_id", chunk.transfer_id.clone()),
                ("chunk_index", chunk.chunk_index.to_string()),
                ("total_chunks", chunk.total_chunks.to_string()),
                ("checksum", chunk.checksum_hex.clone()),
            ],
            &chunk.payload_fragment,
        ),
        BleRouteSyncPacket::SyncMessage(message) => {
            let payload = encode_sync_message(message).into_bytes();
            encode_packet(
                &[
                    ("v", BLE_PACKET_VERSION.to_owned()),
                    ("type", "sync_message".to_owned()),
                ],
                &payload,
            )
        }
    }
}

pub fn decode_ble_packet(payload: &[u8]) -> Result<BleRouteSyncPacket, BleRouteSyncPacketError> {
    let (headers, body) = split_header_and_body(payload)?;
    let version = required_field(&headers, "v")?;
    if version != BLE_PACKET_VERSION {
        return Err(BleRouteSyncPacketError::UnsupportedVersion(
            version.to_owned(),
        ));
    }

    match required_field(&headers, "type")? {
        "chunk" => Ok(BleRouteSyncPacket::Chunk(RouteTransferChunk {
            transfer_id: required_field(&headers, "transfer_id")?.to_owned(),
            chunk_index: parse_u32(required_field(&headers, "chunk_index")?, "chunk_index")?,
            total_chunks: parse_u32(required_field(&headers, "total_chunks")?, "total_chunks")?,
            checksum_hex: required_field(&headers, "checksum")?.to_owned(),
            payload_fragment: body,
        })),
        "sync_message" => Ok(BleRouteSyncPacket::SyncMessage(decode_sync_message(&body)?)),
        other => Err(BleRouteSyncPacketError::UnsupportedPacketType(
            other.to_owned(),
        )),
    }
}

fn encode_packet(headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let mut encoded = headers
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    encoded.extend_from_slice(format!("\npayload_length={}\n\n", body.len()).as_bytes());
    encoded.extend_from_slice(body);
    encoded
}

fn split_header_and_body(
    payload: &[u8],
) -> Result<(BTreeMap<String, String>, Vec<u8>), BleRouteSyncPacketError> {
    let separator = payload
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or(BleRouteSyncPacketError::MissingHeaderSeparator)?;
    let header_bytes = &payload[..separator];
    let body = payload[separator + 2..].to_vec();
    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|_| BleRouteSyncPacketError::InvalidUtf8Header)?;
    let headers = header_text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.to_owned()))
        .collect::<BTreeMap<_, _>>();
    let expected_length = parse_usize(
        required_field(&headers, "payload_length")?,
        "payload_length",
    )?;
    if expected_length != body.len() {
        return Err(BleRouteSyncPacketError::PayloadLengthMismatch {
            expected: expected_length,
            actual: body.len(),
        });
    }
    Ok((headers, body))
}

fn required_field<'a>(
    headers: &'a BTreeMap<String, String>,
    key: &'static str,
) -> Result<&'a str, BleRouteSyncPacketError> {
    headers
        .get(key)
        .map(|value| value.as_str())
        .ok_or(BleRouteSyncPacketError::MissingField(key))
}

fn parse_u32(value: &str, field: &'static str) -> Result<u32, BleRouteSyncPacketError> {
    value
        .parse::<u32>()
        .map_err(|_| BleRouteSyncPacketError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

fn parse_usize(value: &str, field: &'static str) -> Result<usize, BleRouteSyncPacketError> {
    value
        .parse::<usize>()
        .map_err(|_| BleRouteSyncPacketError::InvalidField {
            field,
            value: value.to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_core::api::{
        GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage, RoutePackageVersion,
        RouteProvenance, RouteProvider, RouteRerouteRequestMessage, RouteSetMessage, RouteSummary,
    };

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

    #[test]
    fn ble_chunk_packets_round_trip() {
        let packet = BleRouteSyncPacket::Chunk(RouteTransferChunk {
            transfer_id: "transfer-1".to_owned(),
            chunk_index: 2,
            total_chunks: 4,
            checksum_hex: "deadbeef".to_owned(),
            payload_fragment: b"hello-route".to_vec(),
        });

        let encoded = encode_ble_packet(&packet);
        let decoded = decode_ble_packet(&encoded).expect("decode ble chunk packet");

        assert_eq!(decoded, packet);
    }

    #[test]
    fn ble_sync_message_packets_round_trip() {
        let packet = BleRouteSyncPacket::SyncMessage(RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        }));

        let encoded = encode_ble_packet(&packet);
        let decoded = decode_ble_packet(&encoded).expect("decode ble sync message");

        assert_eq!(decoded, packet);
    }

    #[test]
    fn ble_sync_message_packets_support_reroute_requests() {
        let packet = BleRouteSyncPacket::SyncMessage(RouteSyncMessage::RerouteRequest(
            RouteRerouteRequestMessage {
                route_id: Some("hsl:kamppi->kallio:alt-0".to_owned()),
                rider_position: GeoPoint::new(60.17, 24.94),
                reason: "off_route".to_owned(),
            },
        ));

        let encoded = encode_ble_packet(&packet);
        let decoded = decode_ble_packet(&encoded).expect("decode reroute request packet");

        assert_eq!(decoded, packet);
    }
}
