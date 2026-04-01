# Routing Program Technical Plan

Spec reference: [`project-spec.md`](./project-spec.md)
Runtime architecture reference: [`runtime-ecs-architecture.md`](./runtime-ecs-architecture.md)
Execution guide reference: [`framework-execution-guide.md`](./framework-execution-guide.md)
Route contract reference: [`route-package-contract.md`](./route-package-contract.md)
BLE sync contract reference: [`ble-route-sync-contract.md`](./ble-route-sync-contract.md)

## Architecture Vision
Phone-orchestrated routing with ESP-optimized route following.

The product is built around a map-first riding UX:
- route highlight is the primary guidance surface
- off-route and major-turn alerts are secondary, configurable overlays
- the system can extend alert verbosity later without changing core contracts

Performance principle:
- heavy planning and rerouting stay off-device on companion app infrastructure
- deterministic low-latency route-follow logic runs in shared Rust on device and emulator

## System Architecture

### Core Direction
- Routing is a two-plane system:
  - Control plane: companion app plans routes, owns provider integrations, and publishes route updates.
  - Runtime plane: shared Rust follows the active route, drives alerts, and renders route UI.
- ESP is optimized for on-ride responsiveness and reliability, not full route planning.
- Offline operation target is route follow and alerting after a successful sync.

### Routing Sources and Strategy
Provider picker supports:
- HSL Digitransit
- Google ingest path
- OSM-based routing
- GPX import
- FIT import
- TCX import
- Garmin direct API integration
- Garmin file import path

Source strategy rules:
- all provider outputs must normalize into one source-agnostic `RoutePackage` contract
- provider-specific fields stay in provenance metadata, not in shared runtime behavior logic
- if source capability differs, normalize at companion layer, never in firmware adapter logic

Google governance rule:
- Google ingest is intentionally included as a product decision
- compliance and legal risk is explicitly accepted and tracked as a parallel governance stream
- release gating must include compliance sign-off for any Google-backed on-device rendering path

## Ownership Guards

### Companion App
Controls:
- provider credentials and auth flows
- route planning requests and reroute decisions
- source payload parsing and normalization
- route package versioning and sync orchestration

Implements:
- provider adapter stack
- provider picker UX
- normalized `RoutePackage` publisher
- sync state UX and failure recovery UX

Must not:
- implement route-follow math used by runtime-core
- implement render styling logic that belongs to render-core
- bypass route package schema and push provider-native payloads directly to adapters

### `runtime-core`
Controls:
- route-follow state machine
- snapped route progress projection
- off-route detection and hysteresis
- major-turn alert trigger timing
- alert policy evaluation

Implements:
- deterministic route-follow computations in schedule
- runtime route state in output snapshot
- reroute request event surfacing

Must not:
- call network/provider APIs
- parse provider-specific route formats
- own transport packetization or BLE/Wi-Fi session details

### `render-core`
Controls:
- route and alert visualization behavior
- route line layering and style policy
- completed versus remaining route rendering
- alert component layout and readability

Implements:
- map-first route highlight visuals
- off-route and major-turn overlay rendering

Must not:
- decide reroute
- compute progress state
- parse route package transport payloads

### `map-runtime`
Controls:
- map query and geometry lookup

Implements:
- shared map geometry access for route overlay rendering context

Must not:
- perform route planning
- decide alerts or route-follow policy

### Firmware + WASM Adapters
Controls:
- transport ingress and egress only
- platform IO integration with shared contracts

Implements:
- message bridging into `RuntimeInputFrame` and route ingress contracts
- output forwarding to platform surfaces

Must not:
- implement routing business logic
- fork behavior between device and emulator beyond platform IO constraints

## Control and Data Flow
1. User chooses provider and destination in companion app.
2. Companion app requests/plans route from selected source.
3. Companion app normalizes source payload into `RoutePackage`.
4. Companion app syncs package to ESP over BLE first, Wi-Fi optional.
5. Adapter ingests package into shared runtime route state.
6. `runtime-core` computes progress, off-route state, and major-turn alert events.
7. `render-core` draws route highlight and alert overlays.
8. If off-route is detected, runtime emits reroute request context.
9. Companion app performs reroute and pushes replacement `RoutePackage`.

## Public Contracts

### Route Package Contract
- `RoutePackage` is source-agnostic and versioned.
- Contains:
  - route identity and version
  - geometry polyline payload
  - maneuver list with distance context and maneuver classification
  - summary metadata (distance, duration, destination labels)
  - source provenance metadata and generation timestamp
- Compatibility policy:
  - backward-compatible additions only for minor version changes
  - explicit migration path for breaking version bumps

### Sync Message Contract
Message types:
- `set`
- `update`
- `clear`
- `status`
- `reroute_request`

Transport contract requirements:
- chunking and sequence integrity
- checksum verification
- resume and retry semantics
- idempotent handling for duplicate message delivery

### Runtime Contract Extensions
`RuntimeInputFrame` extension direction:
- route lifecycle ingress events
- sync/transport status ingress where needed for UI

`RuntimeFrameOutput` extension direction:
- active route follow state
- off-route status
- next major-turn alert payload
- route summary fields needed by overlay

### Alert Policy Contract
- Default policy: off-route plus major-turn alerts.
- Policy values must be runtime-configurable and forward-compatible.
- Future policy levels must extend the same contract without replacing it.

## Implementation Checkpoint (March 31, 2026, Transport Fault-Hardening Slice)
- Hardened the firmware route-sync platform loop so malformed or corrupted inbound transfers emit explicit `retryable_failure` or `fatal_failure` sync statuses instead of aborting the frame loop.
- Added deterministic transport reliability coverage for resumed interrupted transfers, out-of-order chunk delivery, checksum mismatch handling, and malformed payload handling.
- Preserved the shared ownership rule that transport adapters report sync-state outcomes while runtime/render remain isolated from transport corruption details.

## Implementation Checkpoint (April 1, 2026, Security Coverage Expansion Slice)
- Expanded repository CVE coverage to include Android companion dependency tracking through Gradle dependency submission.
- Declared `emulator/web/package-lock.json` as the canonical JavaScript security lockfile so npm-based audit results stay authoritative even while `bun.lock` remains present for local tooling.
- Added a dependency-manifest guard so future third-party iOS or other unmanaged dependency ecosystems cannot land without CI security coverage in the same change.

## Implementation Checkpoint (March 31, 2026, Companion BLE Fault-Injection Slice)
- Added live companion-side BLE fault injection controls for retryable interruption, write failure, disconnect-after-chunk, and dropped inbound status paths in both iOS and Android app shells so transport recovery can be exercised without changing firmware behavior.
- Hardened the native BLE sync services so live disconnects and write failures remain resumable pending transfers instead of collapsing into idle state or incorrectly falling through to simulated success.
- Added acknowledgement timeout recovery that rewinds completed chunk uploads into a replayable failed transfer when the final device status never arrives, aligning companion behavior with the same deterministic retry posture already enforced in firmware.

## Implementation Checkpoint (March 31, 2026, Companion Provider Matrix Slice)
- Added sample-backed companion adapters for OSM, Google ingest, GPX import, FIT import, TCX import, Garmin API, and Garmin file flows in both native apps so every planned provider slot now produces a normalized `RoutePackage` through the same preview/send path, even where live integration is still pending.
- Updated the native planning UIs so non-HSL providers are selectable as sample previews rather than dead "coming soon" states, while preserving the distinction that only HSL is live-provider-ready today.
- Kept provider provenance explicit in each normalized package and planning notice so the route-follow/runtime stack continues to receive source-agnostic payloads while companion UX stays honest about which integrations are sample-backed.

## Implementation Checkpoint (March 31, 2026, Live BLE Integration Slice)
- Integrated real CoreBluetooth central/client wiring in the iOS companion app, including scan, connect, GATT service discovery, route chunk writes, notification subscription, and Bluetooth permission prompts/usage descriptions.
- Integrated real Android BLE/GATT central wiring in the Android companion app, including permission-gated scan/connect, characteristic discovery, route chunk writes, notification subscription, and on-device permission request UI.
- Added an ESP-IDF BLE/GATT server adapter around the fixed route-sync service/characteristic contract and exposed firmware builders that can construct a device platform with route-sync IO instead of the null transport. On BLE-capable ESP-IDF targets the device entrypoint can now boot a headless route-sync service directly; the Waveshare ESP32-P4 board remains blocked on its external radio path because standard `esp-idf-svc::bt` Bluedroid APIs are not available on `esp32p4`.
- Extended the firmware platform loop to publish outbound `reroute_request` messages over the same sync-message channel the companions already decode, so live BLE now covers both route activation and reroute escalation.

## Implementation Checkpoint (March 31, 2026, Live HSL Companion Slice)
- Upgraded both native companion apps from fixture-only planning shells into configurable HSL clients that can use live Digitransit routing when a subscription key is present, while falling back to sample routes with explicit planning notices when live routing is disabled or unavailable.
- Added native settings flow for live-versus-sample HSL mode plus subscription-key entry so the planning source is visible and controllable inside the companion apps themselves.
- Added route-alternative selection and rider-location-driven reroute controls in both native apps so reroute publishing is now exercised through an explicit companion UX instead of a hidden first-alternative demo path.

## Implementation Checkpoint (March 31, 2026, Companion BLE Contract Parity Slice)
- Mirrored the fixed BLE route-sync GATT contract into both native companion apps with shared service/characteristic UUIDs plus companion-side packet framing for `chunk` and `sync_message` traffic.
- Refactored the native simulated BLE sync services to source canonical payload bytes and checksum generation from the shared packet codec layer, reducing drift between companion payload construction and firmware decoding.
- Added native companion-side decode paths for inbound `status` and `reroute_request` messages through the same BLE packet envelope, so future CoreBluetooth and Android BLE adapters can plug into an already-shaped packet contract instead of raw string payload helpers.

## Implementation Checkpoint (March 31, 2026, BLE Wire Contract Slice)
- Added a first concrete BLE route-sync wire contract with fixed service and characteristic UUIDs plus a deterministic packet envelope for `chunk` and `sync_message` traffic.
- Implemented firmware-side BLE packet encode/decode for inbound route chunks and outbound canonical sync messages so future CoreBluetooth, Android BLE, and ESP-IDF adapters can share one packet format instead of inventing platform-specific ones.
- Tightened canonical route payload fidelity by preserving maneuver `distance_to_next_m` while keeping decode backward-compatible with older payloads that omitted that field.
- Added deterministic firmware tests for BLE packet round-trips, including chunk packets, route-set sync messages, and reroute request sync messages.

## Implementation Checkpoint (March 31, 2026, Firmware Route Sync Platform Bridge Slice)
- Added a platform-level route sync IO seam in firmware so device adapters can poll inbound route chunks and publish outbound route status messages without embedding route business logic into the BLE layer.
- Wired `RuntimePlatform` to drain inbound route chunks before each frame, feed accepted route messages into the app/runtime loop, and flush accepted/applying/active status batches back through the transport seam after frame execution.
- Added deterministic firmware tests proving the platform bridge can carry a full chunked `set` transfer through route sync reassembly, runtime activation, and status publication using the same route-sync contract the future BLE backend will use.

## Implementation Checkpoint (March 31, 2026, Firmware Route Sync Device Slice)
- Added a firmware-side route sync transport module that reassembles chunked route payloads, verifies payload checksums, validates route package compatibility, and parses inbound `set`, `update`, and `clear` messages into shared `runtime-core` contracts.
- Added device-side route lifecycle guards for stale revision rejection, duplicate replay dedupe, conflicting same-revision payload detection, and explicit applied-route status transitions so firmware now owns its side of route version integrity instead of trusting the companion blindly.
- Wired the firmware app loop to ingest pending route sync messages into `RuntimeInputFrame`, apply them through shared runtime state, and emit post-apply `active` and `cleared` status messages for platform adapters to forward back over transport.
- Added deterministic firmware tests covering chunk reassembly, checksum mismatch rejection, duplicate replay handling, stale revision rejection, clear handling, and end-to-end app integration of a chunked route transfer into runtime output state.

## Implementation Checkpoint (March 31, 2026, Chunked Sync Session Slice)
- Upgraded the native iOS and Android BLE sync seams from immediate route activation stubs into chunked transfer sessions with payload sizing, chunk counts, checksum tracking, pending-route lifecycle, and active-route checksum visibility.
- Added deterministic retryable-interruption simulation plus explicit resume handling in both native apps so an interrupted transfer can continue from the pending chunk instead of restarting blindly.
- Added route-version lifecycle handling in the native transport seam, including stale-revision rejection, duplicate replay dedupe, and conflicting-payload detection for same-revision transfers.
- Updated native device and settings surfaces to expose transfer telemetry and pending-versus-active route state for manual verification while the real BLE backends are active.

## Implementation Checkpoint (March 30, 2026, Native Sync Contract Slice)
- Added explicit native companion sync message models for `set`, `update`, `clear`, `status`, and `reroute_request` in both iOS and Android app domains.
- Upgraded native BLE sync services to track outbound and inbound message state, active route version, status codes, and clear versus replace lifecycle transitions.
- Updated native app models so route publishing now chooses between `set` and `update` based on active synced route ownership, and reroute simulation now enters through an explicit `reroute_request` message.
- Updated native device/settings surfaces to expose message flow and sync lifecycle state directly for manual verification.

## Implementation Checkpoint (March 30, 2026, HSL Normalization Slice)
- Upgraded native companion route package models in `companion-ios` and `companion-android` to carry geometry, maneuvers, summary, version, and provenance instead of summary-only placeholders.
- Implemented Digitransit-shaped HSL request builders in both native apps so the provider seam now has a concrete GraphQL request contract for route planning.
- Implemented fixture-backed HSL itinerary normalization in both native apps so route alternatives now embed fully normalized route packages with geometry and maneuver metadata.
- Updated native preview and ride surfaces to expose normalized route package details, including geometry-point count, maneuver count, provenance, and destination context.
- Updated native BLE sync stubs to report normalized route package transfer summaries instead of placeholder counts.

## Implementation Checkpoint (March 30, 2026)
- Added native companion app scaffolds in `companion-ios` and `companion-android` with explicit presentation/domain/integration boundaries.
- Added iOS SwiftUI app shell covering launch, planning, preview, device, ride, and settings surfaces.
- Added Android Jetpack Compose app shell covering launch, planning, preview, device, ride, and settings surfaces.
- Added first HSL provider seam in both native apps with demo-backed planning, reroute, and normalization stubs behind provider interfaces.
- Added first BLE sync seam in both native apps with explicit route send, clear, connect, and sync-state boundaries behind transport interfaces.
- Added persistence and diagnostics seams in both native apps so route history, active session state, and sync/reroute reporting have a stable ownership home before production integration.

## Implementation Checkpoint (March 29, 2026)
- Delivered versioned route package and sync contracts in shared `runtime-core` API.
- Added provider fixture contract coverage (HSL, Google ingest, OSM, GPX, FIT, TCX, Garmin API/file).
- Extended `RuntimeInputFrame` with route sync ingress and `RuntimeFrameOutput` with active route render state baseline.
- Added runtime route state application for `set`/`update`/`clear` flows and deterministic tests for set/clear behavior.
- Implemented snapped route progress projection in `runtime-core`, including monotonic progress behavior under backward GPS jitter.
- Added hysteresis-based off-route detection in `runtime-core` with deterministic recovery once the rider returns within the exit threshold.
- Added major-turn alert timing in `runtime-core` based on upcoming maneuver distance and major-turn classification.
- Added a reroute-request surface in `runtime-core` that escalates sustained off-route state into a distinct rerouting state.
- Added completed-versus-remaining route rendering in `render-core` so route follow state is visually inspectable while riding.
- Added a first map-first off-route warning banner in `render-core` for visible route deviation feedback.
- Added a first major-turn banner in `render-core` showing turn direction plus remaining meters.
- Added a rerouting banner in `render-core` so sustained off-route state is visibly distinguishable from transient deviation.
- Added deterministic runtime coverage for replacement-route activation after reroute request.
- Added a shared route alert verbosity contract in `runtime-core` so policy controls live in shared config instead of emulator-only behavior.
- Added configurable alert verbosity rendering in `render-core`, including suppression of major-turn banners in `essential` mode and expanded banner phrasing in `detailed` mode.
- Added viewport-fitting banner text and render coverage for small-view and rotated-camera readability.
- Wired emulator wasm bridge + web program to inject a Helsinki demo route with quick turns on first GPS sample, to simulate delayed reroute replacement when the runtime requests rerouting, and to accept `?routeAlerts=essential|standard|detailed` for visible alert policy checks.

## Program Phases

### Stage 1: HSL-First Routing Vertical Slice (Detailed)
Goal:
- ship a complete Helsinki-quality routing loop from planning to on-device following using HSL as the first production provider

Implementation lanes:
- Companion planning lane:
  - implement `HslRoutingAdapter` against Digitransit routing API for bike routes
  - support origin and destination as lat/lon inputs with rider profile defaults
  - map HSL itinerary response into internal provider model before normalization
- Normalization lane:
  - normalize HSL geometry into `RoutePackage.geometry`
  - normalize maneuver semantics into canonical maneuver types and distance fields
  - attach source provenance as `provider = hsl_digitransit` with generation timestamp
- Sync lane:
  - send normalized HSL route via `set` message and wait for explicit device ack/status
  - handle chunking, checksum, and retry semantics for first-route sync
- Runtime lane:
  - activate route-follow state from synced package
  - compute progress projection and off-route detection baseline
  - emit reroute request event when off-route hysteresis threshold is crossed
- Render lane:
  - render primary route highlight over map
  - render off-route warning and major-turn alert overlay in map-first style
- Failure-handling lane:
  - no-route and timeout errors become companion-level actionable states
  - if HSL route payload lacks maneuver detail, fallback to geometry-only following with off-route alerts

Stage 1 exit criteria:
- HSL route can be planned, normalized, synced, and followed end-to-end on emulator and device
- off-route detection triggers companion reroute request loop successfully
- route visualization and alerts remain readable while riding in Helsinki field validation runs
- deterministic tests cover HSL payload normalization, runtime follow behavior, and sync retry handling

### Stage 2: Contracts and Multi-Provider Scaffold
- Generalize provider adapter interface from HSL-first implementation
- Harden route package versioning and compatibility policy
- Add provider fixture corpus for OSM, Google ingest, GPX/FIT/TCX, Garmin API/import

Exit criteria:
- provider interface is stable and source-agnostic
- route package fixtures validate against schema across all target source types

### Stage 3: Sync, Persistence, Runtime Hardening
- Expand BLE-first sync protocol to full lifecycle (`set`, `update`, `clear`, `status`, `reroute_request`)
- add route persistence and route version lifecycle handling
- harden runtime route-follow and reroute state transitions

Exit criteria:
- interrupted sync recovers without route corruption
- route can be persisted, restored, and replaced deterministically
- runtime route states remain deterministic under dropout and reroute scenarios

### Stage 4: Rendering and Alert UX Expansion
- refine route styling for completed versus remaining segments
- refine configurable alert policy behavior and rendering
- validate readability across zoom/orientation/motion transitions

Exit criteria:
- route line and alerts are consistently legible in riding states
- alert policy toggles are reflected deterministically in runtime and render output

### Stage 5: Multi-Provider Hardening and Field Validation
- harden non-HSL adapters and normalization edge handling
- execute field validation for Helsinki and fallback-provider scenarios
- validate cross-source reroute behavior

Exit criteria:
- provider fallback behavior is deterministic
- field scenarios pass for target riding conditions and reroute loops

### Stage 6: Productionization, Observability, Compliance Closure
- add telemetry and diagnostics for planning, sync, follow, and reroute lifecycle
- complete compliance sign-off workstream, including Google path governance
- finalize rollout and support playbook

Exit criteria:
- operational dashboards and incident procedures are defined
- compliance gate sign-off is recorded
- launch checklist is complete

## Quality and Validation Strategy
- Contract conformance tests across all providers.
- Sync reliability tests under packet loss and interrupted sessions.
- Runtime determinism tests for route progress and off-route stability.
- Rendering snapshot and readability tests across motion/orientation states.
- End-to-end tests for HSL, OSM fallback, Google ingest, GPX import, Garmin API/import, and live reroute loop.

## Explicit Program Assumptions
- Implementation effort is unconstrained; UX and performance are top priorities.
- Companion app is the orchestration authority for planning and rerouting.
- ESP does not execute full route planning in v1.
- Offline target is route follow and alerts after successful sync.
- Existing shared-Rust ownership boundaries are extended, not replaced.
