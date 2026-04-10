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
- [x] Create canonical fixture set for HSL, OSM, GPX, FIT, and TCX inputs.
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
- [ ] Implement provider adapters for HSL, OSM, and GPX/FIT/TCX imports.
  Current status: HSL is live-provider-ready, GPX now imports natively in iOS and Android companion apps plus emulator/web, and the remaining file/import flows are still sample-backed until their live adapters land.
- [x] Implement HSL normalization pipeline from provider payload shape to `RoutePackage`.
- [x] Implement provider picker UX with source provenance visibility.
- [ ] Implement reroute orchestration and replacement route publishing.
- [~] Redesign companion information architecture into the planned single-surface Home plus full-screen Settings model.
  Current status: root navigation shells were replaced in both native apps, and the remaining work is feature-depth completion plus cleanup of the underlying state architecture.
- [ ] Implement share-sheet ingestion UX for Google Maps links, shared locations, GPX, and future routing payloads.
- [ ] Add Google Maps handoff UX that imports destination/route intent and replans it through the companion's own provider stack.
- [~] Move route-source selection for the active planning session into the Home planning UI as a compact top control with `Mixed` as the default, instead of relying only on Settings.
  Current status: the redesign implementation is moving source selection into Home in both native apps, with `Mixed`, `HSL`, and `OSM` as the active planning choices.
- [~] Define the planning-mode versus guidance-mode transition explicitly so `Start` commits to one route and `Stop` returns to refreshed suggestions from the current location.
  Current status: the redesign now targets an explicit split between planning mode, phone riding mode, and device overview mode, with device-connected `Start` becoming device-first.
- [~] Add Home-tab long-press destination selection on the map and route-suggestion flow parity with typed search.
  Current status: both native shells now support long-press destination selection and the shared three-route preview flow, while deeper share-import parity still remains.
- [~] Implement Home search UX with recents by default, live suggestions while typing, and paged loading in both states.
  Current status: both native shells now show recents by default, swap to native search suggestions while typing, and page the visible lists locally, but import fast-path and native toolchain verification still remain.
- [~] Design full-screen Settings information architecture with `Routes`, `Device`, `Route Planner`, and `Import Diagnostics` sections.
  Current status: the full-screen settings hub now exists in both native shells, with `Import Diagnostics` added and the remaining work focused on feature-depth completion plus native-toolchain validation.
- [x] Specify Home screen states: idle map, recents, live suggestions, long-press destination, route preview, and active guidance.
- [~] Specify one universal route detail page reused across imports and recents.
  Current status: both native shells now use a universal route-detail surface for route-history items, while unsupported share payloads fall back to `Import Diagnostics`.
- [x] Define share-sheet handling rules for fast-path-to-Home versus fallback-to-route-detail behavior.
- [~] Build the Settings `Routes` page as a lightweight recovery and re-entry surface for imported routes, destination history, and route-intent recovery, not as a heavy route library.
  Current status: both native shells now expose a lightweight `Routes` surface with GPX import, shared destination recovery, and recent route-history recovery, while deeper share-import polish still remains.
- [ ] Finish share-sheet import fast paths, diagnostics export, and retry flows across both native apps.
- [ ] Keep outbound ride recording/export out of the near-term companion redesign scope.

Current checkpoint:
- Native app shells exist in `companion-ios` and `companion-android`.
- HSL now supports live Digitransit planning with in-app subscription-key configuration plus explicit fallback to sample routes when live routing is unavailable.
- Native preview screens now let the user choose a specific route alternative before sync instead of silently sending the first alternative.
- Native ride/device screens now let the user drive reroute publishing from an explicit rider location, so replacement-route generation no longer depends on a hidden origin-only demo path.
- Native BLE seams now have real CoreBluetooth and Android BLE/GATT clients wired into the same chunked transfer/session model that was originally simulated, while preserving the simulated fallback path when no BLE connection is active.
- Native GPX import now works through the iOS and Android document pickers, feeding imported routes into the same preview and sync flow used by planned routes.
- The native redesign shell is now in place: map-first Home on both platforms, long-press destination selection, three-route suggestion preview, universal route-detail surface, and a full-screen Settings hub with `Routes`, `Device`, `Route Planner`, and `Import Diagnostics` sections.
- Native device screens now expose live BLE fault-injection controls for retryable interruption, write failure, disconnect-after-chunk, and dropped inbound status so packet-loss and ack-loss recovery can be exercised against the same chunked session model used by real CoreBluetooth and Android BLE clients.
- All remaining provider slots now have sample-backed companion adapters that produce normalized route packages through the shared preview/send path, while only HSL is live-provider-ready today.
- Hardware validation is deferred until a BLE-capable device is available. Near-term implementation should focus on optional Wi-Fi transport, live provider integrations beyond HSL, and companion-product expansion work.
- Once hardware is available, the next hardware milestone remains end-to-end device validation against the ESP GATT server on BLE-capable hardware, followed by the ESP32-P4 external-radio path.
- Planned companion expansion notes: standalone map/routing usability, route planning from current location to a destination selected on the device map, share-sheet ingestion for locations/GPX/related payloads, and a substantial future UI/UX redesign.
- Planned companion redesign target is a single map-first Home surface plus a full-screen Settings hub, not a tabbed app shell.
- Architecture doc `companion-app-architecture.md` now captures the redesign boundaries and guardrails.
- Planned Home route flow: destination search in a top map overlay, recents shown by default, live dropdown results while typing, tap-and-hold on the map to drop a destination pin, three suggested routes after destination selection, best route preselected by default, explicit `Start` to begin guidance, and explicit `Stop` to end guidance and re-suggest routes from the rider's new current location.
- Planned route-option UX: planning mode should default to a `Mixed` source strategy, expose a small inline source control near `Where to?`, label route options by rider-meaningful style first (`Fastest`, `Quieter`, `Simpler`) with source as secondary context, and treat `Start` as the mode shift from comparison into active guidance.
- Planned Settings `Routes` page role: imported routes, destination history, and recent route intents should live together as a clean action-oriented list with source badges and obvious next actions, while the route detail page acts as the universal fallback/re-entry surface.
- Planned share-import direction: focus on inbound share-sheet and link/file ingestion for Google Maps handoffs, shared locations, and GPX-first import flows, while leaving ride recording/export for later.
- Google Maps integration should be implemented as an intent-import and destination-handoff flow through share sheet and link parsing, not as the app pretending to mirror Google's exact route semantics; clear imports should jump to Home route preview, while ambiguous imports should open the universal route detail page.

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
- Hardware validation is deferred until a BLE-capable device is available. Until then, transport work should focus on the optional Wi-Fi path while companion-side product work continues.
- Once hardware is available, the next hardware milestone remains field validation against real ESP hardware, followed by the ESP32-P4 external-radio path.

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
- [ ] Execute cross-source validation for OSM fallback, Google Maps handoff imports, GPX import, and share-diagnostics recovery flows.

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
- [ ] End-to-end route loop works across HSL, OSM fallback, Google Maps handoff import, and GPX import.
- [ ] Observability and operational diagnostics are available for sync, follow, off-route, and reroute lifecycle.
