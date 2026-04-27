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
- packet encode/decode for `chunk` and `sync_message`
- route chunk reassembly and runtime ingress
- platform bridge for inbound chunk polling plus outbound `status` and `reroute_request` publication
- ESP-IDF BLE/GATT server adapter for the fixed route-sync service and characteristics on BLE-capable ESP-IDF targets (on-chip radio path via `esp-idf-svc::bt`)
- ESP32-P4 BLE/GATT server backed by the on-board ESP32-C6 radio over hosted SDIO (`firmware/components/hosted_ble`, exposed to Rust as `HostedBleRouteSyncIo`); the P4 device entrypoint advertises the route-sync service on boot and routes chunks straight into the same `RouteSyncTransport` the host tests cover
- deterministic firmware tests for packet round-trips, runtime activation, and reroute-request publication from the platform seam

Implemented in native companion apps:
- matching packet codec and GATT constants mirrored into both native companion apps
- CoreBluetooth central/client adapter on iOS for scan, connect, chunk writes, and notification handling
- Android BLE/GATT central adapter for scan, connect, chunk writes, notification handling, and runtime permission prompting

Remaining implementation work:
- live packet-loss / interruption fault-injection tests against the concrete adapters
- full field validation on real ESP32-P4 hardware paired with each mobile platform (initial bring-up confirmed on the Waveshare 3.4C with the iOS companion: scan via service UUID, connect, GATT discovery, notification subscription all round-trip end-to-end)

## Advertising layout

Legacy BLE advertising is capped at 31 bytes per packet, so the route-sync service splits its identifying data across two packets:

- **Main advertising packet** — flags (3 bytes) + the 128-bit service UUID (18 bytes). Nothing else: the device name, appearance, and connection-interval-range fields are deliberately omitted. With the 18-byte UUID + 3-byte flags + IDF's internal accounting overhead, even one extra optional field (the 6-byte Slave Connection Interval Range) was enough to push the packet over 31 bytes, at which point Bluedroid silently drops the trailing AD entries — typically the 128-bit UUID itself, which is the exact field iOS / Android filter on while scanning.
- **Scan response** — device name (`ESP32 Bike Minimap`, 20 bytes) + appearance (4 bytes, `0x0480` "generic cycling"). Returned only when the central does an active scan, so there's no cost to including it.

The iOS / Android companions filter scan results by the 128-bit service UUID. Make sure that field stays in the *main* packet across any future advertising changes; if you add another field there, recount the bytes.

## Interrupt watchdog

`CONFIG_ESP_INT_WDT_TIMEOUT_MS` is set to **1000 ms** (versus IDF's 300 ms default) on this build. Once the hosted-BLE stack is running, the v2.x host periodically retries the `Req_FeatureControl` RPC against the older v0.0.6 C6 slave; the synchronous wait holds the BTM task long enough that legacy-I2C touch polling stalls IRQ servicing past the 300 ms threshold and HP_WDT trips. 1 s is enough headroom to absorb the worst observed stalls without disabling the watchdog entirely; once the C6 slave is upgraded (or the failing RPC is stubbed at the host) we can tighten this back down.

## ESP32-C6 slave firmware

Espressif and Waveshare pre-flash the on-board C6 with the `esp_hosted` slave (typically `v0.0.6` on the 3.4C). That older firmware doesn't implement the `Req_FeatureControl` RPC the v2.x host stack issues, but it does auto-start BT on boot. We treat the controller-init/enable RPCs as advisory (matching Espressif's `host_bluedroid_host_only` reference), so BLE comes up against the pre-flashed slave anyway. You'll see this in the serial log:

```
W (...) transport: Version mismatch: Host [2.x.x] > Co-proc [0.0.0]
W (...) hosted_ble: esp_hosted_bt_controller_init returned ESP_FAIL — slave BT is expected to be self-starting; continuing with HCI bridge
I (...) hosted_ble: BLE host stack online via ESP32-C6 over hosted SDIO
I (...) firmware::hosted_ble: hosted-ble: route-sync GATT server online
```

That's the expected steady state — no slave reflash required. If the version mismatch ever turns into a real incompatibility (HCI-bridge symptoms, missed advertisements, broken connections), the upgrade path below builds and flashes the matching slave image:

1. Build the slave image (after at least one `cargo xtask bundle-device` run, which unpacks the `esp_hosted` managed component):
   ```
   cargo xtask build-c6-slave
   ```
   This produces `.xtask/c6-slave/c6-slave-merged.bin` — a single bootloader + partition-table + app image flashable at offset 0x0.
2. Flash to the C6 over its UART. The C6 is **not** reachable over the same USB-JTAG port that flashes the P4 — the Waveshare 3.4C exposes the C6's UART on a separate connector / pad set (see the kit's schematic). Once that port shows up on your host:
   ```
   espflash write-bin --chip esp32c6 --port <C6-PORT> 0x0 c6-slave-merged.bin
   ```
3. Power-cycle the board. The next P4 boot should print `H_API: Transport active`, `Identified slave [esp32c6]`, and `Host BT Support: Enabled` without the version-mismatch warning, and `hosted-ble: route-sync GATT server online` from our wrapper.
