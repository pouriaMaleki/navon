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
- [ ] Build provider adapter interface and adapter lifecycle contract.
- [ ] Implement provider adapters for HSL, Google ingest, OSM, GPX/FIT/TCX, Garmin API/import.
- [ ] Implement normalization pipeline from each provider payload to `RoutePackage`.
- [ ] Implement provider picker UX with source provenance visibility.
- [ ] Implement reroute orchestration and replacement route publishing.

### Definition of Done
- [ ] Destination-to-`RoutePackage` generation works for each launch provider path.
- [ ] Provider failures produce deterministic fallback or actionable user error states.
- [ ] Reroute requests produce replacement packages without app restart.
- [ ] Output route packages are byte-for-byte schema-valid against Epic A contracts.

## Epic C: Device Sync Transport
Depends on:
- Epic A
- Epic B

### Work Items
- [ ] Define message protocol for `set`, `update`, `clear`, `status`, and `reroute_request`.
- [ ] Implement BLE-first transport with route chunking and flow control.
- [ ] Implement checksum verification, dedupe, and idempotent replay handling.
- [ ] Implement transfer resume/retry for interrupted sessions.
- [ ] Add optional Wi-Fi transport path behind same message contract.
- [ ] Add route version lifecycle handling on device.

### Definition of Done
- [ ] Route transfer remains correct under interrupted sessions and resumed transfer.
- [ ] Duplicate packets do not corrupt route state.
- [ ] Checksum mismatch is detected and recovered with deterministic retry behavior.
- [ ] Device route state always reflects last acknowledged route version.

## Epic D: Shared Runtime Route-Follow Engine
Depends on:
- Epic A
- Epic C

### Work Items
- [x] Extend runtime input contracts with route lifecycle events.
- [x] Implement route progress projection over active route geometry.
- [x] Implement off-route detection with hysteresis to prevent oscillation.
- [ ] Implement major-turn alert trigger state and timing policy.
- [ ] Implement runtime reroute request event surface.
- [x] Extend runtime output contracts with route follow state (active route geometry/state baseline).
- [ ] Add alert policy config contract for off-route and major-turn behavior.

### Definition of Done
- [x] Runtime route follow behavior is deterministic for identical input sequences for route activation and progress projection scenarios.
- [x] Off-route detection is stable in deterministic runtime scenarios with explicit enter/exit hysteresis coverage.
- [ ] Major-turn alerts trigger consistently with configured thresholds.
- [ ] Runtime output contains all route state needed by render without adapter-owned logic.

## Epic E: Shared Rendering and UX
Depends on:
- Epic D

### Work Items
- [x] Implement route highlight rendering in `render-core`.
- [x] Implement completed versus remaining route segment styling.
- [x] Implement map-first off-route alert visualization.
- [ ] Implement map-first major-turn alert visualization.
- [ ] Implement configurable alert verbosity plumbing and rendering behavior.
- [ ] Ensure route/alert readability across zoom and orientation modes.

### Definition of Done
- [ ] Route highlight stays clearly visible in riding presentation bands.
- [ ] Off-route and major-turn alerts are legible with existing overlay stack.
- [ ] Alert verbosity settings map to deterministic rendering behavior.
- [ ] Firmware and emulator render behavior remains parity-consistent for route overlays.

## Epic F: Validation and Field Readiness
Depends on:
- Epic A
- Epic B
- Epic C
- Epic D
- Epic E

### Work Items
- [x] Add provider contract conformance tests.
- [ ] Add sync reliability and fault-injection tests for packet loss/interruption.
- [~] Add runtime scenario tests for route progress, off-route, and reroute replacement. Progress projection and off-route hysteresis coverage are in place; reroute scenarios remain.
- [~] Add rendering snapshot/readability tests for route and alerts. Route overlay and off-route banner coverage are in place; broader readability/snapshot coverage remains.
- [ ] Execute Helsinki field validation rides with HSL-first scenarios.
- [ ] Execute cross-source validation for OSM fallback, Google ingest, GPX, and Garmin flows.
- [ ] Add compliance validation checks and release gates for Google ingest path.

### Definition of Done
- [ ] All provider adapters pass conformance test suite.
- [ ] Sync transport passes reliability suite with deterministic recovery behavior.
- [ ] Runtime and render route behavior pass deterministic regression suites.
- [ ] Field validation confirms stable follow and reroute user experience in Helsinki conditions.
- [ ] Compliance and legal gating artifacts are complete for planned launch sources.

## Program-Level Acceptance Criteria
- [ ] Companion app remains sole authority for planning and rerouting decisions.
- [ ] ESP route following works offline after successful route sync.
- [ ] Runtime and render ownership boundaries are preserved with no adapter-owned routing logic.
- [ ] End-to-end route loop works across HSL, OSM fallback, Google ingest, GPX import, Garmin API/import.
- [ ] Observability and operational diagnostics are available for sync, follow, off-route, and reroute lifecycle.
