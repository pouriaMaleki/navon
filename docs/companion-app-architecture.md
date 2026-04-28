# Companion App Architecture

Spec reference: [`project-spec.md`](./project-spec.md)
Plan reference: [`current-plan.md`](./current-plan.md)

## Goal
Build the companion apps as map-first navigation products with the same logical architecture across three platforms (iOS, Android, web) while preserving platform conventions and reusing existing route, import, and sync seams. The native apps ship the full product; the web companion ships the same planner surface minus device/BLE pieces and uses OSM tiles via MapLibre.

## Product Shape
- Home is the primary surface.
- Home is a full-screen map with a top `Where to?` input and a secondary settings entry.
- Settings is a full-screen hub that contains `Routes`, `Device`, `Route Planner`, and `Import Diagnostics`.
- Route detail is universal across imported and recent route intents, while unsupported share payloads fall back to `Import Diagnostics`.
- Home has three clear user modes:
  - planning mode: compare route options, adjust route-source strategy, and choose one route
  - phone guidance mode: follow one committed route on-phone with guidance-focused UI
  - device overview mode: monitor a committed route after sending it to a connected device

## Technical Layers
### App composition
- One composition root wires dependencies, app navigation, and app-wide session state.
- App-wide state is limited to coarse concerns:
  - current route intent
  - active guidance session
  - imports queue and route history
  - user settings
  - device sync summary
- App-wide state must not become the implementation surface for every feature.

### Feature layer
- Features are:
  - `Home`
  - `RouteDetail`
  - `Settings`
  - `Routes`
  - `Device`
  - `RoutePlanner`
  - `ShareImport`
  - `ImportDiagnostics`
- Each feature owns only its own screen state and feature actions.
- Features read repositories and use cases through interfaces; views never talk to integrations directly.

### Domain layer
- Domain logic remains native in Swift and Kotlin.
- Shared conceptual contracts must stay aligned across platforms for:
  - destination search result
  - route intent
  - route preview state
  - active guidance session
  - route history item
  - settings and planner preferences
- `RoutePackage` remains the canonical route payload and sync format.

### Data / integration layer
- Integrations own:
  - provider access and normalization
  - GPX import
  - BLE sync transport (native only; web skips it)
  - persistence
  - diagnostics
  - share import ingestion and classification, shared with in-app URL paste through a `UrlDestinationResolver` seam
  - native place search (MapKit on iOS, Android Places on Android, Photon + Nominatim on web)
  - device GPS via a `LocationService` abstraction that each platform implements with its native APIs (`CoreLocation`, `FusedLocationProvider`, browser Geolocation)
- Integrations return domain models and results, never screen state.
- Integrations do not navigate and do not mutate feature state directly.

## Platform Defaults
### iOS
- SwiftUI
- `NavigationStack`
- MapKit for the Home map surface and long-press destination selection
- MapKit local search for destination suggestions
- native URL handling, share extensions, and document picker flows
- AVFoundation `AVCaptureMetadataOutput` for pairing-QR capture, gated behind `NSCameraUsageDescription`
- Single bonded peripheral persisted as `PairedPeripheralRecord` (CompanionPersistence). Fast-path reconnect via `CBCentralManager.retrievePeripherals(withIdentifiers:)`; falls back to a service-UUID scan when the iOS bond store is empty (fresh install). Pairing flow writes the QR-displayed 32-byte secret to the `…1004` pairing-confirm characteristic to gate the device's transition to operational mode.

### Android
- single-activity Jetpack Compose
- Navigation Compose
- Google Maps for the Home map surface
- platform-native place search for destination suggestions
- native share intents and SAF document picker flows

### Web
- Vite + React 19 with `mobx-react-lite` observers and a single `RootStore` composed of small sub-stores (Settings, Planning, Guidance, History, MapCamera, Diagnostics, Location)
- MapLibre GL JS with an OpenStreetMap raster style for the Home map surface
- browser Geolocation for rider location, with persisted last-known fallback
- Photon (typeahead) + Nominatim (reverse geocode) for destination search
- drag-and-drop + paste handling for GPX files and shared URLs; no native share-extension equivalent
- localStorage for persistence under the same `companion.*` key prefix as the native apps

## Reuse Rules
Reuse these seams unless there is a strong architecture reason not to:
- provider interfaces and current adapters
- BLE packet contracts and transport services
- persistence stores for settings and session history
- diagnostics stores
- normalized route/domain payloads shaped around `RoutePackage`

Refactor these aggressively:
- tabbed shells
- monolithic app state objects used as feature implementation surfaces
- direct view-to-service coupling
- launch/plan/preview/ride primary navigation fragmentation

## Feature Rules
### Home
- Home owns destination search, destination selection, route suggestion preview, route-source mode selection for the current planning session, and active guidance transitions.
- Home does not own provider/network details or BLE logic.
- Home uses a `PlaceSearchService` and a planner use case; it does not call provider adapters directly from the view.
- Home planning mode should default to `Mixed` route sourcing and expose that choice inline near the top search field, while long-term defaults still live in Settings `Route Planner`.
- Home guidance mode begins only after `Start` is pressed; `Start` is the explicit boundary between comparing routes and following one route.
- If no device is connected, `Start` enters phone guidance mode.
- If a device is connected, `Start` becomes device-first, shows sync progress, and then leaves Home in device overview mode rather than duplicating on-phone guidance.

### Route Detail
- One route detail page is used across Google Maps imports, GPX imports, and recents.
- Source-specific metadata appears as badges and secondary rows, not new screen types.

### Settings
- Settings is the structured hub for secondary features.
- Settings `Routes` is a lightweight recovery and re-entry surface, not a route library.
- Settings `Device` contains pairing, sync, and troubleshooting only.

## Implementation Guardrails
- No integration code inside views.
- No feature should parse files, perform network calls, or speak BLE directly.
- No source-specific route detail screens unless the universal route detail page cannot express the feature.
- No reintroduction of bottom-tab primary navigation.
- No uncontrolled growth of a single `AppModel` / `CompanionAppState` style object.
- Shared route contracts must remain platform-parity tested whenever native domain models duplicate them.

## UX Rules
- route options should be described in rider-meaningful terms first, such as `Fastest`, `Quieter`, or `Simpler`, with provider/source shown as secondary metadata
- route-source strategy should be adjustable inline in planning mode rather than hidden only in Settings
- `Start` commits to one route and transitions into the correct active-navigation mode
- `Stop` exits phone guidance or device overview mode and returns the user to refreshed planning suggestions from the current location
- The selected route and selected source remain locked for the active session; reroutes stay within that source until the rider stops
- planning-mode UI and guidance-mode UI should feel distinct; route comparison chrome should not remain on screen after guidance begins
- HSL routing is only offered when it is actually usable: both the Digitransit subscription key is configured AND both trip endpoints fall inside the Uusimaa region. Otherwise the source picker collapses to OSM and the Mixed / HSL tabs are hidden. Route Planner settings explains what HSL is and links to the Digitransit portal for key registration.
- OSM cycling routing is a multi-source orchestrator (`OsmCyclingRoutingAdapter`) that fans out in parallel to BRouter `fastbike` (paths-preferred), BRouter `trekking` (balanced), and OSRM `bike` (direct) and exposes whichever succeed as up to 3 `RouteAlternative` entries — each with a different cycle-infrastructure trade-off. Showing all three lets the rider compare lines on the map and pick the one that looks right for their trip; the default selection is fastbike (paths-preferred) so the bug "OSRM picks driving streets even when bike paths exist" is fixed by default. If a backend fails its slot drops out (planningNotice flags it as `"N source(s) unavailable"`); if all three fail we fall back to the sample preview. BRouter requires `timode=2` to populate `voicehints`. Adapter file: [`companion-web/src/integrations/osm/OsmCyclingRoutingAdapter.ts`](../companion-web/src/integrations/osm/OsmCyclingRoutingAdapter.ts) with the BRouter parsing in [`brouter/`](../companion-web/src/integrations/osm/brouter/). The existing `presentAlternatives` post-processing relabels by position to rider-meaningful "Fastest" / "Quieter" / "Simpler"; the orchestrator's emit order (fastbike → trekking → OSRM) intentionally maps fastbike to "Fastest" so the path-aware option is the default-selected one.
- "Where to?" accepts http(s) URLs directly: pasted Google Maps / OSM links are followed to a destination (inline coords first; then redirect-following through the share-import classifier), with explicit loading and error rows in the search panel.
- Rider position always comes from real device GPS through the `LocationService` seam. The locate/recenter control shows a spinner until the first fix arrives, then swaps to the normal control. When permission is denied or unavailable, planning falls back to the last persisted fix then to a static default so the planner still works.
- Typeahead search debounces (250 ms) and passes the rider's current location to `PlaceSearchService.searchDestinations` as a bias so same-city results rank first (`docs/ux-specs.md` line 75). The store exposes `isTypeaheadSearching` (web) so the UI can render a spinner during the in-flight request.
- Recents pagination is gated on reaching the last visible item. `loadMoreRecentsIfNeeded(lastId)` grows the visible slice only when `lastId` matches the current end of the list.
- Compass-tap on companion apps also recenters the camera (`docs/ux-specs.md` line 39). The guidance store fires a recenter signal (`onRecenterRequested` callback on web, `mapRecenterRequestID` publisher on iOS, `mapRecenterRequestTick` state on Android) that the map surface observes.
- Camera anchors the rider in the bottom quarter during routing and at screen center when stationary (`docs/ux-specs.md` lines 40, 84). On web this is driven by `MapCameraStore.riderAnchorNormalizedY` (0.5 stationary, 0.72 routing) that MapSurface applies as MapLibre viewport padding; on iOS/Android the equivalent lives in the SwiftUI / Compose layout.
- "Rider is moving" is a single shared signal: the heading-trail (`HeadingTrail`) returns a defined `travelHeadingDegrees` once the rider has displaced ≥ 3 m within the recent fix window. Spec lines 108-118 ("when moving with or without a route") apply whenever this signal is true: the bottom-quarter anchor engages and the camera rotates to the GPS-derived direction even in planning mode without an active route. Web: `LocationStore.travelHeadingDegrees` is observable, `RootStore` autorun ORs it with `homeMode==phoneGuidance`, `refreshCamera.ts` consumes it for the planning-mode camera. iOS: `HomeViewModel.travelHeadingDegrees`, `notifyRiderLocationUpdated` bumps the follow tick on routing OR moving, `resetPlanningCamera` switches to riding-mode framing when the trail is non-nil. Android mirrors with a Compose-observable mirror field.
- Search-panel post-selection latch: after `selectSuggestion` flips `isSearchOpen` to false, a ~350 ms window absorbs follow-up `openSearch()` calls. This defends against the focused input replaying its `onFocus` handler on the next render pass. Both platforms (web `PlanningStore`, iOS `HomeViewModel`) implement the same window.
- Routing progress: `progressDistanceM` advances monotonically as the rider's projected position moves along the route. `nextInstructionLine` skips depart/arrive maneuvers and picks the first one ahead of progress, displaying remaining distance (not distance-from-start). Web has had this since the first guidance ship; iOS now has it as well via the same `projectProgress(onto:rider:)` projection helper.

## Required Tests
- feature-state tests for Home, Route Detail, Settings, and Device
- navigation smoke tests for Home -> Route Detail -> Settings flows
- import fast-path versus fallback tests
- provider normalization parity tests against shared fixtures
- BLE/session regression coverage surviving the shell refactor
