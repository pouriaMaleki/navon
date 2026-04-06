# Companion App Architecture

Spec reference: [`project-spec.md`](./project-spec.md)
Plan reference: [`current-plan.md`](./current-plan.md)

## Goal
Build both companion apps as native, map-first navigation products with the same logical architecture on iOS and Android while preserving platform conventions and reusing existing route, import, and sync seams.

## Product Shape
- Home is the primary surface.
- Home is a full-screen map with a top `Where to?` input and a secondary settings entry.
- Settings is a full-screen hub that contains `Connections`, `Routes`, `Device`, and `Route Planner`.
- Route detail is universal across imported, recent, and partner-originated route intents.
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
  - `Connections`
  - `Routes`
  - `Device`
  - `RoutePlanner`
  - `ShareImport`
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
  - BLE sync transport
  - persistence
  - diagnostics
  - partner integrations
  - native place search
- Integrations return domain models and results, never screen state.
- Integrations do not navigate and do not mutate feature state directly.

## Platform Defaults
### iOS
- SwiftUI
- `NavigationStack`
- MapKit for the Home map surface and long-press destination selection
- MapKit local search for destination suggestions
- native URL handling, share extensions, and document picker flows

### Android
- single-activity Jetpack Compose
- Navigation Compose
- Google Maps for the Home map surface
- platform-native place search for destination suggestions
- native share intents and SAF document picker flows

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
- One route detail page is used across Google Maps imports, GPX imports, partner routes, and recents.
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

## Required Tests
- feature-state tests for Home, Route Detail, Settings, and Device
- navigation smoke tests for Home -> Route Detail -> Settings flows
- import fast-path versus fallback tests
- provider normalization parity tests against shared fixtures
- BLE/session regression coverage surviving the shell refactor
