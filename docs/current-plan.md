# Routing Program Technical Plan

Spec reference: [`project-spec.md`](./project-spec.md)
Runtime architecture reference: [`runtime-ecs-architecture.md`](./runtime-ecs-architecture.md)
Execution guide reference: [`framework-execution-guide.md`](./framework-execution-guide.md)
Route contract reference: [`route-package-contract.md`](./route-package-contract.md)

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
