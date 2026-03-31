# BLE Route Sync Contract

This document defines the first concrete BLE wire contract for companion-to-device route sync.

## Goals
- Keep route business logic out of platform BLE code.
- Reuse the existing canonical `RouteSyncMessage` and `RouteTransferChunk` contracts.
- Make the BLE layer responsible only for packet transport, not route parsing policy.
- Keep the wire format deterministic and debuggable during the first production BLE bring-up.

## GATT Layout
Service UUID:
- `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001`

Characteristics:
- Companion -> device route chunk write:
  - `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002`
  - expected properties: `write` or `writeWithoutResponse`
- Device -> companion route event notify:
  - `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003`
  - expected properties: `notify`

## Packet Types
BLE packets carry one of two payload shapes:
- `chunk`
  - one chunk of a chunked route transfer from companion to device
  - wraps `RouteTransferChunk`
- `sync_message`
  - one canonical `RouteSyncMessage`
  - used for device-to-companion `status` and `reroute_request` messages
  - may also be used in tooling or test harnesses for end-to-end packet validation

## Wire Format
Packet encoding is UTF-8 header lines followed by a raw payload body.

Header fields are `key=value` lines.
A blank line (`\n\n`) separates header and payload.
The raw payload body length must match `payload_length` exactly.

Common header fields:
- `v=1`
- `type=chunk` or `type=sync_message`
- `payload_length=<byte_count>`

### Chunk Packet
Required headers:
- `v=1`
- `type=chunk`
- `transfer_id=<string>`
- `chunk_index=<u32>`
- `total_chunks=<u32>`
- `checksum=<8-char hex>`
- `payload_length=<usize>`

Body:
- raw `payload_fragment` bytes from `RouteTransferChunk`

### Sync Message Packet
Required headers:
- `v=1`
- `type=sync_message`
- `payload_length=<usize>`

Body:
- raw UTF-8 bytes of canonical `RouteSyncMessage` payload encoding

## Canonical Message Payload
The canonical message payload remains the line-oriented route sync encoding already used by the device transport layer.

Important compatibility note:
- maneuver encoding now preserves both:
  - `distance_from_start_m`
  - `distance_to_next_m`
- decoders must remain backward-compatible with older 5-field maneuver payloads that omitted `distance_to_next_m`

## Ownership Rules
- Companion apps own:
  - message creation
  - chunking
  - write scheduling
  - notification subscription
  - retry/resume orchestration
- Firmware owns:
  - chunk reassembly
  - checksum verification
  - stale/conflicting revision rejection
  - route activation / clear application through shared runtime
- BLE adapters must not:
  - parse provider-native route formats
  - implement route-follow logic
  - invent alternate packet encodings outside this contract

## Current Implementation Status
Implemented in firmware:
- packet encode/decode for `chunk` and `sync_message` in firmware
- route chunk reassembly and runtime ingress in firmware
- platform bridge for inbound chunk polling and outbound status publication in firmware
- deterministic firmware tests for packet round-trips and runtime activation
- matching packet codec and GATT constants mirrored into both native companion apps

Remaining implementation work:
- CoreBluetooth adapter using this GATT contract
- Android BLE adapter using this GATT contract
- ESP-IDF BLE/GATT adapter using this GATT contract
- packet-loss / interruption fault-injection tests against live adapters
