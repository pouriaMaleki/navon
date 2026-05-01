# Project Specification

Most important doc file is docs/ux-specs.md

## Product

ESP32 bike minimap with a round, game-like map presentation for riding. The product follows GPS position, renders vector map data, and keeps firmware and emulator behavior aligned through shared Rust runtime code.

## User-Facing Behavior

- The camera has two top-level motion modes:
  - `Riding`
  - `Stopped`
- Shared Rust exposes five orientation substates:
  - `Stopped North-Up`
  - `Heading Acquisition`
  - `Travel-Up Auto`
  - `North Preview`
  - `North Locked`
- `Travel-Up Auto` is the default moving presentation when heading confidence is good:
  - trusted travel direction rotates toward the top of the screen
  - rider anchor moves to the lower quarter for look-ahead
- `Heading Acquisition`, `North Preview`, `North Locked`, and `Stopped North-Up` keep the rider centered.
- Single tap on the north indicator enters temporary `North Preview`.
- Double tap on the north indicator enters `North Locked`.
- Tapping again unlocks north-up and returns to auto-follow when heading confidence is ready.
- Tapping the north indicator while already north-up should not change mode, but should give a brief acknowledgement pulse.
- Low-confidence movement must hold the last trusted camera angle instead of following noisy raw GPS course data.
- One-finger pan, two-finger pinch, and two-finger rotate are shared-Rust behaviors; rotate is only active in `Travel-Up Auto`.
- While riding, manual pan temporarily releases follow, keeps the rider marker map-anchored, and smoothly recenters after idle.
- While stopped, manual pan enters a sticky browse state:
  - no idle recenter timeout runs
  - the map stays where the user left it
  - tapping the north indicator recenters to the current rider location
- While moving, a shared-Rust speed overlay appears in the bottom quarter:
  - solid black panel
  - large white rounded speed digits
  - smaller white `kph` or `mph` unit label
  - tapping the panel toggles units
  - default unit is `kph`
  - the chosen unit should persist across restart in both firmware and emulator adapters
- The canonical orientation UX lives in [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md).

## Visual Palette

- Shared map and overlay rendering use this palette:
  - `#050B12`
  - `#051E24`
  - `#10132B`
  - `#103B48`
  - `#12A3A3`
  - `#D7FF3F`
- Background stays near-black.
- Map geometry uses the dark blue and teal range.
- Rider markers use `#D7FF3F`.
- Emulator and device should render from the same shared-Rust palette choices whenever possible.

## Architecture Boundaries

- `/work` owns runtime camera, motion, input, query, and render behavior.
- `/work/map-vector-cli` owns host-side map conversion and `.svm` format generation only.
- `runtime-core` owns stateful runtime policy:
  - motion estimation
  - gesture and tap interpretation
  - camera state transitions
  - follow-lock and recenter
  - map-query policy
- `render-core` owns stateless rendering:
  - projection
  - visibility and clipping
  - styling
  - overlay drawing
  - framebuffer generation
- `map-runtime` owns shared `.svm` parsing and coarse query lookup.
- `firmware` and `render-core-wasm` are adapters:
  - they translate platform I/O into shared contracts
  - they must not own product camera policy

## Map Presentation Direction

- The map system should evolve from a flat road-layer model into a zoom-aware presentation system with richer feature classes and declarative profiles.
- The preferred direction is one generated regional map package with multiple internal feature classes and LOD slices, rather than many unrelated zoom-specific files.
- Shared Rust runtime should choose the active presentation band from zoom and camera state.
- Converter-owned declarative profiles should decide which features exist for `bike`, `car`, `transit`, and future map modes.
- The current `bike` presentation should hide rail transit geometry and keep farther overview bands cleaner by preferring arterial-road and main-bike-route layers over dense street-level detail.
- The canonical design lives in [`/work/docs/map-presentation-system-design.md`](/work/docs/map-presentation-system-design.md).

## POI Layer Direction

- The current shared POI slice supports:
  - bicycle parking
  - bicycle repair / pump stations
  - supermarkets
  - food
- `Close Detail` should show all four categories with decluttered markers.
- `Ride Detail` should keep only bike utility POIs visible.
- `Network Overview` and `District Overview` should hide POIs.
- POI normalization belongs in the converter, not in runtime adapters.
- The canonical POI UX and ownership design lives in [`/work/docs/poi-layer-design.md`](/work/docs/poi-layer-design.md).

## Shared Runtime Contracts

- `RuntimeInputFrame`: ordered per-frame input envelope containing `dt`, optional GPS, and optional normalized touch contacts.
- `TouchContact` and `TouchContactFrame`: stable normalized touch input shared by firmware and wasm.
- `RuntimeFrameOutput`: runtime snapshot containing camera state, overlay state, `MapQuerySpec`, and optional diagnostics.
- `MapQuerySpec`: runtime-owned world/query intent used for coarse map lookup.
- `DiagnosticsSnapshot`: read-only debug surface for parity and runtime inspection.
- Shared public contracts must stay small, adapter-safe, and free of platform handles or ECS internals.

## Determinism And Validation

- The same ordered input frames must produce the same runtime behavior on firmware and wasm.
- Camera policy must prefer trusted movement-derived heading over raw GPS course whenever possible.
- `MapQuerySpec` must be a pure function of runtime state, viewport, and LOD policy.
- Final visibility and clipping stay in `render-core`, not in adapters or map query backends.
- Runtime and render logic must be covered by:
  - unit tests for math and policy
  - scenario tests for ride/stop/pan/recenter/compass flows
  - parity fixtures for firmware and wasm paths

## Security And Dependency Tracking

- Rust workspace dependencies must pass repository vulnerability scanning through the root workspace lockfile.
- Emulator web dependencies must pass repository vulnerability scanning through `emulator/web/package-lock.json`.
- Companion web dependencies must pass repository vulnerability scanning through `companion-web/package-lock.json`.
- `emulator/web/package-lock.json` and `companion-web/package-lock.json` are the canonical JavaScript lockfiles for repository security automation; any additional JS lockfiles must not become the sole source of dependency truth.
- Android companion dependencies must be published into GitHub dependency tracking so repository security advisories and dependency review can cover that subtree.
- Android companion builds must use the committed Gradle Wrapper and repository-owned dependency verification metadata instead of relying on ad-hoc machine-wide Gradle installs.
- iOS companion dependency managers must not be introduced without adding repository CVE coverage in CI during the same change.
- iOS companion local signing must come from an ignored local config file rather than from generated Xcode project state so `git pull` and `xcodegen generate` do not wipe the selected Apple team.

## Current Implementation Direction

- Shared Rust already owns:
  - riding/stopped camera policy
  - heading-confidence handling
  - pan/pinch/rotate
  - compass preview/lock behavior
  - stopped north-up settle
  - auto-recenter
- Emulator web forwards raw GPS and normalized touch contacts only.
- Firmware follows the same runtime/query/render pipeline, with real hardware integration still being completed behind the adapter boundary.
- Companion apps ship on three platforms:
  - `companion-ios/` — native SwiftUI, MapKit, CoreLocation, CoreBluetooth.
  - `companion-android/` — Jetpack Compose, Google Maps, FusedLocationProvider, Android BLE/GATT.
  - `companion-web/` — React + MobX + MapLibre on OSM tiles, browser Geolocation, no BLE (phone-guidance only).
    All three share the same product surface, the same `companion.*` persistence keys, and the same RoutePackage contracts; see [`companion-app-architecture.md`](./companion-app-architecture.md) for the detailed architecture.
- iOS phone-guidance Live Activities show route status plus a MapKit snapshot of the active route ahead. Snapshot version stamps are route-scoped so settings toggles can reuse a valid current snapshot, while route changes and route end clear stale map images.
- Production deployment co-hosts `companion-web` and `emulator/web` in a single nginx container (companion at `/`, emulator at `/emulator/`) so edge routing remains a single service.

## Supporting References

- Camera orientation UX: [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md)
- Map presentation system: [`/work/docs/map-presentation-system-design.md`](/work/docs/map-presentation-system-design.md)
- POI layer design: [`/work/docs/poi-layer-design.md`](/work/docs/poi-layer-design.md)
- Runtime architecture decision: [`/work/docs/runtime-ecs-architecture.md`](/work/docs/runtime-ecs-architecture.md)
- Framework execution guide: [`/work/docs/framework-execution-guide.md`](/work/docs/framework-execution-guide.md)
- Device touch integration: [`/work/docs/device-touch-integration-plan.md`](/work/docs/device-touch-integration-plan.md)
- Main plan: [`/work/docs/current-plan.md`](/work/docs/current-plan.md)
- Main TODO: [`/work/docs/todo.md`](/work/docs/todo.md)
