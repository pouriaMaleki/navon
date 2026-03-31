# Routing Program TODO

Spec reference: [`project-spec.md`](./project-spec.md)
Plan reference: [`current-plan.md`](./current-plan.md)
Contract reference: [`route-package-contract.md`](./route-package-contract.md)

## Delivery Order
- Phase alignment: Epic A -> Epic B -> Epic C -> Epic D -> Epic E -> Epic F
- Parallelization rule: only run epics in parallel when dependencies are explicitly marked satisfied
- Ownership rule: companion controls planning and reroute, shared runtime controls following and alert state, render-core controls route visuals

## Epic A: Route Package Contract
Depends on:
- None

### Work Items
- [x] Define a versioned source-agnostic `RoutePackage` schema.
- [x] Define maneuver model, route summary model, and provenance metadata model.
- [x] Define compatibility policy for schema evolution.
- [x] Create canonical fixture set for HSL, Google ingest, OSM, GPX, FIT, TCX, and Garmin inputs.
- [x] Document contract invariants and normalization guarantees.

### Definition of Done
- [x] Schema supports all listed providers without provider-specific runtime branching.
- [x] Fixture corpus validates cleanly against schema.
- [x] Backward compatibility rules are documented with explicit migration path for breaking changes.
- [x] Contract documentation is sufficient for independent client implementation.

## Epic B: Companion Routing Orchestrator
Depends on:
- Epic A

### Work Items
- [x] Scaffold native iOS and Android companion app shells with presentation/domain/integration boundaries.
- [x] Build provider adapter interface and adapter lifecycle contract.
- [ ] Implement provider adapters for HSL, Google ingest, OSM, GPX/FIT/TCX, Garmin API/import.
- [x] Implement HSL normalization pipeline from provider payload shape to `RoutePackage`.
- [x] Implement provider picker UX with source provenance visibility.
- [ ] Implement reroute orchestration and replacement route publishing.

Current checkpoint:
- Native app shells exist in `companion-ios` and `companion-android`.
- HSL now supports live Digitransit planning with in-app subscription-key configuration plus explicit fallback to sample routes when live routing is unavailable.
- Native preview screens now let the user choose a specific route alternative before sync instead of silently sending the first alternative.
- Native ride/device screens now let the user drive reroute publishing from an explicit rider location, so replacement-route generation no longer depends on a hidden origin-only demo path.
- Native BLE seams now have real CoreBluetooth and Android BLE/GATT clients wired into the same chunked transfer/session model that was originally simulated, while preserving the simulated fallback path when no BLE connection is active.
- Native device screens now expose live BLE fault-injection controls for retryable interruption, write failure, disconnect-after-chunk, and dropped inbound status so packet-loss and ack-loss recovery can be exercised against the same chunked session model used by real CoreBluetooth and Android BLE clients.
- Next implementation step is end-to-end device validation against the ESP GATT server on BLE-capable hardware, followed by the ESP32-P4 external-radio path and remaining provider breadth.

### Definition of Done
- [ ] Destination-to-`RoutePackage` generation works for each launch provider path.
- [ ] Provider failures produce deterministic fallback or actionable user error states.
- [ ] Reroute requests produce replacement packages without app restart.
- [ ] Output route packages are byte-for-byte schema-valid against Epic A contracts.

## Epic C: Device Sync Transport
Depends on:
- Epic A
- Epic B

Current checkpoint:
- Native BLE transport seams in iOS and Android now use real CoreBluetooth / Android BLE packet IO when a compatible ESP32 route-sync peripheral is available, while preserving the existing chunked transfer/session state model.
- Firmware now owns device-side route sync reassembly, checksum verification, stale-revision rejection, duplicate replay dedupe, conflicting same-revision detection, runtime route ingress, platform-to-runtime status publication, and outbound reroute-request publication.
- The BLE wire contract is now fixed in code and docs across firmware plus both native companion apps, including service/characteristic UUIDs, packet envelopes for `chunk` and `sync_message` traffic, and actual GATT adapter implementations on all three sides where the target silicon exposes standard BLE APIs. The ESP32-P4 production board still needs its external-radio path wired separately.
- Deterministic firmware fault-injection coverage now exists for interrupted transfer resume, out-of-order chunks, checksum mismatch handling, and malformed payload handling.
- Native companion apps now expose matching live adapter fault-injection controls and acknowledgement-timeout recovery so the phone-side transport loop can be validated against the same failure classes.
- Next implementation step is field validation against real ESP hardware, then the ESP32-P4 external-radio path and optional Wi-Fi transport.

### Work Items
- [x] Define message protocol for `set`, `update`, `clear`, `status`, and `reroute_request`.
- [x] Implement BLE-first transport with route chunking and flow control.
- [x] Implement checksum verification, dedupe, and idempotent replay handling.
- [x] Implement transfer resume/retry for interrupted sessions.
- [ ] Add optional Wi-Fi transport path behind same message contract.
- [x] Add route version lifecycle handling on device.

### Definition of Done
- [x] Route transfer remains correct under interrupted sessions and resumed transfer.
- [x] Duplicate packets do not corrupt route state.
- [x] Checksum mismatch is detected and recovered with deterministic retry behavior.
- [x] Device route state always reflects last acknowledged route version.

## Epic D: Shared Runtime Route-Follow Engine
Depends on:
- Epic A
- Epic C

### Work Items
- [x] Extend runtime input contracts with route lifecycle events.
- [x] Implement route progress projection over active route geometry.
- [x] Implement off-route detection with hysteresis to prevent oscillation.
- [x] Implement major-turn alert trigger state and timing policy.
- [x] Implement runtime reroute request event surface.
- [x] Extend runtime output contracts with route follow state (active route geometry/state baseline).
- [x] Add alert policy config contract for off-route and major-turn behavior.

### Definition of Done
- [x] Runtime route follow behavior is deterministic for identical input sequences for route activation and progress projection scenarios.
- [x] Off-route detection is stable in deterministic runtime scenarios with explicit enter/exit hysteresis coverage.
- [x] Major-turn alerts trigger consistently in deterministic runtime scenarios with route-progress-driven maneuver switching.
- [x] Runtime output contains all route state needed by render for route progress, off-route, reroute request, and major-turn alert behavior.

## Epic E: Shared Rendering and UX
Depends on:
- Epic D

### Work Items
- [x] Implement route highlight rendering in `render-core`.
- [x] Implement completed versus remaining route segment styling.
- [x] Implement map-first off-route alert visualization.
- [x] Implement map-first major-turn alert visualization.
- [x] Implement configurable alert verbosity plumbing and rendering behavior.
- [x] Ensure route/alert readability across zoom and orientation modes.

### Definition of Done
- [x] Route highlight stays clearly visible in riding presentation bands for the current emulator/device-parity render path.
- [x] Off-route and major-turn alerts are legible with existing overlay stack.
- [x] Alert verbosity settings map to deterministic rendering behavior.
- [x] Firmware and emulator render behavior remains parity-consistent for route overlays in the shared Rust + wasm bridge path.

## Epic F: Validation and Field Readiness
Depends on:
- Epic A
- Epic B
- Epic C
- Epic D
- Epic E

### Work Items
- [x] Add provider contract conformance tests.
- [x] Add sync reliability and fault-injection tests for packet loss/interruption.
- [x] Add runtime scenario tests for route progress, off-route, and reroute replacement.
- [x] Add rendering snapshot/readability tests for route and alerts.
- [ ] Execute Helsinki field validation rides with HSL-first scenarios.
- [ ] Execute cross-source validation for OSM fallback, Google ingest, GPX, and Garmin flows.
- [ ] Add compliance validation checks and release gates for Google ingest path.

### Definition of Done
- [ ] All provider adapters pass conformance test suite.
- [x] Sync transport passes reliability suite with deterministic recovery behavior.
- [x] Runtime and render route behavior pass deterministic regression suites for the current route-follow, alert, and reroute-replacement scope.
- [ ] Field validation confirms stable follow and reroute user experience in Helsinki conditions.
- [ ] Compliance and legal gating artifacts are complete for planned launch sources.

## Program-Level Acceptance Criteria
- [ ] Companion app remains sole authority for planning and rerouting decisions.
- [ ] ESP route following works offline after successful route sync.
- [ ] Runtime and render ownership boundaries are preserved with no adapter-owned routing logic.
- [ ] End-to-end route loop works across HSL, OSM fallback, Google ingest, GPX import, Garmin API/import.
- [ ] Observability and operational diagnostics are available for sync, follow, off-route, and reroute lifecycle.
