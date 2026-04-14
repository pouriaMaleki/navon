# Routing Program Technical Plan

Spec reference: [`project-spec.md`](./project-spec.md)
Runtime architecture reference: [`runtime-ecs-architecture.md`](./runtime-ecs-architecture.md)
Execution guide reference: [`framework-execution-guide.md`](./framework-execution-guide.md)
Route contract reference: [`route-package-contract.md`](./route-package-contract.md)
BLE sync contract reference: [`ble-route-sync-contract.md`](./ble-route-sync-contract.md)
Companion app architecture reference: [`companion-app-architecture.md`](./companion-app-architecture.md)

## Architecture Vision
Phone-orchestrated routing with ESP-optimized route following.

The product is built around a map-first riding UX:
- route highlight is the primary guidance surface
- off-route and major-turn alerts are secondary, configurable overlays
- the system can extend alert verbosity later without changing core contracts

Performance principle:
- heavy planning and rerouting stay off-device on companion app infrastructure
- deterministic low-latency route-follow logic runs in shared Rust on device and emulator


## Current implementation checkpoint (April 6, 2026)
- Durable companion architecture guidance now lives in [`companion-app-architecture.md`](./companion-app-architecture.md).
- iOS root navigation now uses a map-first Home plus full-screen Settings flow instead of the earlier tab shell.
- Android root navigation now uses a map-first Home plus full-screen Settings flow instead of the earlier tab shell.
- Companion iOS signing now expects a one-time local `companion-ios/Config/Signing.local.xcconfig` with `DEVELOPMENT_TEAM`, so generated Xcode projects survive pulls without manual team re-selection.
- Both companion apps now carry lightweight route-history and route-planner-preference models to support the redesign flow.
- Existing provider adapters, GPX import, BLE sync transport, persistence, and diagnostics seams remain the reused core beneath the new shell.
- iOS share import now normalizes file-URL shares before generic URL handling, expands Google short links, parses OpenStreetMap coordinate fragments, and feeds FIT/TCX file shares into the existing sample-backed import path instead of dropping them straight into diagnostics.
- iOS share extension now collapses multi-item share payloads down to one preferred import candidate so Google Maps URL shares are not overridden by lower-signal secondary text attachments.
- iOS shared-import debug packages now retain extension candidate ranking plus URL-resolution steps so broken share payloads can be diagnosed from one exported JSON instead of by inference.
- iOS Home now treats `homePreviewRequestID` as an explicit return-to-planning signal so shared destinations reopen the first page with the imported preview and mixed/HSL/OSM route suggestions visible instead of remaining hidden behind prior guidance state.
- iOS successful share imports now persist a pending Home presentation keyed to the created route-history item, and Home consumes that persisted handoff on launch so cold starts can still reopen directly into the imported route preview.
- iOS share extension handoff now uses an app-group queue file plus app URL-scheme reopen so Google Maps shares can bring Companion forward and trigger import consumption immediately instead of relying on a race-prone UserDefaults-only handoff.
- iOS Google Maps imports now preserve the resolved destination through the second-stage import path by preferring the resolved text payload over the raw URL when extracting coordinates for Home preview planning.
- iOS Google Maps coordinate imports now keep the shared address/title authoritative when it looks like a specific street address, instead of letting reverse geocoding rename nearby entrances or house numbers; address searches also use address results instead of POI-only matching.

## Companion Expansion Direction (April 5, 2026)
The companion app is planned to evolve beyond a device-support utility into a strong standalone navigation product.

Companion product direction:
- the companion app should become independently useful even when the ESP device is not present
- the companion app should render the map and route guidance itself, using the same route-follow and direction concepts as the device
- the companion app should support planning from the rider's current location to a destination selected directly on the device map
- the companion app should become a share-target on iOS and Android so other apps can hand it locations, GPX files, and future route/location payloads
- the companion app is expected to receive a substantial future UI and UX redesign, with implementation work planned after the current routing foundations are stable

Architecture implication:
- this expanded companion scope does not change the shared ownership boundaries: provider/network orchestration stays in the companion, while shared Rust remains the source of truth for follow logic and alert semantics where practical

## Google Maps Handoff UX (April 5, 2026)
Google Maps integration should be treated as an intent-import and destination-handoff flow, not as the companion app surrendering route ownership to Google.

Product framing:
- Google Maps is a discovery and handoff source
- the companion app remains the bike-first navigation product
- imported Google Maps content should normally become a destination or route intent that the companion app replans through its own provider stack

UX rules:
- the preferred entry point is the system share sheet from Google Maps into the companion app
- the companion app should also accept pasted Google Maps links and convert them into destination or route intent
- if the imported payload is a place or pin, the app should present it as "Imported destination from Google Maps" and immediately offer route planning from current location
- if the imported payload looks like active Google directions, the app should still frame it as a destination/route intent and communicate that the companion app will compute its own bike-optimized route
- the app should avoid implying that it is reproducing the exact Google route unless a future compliant integration explicitly supports that

User outcomes:
- ride on phone using the companion's own guidance UX
- send the route to the device
- switch providers after import without losing the imported destination context

Suggested import copy:
- "Imported from Google Maps" as the source badge
- "Destination imported" for place/pin handoff
- "Planning bike route in Companion" when the app is converting the imported Google intent into its own route

## Companion App Redesign Direction (Single-Surface Home)
The current companion shells proved the architecture and transport seams, but they are not the desired long-term product surface. The next redesign should simplify the app into a single map-first primary surface plus a full-screen settings hub.

Primary app surface:
- when the app opens, the user should land directly on the map
- the map is the product, not a step on the way to the product
- the only persistent primary control is a large touch-friendly `Where to?` input at the top
- a settings button opens the secondary full-screen settings area

Home surface behavior:
- tapping the `Where to?` input should show recent routes and destinations by default
- the initial list should render 10 items and load more when the user scrolls to the end
- typing into the input should replace recents with live destination suggestions
- destination suggestions should also render 10 items first, with more loaded as the user scrolls
- tap-and-hold on the map should drop a destination pin directly without using search
- after destination selection, whether from search, recents, or tap-and-hold, the app should show three suggested routes in a compact chooser
- the best route should be preselected by default
- the user explicitly presses `Start` to begin guidance
- while guidance is active, the user can press `Stop`, after which the app should immediately offer refreshed route suggestions from the new current location

Route suggestion behavior:
- destination selection should normally produce three suggested routes
- the default route-source mode should be `Mixed`, not a hidden settings-only choice
- `Mixed` means the app may assemble three good options from one provider, multiple providers, or future ranking strategies, while still presenting them as understandable ride choices rather than backend plumbing
- the preferred user-facing framing is route style first and source second, for example `Fastest`, `Quieter`, or `Simpler`, with a smaller source label such as `via HSL` or `via OSM`
- route selection should feel lightweight: compare, accept, start
- the product should optimize for quick everyday bike use rather than heavy route-library management
- source strategy should be adjustable directly in planning mode through a small top control near `Where to?`, not only through the Settings `Route Planner` page

Secondary navigation:
- the app should not use bottom tabs for the main experience
- instead, a settings button should open a full-screen settings area that contains the structured secondary features

Settings information architecture:

### 1. Routes
Purpose:
- full-screen list/history surface for route and destination items outside the quick Home search flow
- this is a recovery and re-entry surface, not a heavy route library or route-planner workspace

Contains:
- recently imported routes and destinations
- shared Google Maps handoffs, GPX/FIT/TCX imports, and recent share-based destination intents
- recent route intents and destination history
- clear source badges and lightweight actions such as `Open`, `Start`, `Open again`, and `Dismiss`

### 2. Device
Purpose:
- all ESP32 device connection, setup, and troubleshooting flows

Contains:
- pairing and connection state
- sync status and diagnostics
- device setup help and troubleshooting
- future device-specific controls once real hardware flow is finalized

### 3. Route Planner
Purpose:
- control the behavior of route generation and suggestion strategy

Contains:
- provider policy such as HSL, OSM, others, or mixed/all-provider mode
- route suggestion mode such as best route only versus three-route suggestions
- whether routing should start immediately by default or require explicit confirmation
- future heuristics such as history-informed route ranking

### 4. Import Diagnostics
Purpose:
- capture unsupported or ambiguous share payloads without blocking the lightweight import path

Contains:
- unsupported links, files, and mixed-share payloads
- exportable debug packages for manual inspection
- retry and dismiss actions for failed imports

Design principles for the redesign:
- the app should feel like a navigation product first, not a developer tool
- Home should be the map, not a dashboard that requires another tap to start planning
- settings pages should organize secondary features without competing with the immediacy of Home
- route preview, ride-on-phone guidance, and later device handoff should all feel like adjacent actions from the same route state, not separate apps inside one app
- current Launch / Plan / Preview / Device / Ride / Settings fragmentation should eventually collapse into this simpler single-surface home plus settings-hub model

## Concrete Screen Spec (April 6, 2026)

### Home States

#### State 1: Idle map
- full map visible
- top `Where to?` input visible
- settings button visible
- no search list open
- current location visible when available

#### State 2: Search focused with recents
- tapping `Where to?` focuses input and opens a dropdown panel below it
- default contents are recent routes and destinations
- 10 items render initially
- scrolling near the end loads more items
- tapping a recent item opens route preview immediately

#### State 3: Search focused with live suggestions
- as the user types, recents are replaced by live destination suggestions
- 10 suggestions render initially
- scrolling near the end loads more suggestions
- selecting a suggestion closes the keyboard and opens route preview

#### State 4: Long-press destination selection
- long-pressing on the map drops a destination pin
- the map recenters if needed to keep origin, route preview, and destination legible
- the same route preview state is used as with typed search

#### State 5: Route preview
- map shows the destination and three suggested routes
- one route is selected by default as the recommended best option
- the user can switch between the three routes in a lightweight chooser
- a compact source-strategy control is visible near the top in planning mode, with `Mixed` as the default and direct switching to provider-specific modes when needed
- primary action is `Start`
- secondary action is `Close` or back-to-map, not a heavy planner action

#### Planning versus guidance mode
- before `Start`, the app is in planning mode: it compares route options and lets the rider change route or source strategy freely
- pressing `Start` commits to one route and transitions into the correct active-navigation mode
- after `Start`, route-comparison clutter should disappear and the UI should focus on the active route, current guidance, and a clear `Stop` action
- pressing `Stop` ends guidance and returns the rider to refreshed route suggestions from the current location rather than leaving the app in a half-planning, half-guidance state

#### State 6: Active guidance
- map shows the active route and ride guidance
- primary interrupt action is `Stop`
- stopping guidance returns the user to refreshed route suggestions from the new current location

#### Start behavior by device state
- if no device is connected, `Start` enters full phone riding mode
- phone riding mode should use the same guidance principles as the device, including direction-of-movement orientation and north-indicator preview/lock behavior
- if a device is connected, `Start` becomes device-first, shows loading while sync is in flight, and leaves the phone in route overview / companion mode once the device has the route
- reroutes during an active session should stay within the selected source / mode until the rider explicitly stops

#### State 7: Device-connected start
- when a device is connected, pressing `Start` sends the selected route to the device
- while sending, the start control shows loading state instead of silently changing modes
- after sync succeeds, the phone stays in route overview / companion mode instead of entering full phone riding mode
- stopping a device-backed route clears the active route session and returns the app to refreshed planning suggestions from the rider's current location

### Universal Route Detail Page
A single route detail page should be reused across:
- Google Maps handoffs
- GPX imports
- share-import route/destination recoveries that resolve into normal history entries
- recent route intents and destination history

Contents:
- route/destination title
- source badge such as `From Google Maps`, `GPX`, `Shared`, or `Recent`
- map preview snippet or full inline preview depending on platform conventions
- route summary and provider/source context
- lightweight actions such as `Open`, `Start`, `Open again`, or `Dismiss`

Rules:
- the page should keep a consistent structure regardless of source
- source-specific differences should show up as badges, metadata, and optional secondary rows rather than different page layouts
- the route detail page is the recovery surface when an import cannot jump directly into Home route preview cleanly
- the route detail page should not grow into a heavy planner or archival-management screen

### Inline Planning Controls
Planning mode should expose a small, easy-to-understand route-source control near the top search area instead of hiding source strategy entirely in Settings.

Expected behavior:
- default mode is `Mixed`
- switching the source mode reruns route suggestions for the current destination
- good first options are `Mixed`, `HSL`, `OSM`, and later `Google` when that path becomes real
- this control should appear only while a destination is selected or route suggestions are visible, not as permanent chrome in idle map mode
- Settings `Route Planner` still owns long-term defaults, but the current planning session must remain adjustable inline

### Settings Layout
Settings should be a full-screen page with simple top-level sections:
- `Routes`
- `Device`
- `Route Planner`
- `Import Diagnostics`

Behavior:
- each section opens its own focused page
- `Routes` should be the deeper history/recovery surface, not the default quick-entry surface
- `Routes` should stay lightweight and recent-oriented rather than turning into a permanent route archive
- `Device` should contain connection/setup/troubleshooting only, not dominate the main navigation experience
- `Route Planner` should centralize provider policy and route suggestion behavior preferences
- `Import Diagnostics` should retain unsupported share payloads with exportable debug info and easy retry/dismiss controls

### Share-Sheet Handling Rules
- if a shared item is clearly understood and can be turned into a route or destination intent immediately, the app should open directly into Home route preview
- if a shared item needs user choice, parsing recovery, or provider clarification, the app should open the universal route detail page instead
- every share/import should also become discoverable again through Settings `Routes`, so users are not forced to repeat a share if they leave the app
- Google Maps place/pin shares should usually resolve to direct Home route preview
- ambiguous route-like links or partially parsed files should resolve to the route detail page


## Share Import Direction (April 6, 2026)
For now, the companion app should focus on inbound share-sheet and link/file ingestion rather than activity recording or outbound fitness sync.

Product direction:
- the app should ingest route and destination intents from Google Maps shares, generic location links, GPX-first file import, and future shareable payloads
- imported share payloads should flow into the same route-intent pipeline as typed search, recents, and direct map destination selection
- recording rides and exporting completed activities are intentionally deferred from the near-term companion scope

UX implication:
- imported routes and destinations should appear in `Routes` with the same clear source-badge model as other incoming items
- the app should optimize for quick `Open` and `Start` flows, not account-heavy training or activity-management UX
- future outbound sync or ride-history features can be layered on later without changing the lightweight route-intake model

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
- OSM-based routing
- GPX import
- FIT import
- TCX import

Source strategy rules:
- all provider outputs must normalize into one source-agnostic `RoutePackage` contract
- provider-specific fields stay in provenance metadata, not in shared runtime behavior logic
- if source capability differs, normalize at companion layer, never in firmware adapter logic

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

## Implementation Checkpoint (April 3, 2026, Native GPX Companion Slice)
- Added a shared Rust `route-import-gpx` crate that parses GPX route/track XML and normalizes it into the canonical `RoutePackage` contract instead of letting raw GPX leak into runtime follow logic.
- Exposed the importer through `render-core-wasm` so emulator/web can import `.gpx` files through the same Rust normalization path, and kept emulator-side GPX import controls in both standard and fullscreen web UIs.
- Added native GPX import flows in both companion apps through the platform document pickers so imported `.gpx` files now enter the same preview, selection, and sync lifecycle as provider-backed routes.
- Preserved the route-package boundary: once imported, GPX routes are treated as canonical normalized packages by companion preview/send flows rather than a special-case runtime path.

## Implementation Checkpoint (March 31, 2026, Transport Fault-Hardening Slice)
- Hardened the firmware route-sync platform loop so malformed or corrupted inbound transfers emit explicit `retryable_failure` or `fatal_failure` sync statuses instead of aborting the frame loop.
- Added deterministic transport reliability coverage for resumed interrupted transfers, out-of-order chunk delivery, checksum mismatch handling, and malformed payload handling.
- Preserved the shared ownership rule that transport adapters report sync-state outcomes while runtime/render remain isolated from transport corruption details.

## Implementation Checkpoint (April 1, 2026, Security Coverage Expansion Slice)
- Expanded repository CVE coverage to include Android companion dependency tracking through Gradle dependency submission.
- Declared `emulator/web/package-lock.json` as the canonical JavaScript security lockfile so npm-based audit results stay authoritative even while `bun.lock` remains present for local tooling.
- Added a dependency-manifest guard so future third-party iOS or other unmanaged dependency ecosystems cannot land without CI security coverage in the same change.

## Implementation Checkpoint (April 3, 2026, Android Hardening + CVE Remediation Slice)
- Upgraded the Android build path to a committed Gradle Wrapper plus checksum-based dependency verification metadata, and centralized Android plugin/library versions through a version catalog so local development and CI resolve the same toolchain.
- Hardened the Android app manifest to secure-by-default backup and network settings while keeping the current single-module companion structure intact for easier onboarding and maintenance.
- Expanded Android CI from debug assembly only into wrapper validation plus lint, unit tests, and debug assembly, and moved workflow execution onto wrapper-driven builds instead of repo-external Gradle installs.
- Added first Android JVM tests around BLE packet codec behavior and persistence helpers so the companion module now has baseline regression coverage for non-UI logic.

## Implementation Checkpoint (March 31, 2026, Companion BLE Fault-Injection Slice)
- Added live companion-side BLE fault injection controls for retryable interruption, write failure, disconnect-after-chunk, and dropped inbound status paths in both iOS and Android app shells so transport recovery can be exercised without changing firmware behavior.
- Hardened the native BLE sync services so live disconnects and write failures remain resumable pending transfers instead of collapsing into idle state or incorrectly falling through to simulated success.
- Added acknowledgement timeout recovery that rewinds completed chunk uploads into a replayable failed transfer when the final device status never arrives, aligning companion behavior with the same deterministic retry posture already enforced in firmware.

## Implementation Checkpoint (March 31, 2026, Companion Provider Matrix Slice)
- Added sample-backed companion adapters for OSM, GPX import, FIT import, and TCX import in both native apps so every planned provider slot now produces a normalized `RoutePackage` through the same preview/send path, even where live integration is still pending.
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
- Added provider fixture contract coverage (HSL, OSM, GPX, FIT, TCX).
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
- Add provider fixture corpus for OSM and GPX/FIT/TCX imports

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

### Stage 6: Productionization, Observability, and Rollout Closure
- add telemetry and diagnostics for planning, sync, follow, and reroute lifecycle
- finalize rollout and support playbook

Exit criteria:
- operational dashboards and incident procedures are defined
- launch checklist is complete

## Quality and Validation Strategy
- Contract conformance tests across all providers.
- Sync reliability tests under packet loss and interrupted sessions.
- Runtime determinism tests for route progress and off-route stability.
- Rendering snapshot and readability tests across motion/orientation states.
- End-to-end tests for HSL, OSM fallback, Google Maps handoff import, GPX import, and live reroute loop.

## Explicit Program Assumptions
- Implementation effort is unconstrained; UX and performance are top priorities.
- Companion app is the orchestration authority for planning and rerouting.
- ESP does not execute full route planning in v1.
- Offline target is route follow and alerts after successful sync.
- Existing shared-Rust ownership boundaries are extended, not replaced.
