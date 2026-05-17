# Framework Architecture

## Architecture Principles
The project is a framework-first Rust buildout: shared runtime behavior is implemented once in `runtime-core` and consumed by both the firmware and wasm adapters. The adapters are thin I/O shells — they own hardware/browser input acquisition and framebuffer presentation, nothing more.

1. **Framework-first.** Behavior lives in shared Rust. Adapters translate platform I/O into normalized inputs and route outputs to platform-specific presentation.
2. **Small public contracts.** Prefer narrow, stable types over broad convenience APIs. Every public type must justify its existence across crate boundaries.
3. **Stateless rendering.** `render-core` takes a `RenderScene` and draws it. It owns no mutable world state and makes no policy decisions.
4. **Deterministic runtime.** `RuntimeCore::step(input) -> output` is pure: the same ordered input frames produce the same output. Timing, gesture classification, and camera policy all resolve inside the step.
5. **Validate continuously.** Structure and behavior are guarded by contract tests, scenario tests, and parity checks between targets.

## Crate Ownership Map

### `runtime-core` (`/work/device/core/runtime-core`)
Owns all product behavior and mutable state:
- touch contact interpretation and gesture recognition (pan, pinch, rotate, tap, recenter, compass)
- motion fusion from GPS samples
- camera policy and mode transitions
- route state and navigation progress
- diagnostics aggregation
- producing `RuntimeFrameOutput` including `MapQuerySpec`

### `render-core` (`/work/device/core/render-core`)
Owns pure, stateless rendering:
- world-to-screen projection (`CameraView`)
- geometry clipping and visibility
- style resolution from feature class and presentation band
- raster primitives (lines, points, framebuffer)
- overlay and UI rendering

Takes a `RenderScene` (config + `RuntimeFrameOutput` + `MapQueryResult`) and a `Framebuffer`. Owns no decisions about what to show — only how to draw it.

### `map-runtime` (`/work/device/core/map-runtime`)
Owns coarse map data lookup:
- loading `.svm` packages
- answering `MapQuerySpec` queries (bbox + LOD mask + feature class filter → `MapQueryResult`)
- exposing map metadata (bounds, center)

Implements the `MapSource` trait. Does not own visibility, styling, or camera logic.

### `map-vector-cli` (`/work/tools/map-vector-cli`)
Owns offline map preparation:
- source ingestion and feature classification
- simplification and generalization
- declarative profile parsing
- emitting `.svm` packages

### `render-core-wasm` (`/work/device/core/render-core-wasm`)
Wasm-specific rendering backend. Consumes the same `RenderScene` contract as the firmware render path.

### Adapters (`/work/device/firmware`, `/work/companion-apps/web`)
Thin I/O shells:
- acquire GPS, touch, BLE, and route-sync events from platform hardware or browser APIs
- normalize them into `RuntimeInputFrame`
- feed the frame into `RuntimeCore::step()`
- hand the resulting `RuntimeFrameOutput` + `MapQueryResult` to `render_frame()`

Adapters do not own camera policy, gesture recognition, LOD logic, or product-control hit testing.

## Public Contract Surface

These types form the API boundary between crates. They are stable and adapter-safe.

**Input side** (`runtime-core::api`):
- `RuntimeInputFrame` — a single frame's worth of normalized input (dt, GPS, touch contacts, route sync, viewport)
- `TouchContact` / `TouchContactFrame` — normalized touch events with phase tracking
- `GpsSample` — position, speed, bearing, accuracy
- `ViewportSize` — framebuffer dimensions in device pixels

**Output side** (`runtime-core::api`):
- `RuntimeFrameOutput` — camera state, overlay state, and `MapQuerySpec` (does **not** carry geometry buffers)
- `CameraStateSnapshot` — world center, zoom, rotation, tilt, camera mode
- `OverlayState` — speed, distance, navigation cues, alert state
- `MapQuerySpec` — bbox, LOD mask, and feature class filter for the current frame

**Configuration** (`runtime-core::api`):
- `RuntimeConfig` — speed unit, route alert verbosity, zoom bounds
- `ZoomBounds` — min/max zoom clamp

**Map source** (`runtime-core::api`):
- `MapQueryResult` — polyline and point candidates returned by a `MapSource`
- `MapLayer` / `MapPresentationBand` / `LodMask` — feature class and LOD enums
- `WorldBounds` / `WorldPoint` — spatial primitives

**Diagnostics** (`runtime-core::api`):
- `DiagnosticsSnapshot` — frame index, frame timing, GPS fix status, touch state

## Invariants

1. **Single source of truth for behavior-critical semantics.** If multiple representations of the same concept exist, document which one is authoritative. Do not let timing come from one representation and classification from another without an explicit canonicalization step.

2. **Derived state over duplicated state.** When direction, classification, or progress can be derived from canonical runtime state or geometry, derive it. Do not hand-maintain duplicate labels.

3. **State machines must handle re-entry.** Any `pending`, `applied`, `active`, or `locked` flag needs explicit reset conditions and tested re-entry behavior. A state machine that only works on first activation is incomplete.

4. **Demo and test fixtures must satisfy production invariants.** Demo routes, emulator helpers, and fixtures must satisfy the same invariants as shared-core inputs. Fixture builders should fail fast when assumptions drift.

5. **Ordered-geometry progression must be explicit.** When rejoining, snapping, or projecting onto ordered geometry, choose whether behavior prefers nearest, previous, or next progress. Add edge tests near boundaries.

6. **Every bugfix includes a regression test.** Any change to runtime logic must add or extend at least one regression test for the failure mode.

## Extension Points

New functionality enters through one of these paths:
- a new field on `RuntimeInputFrame` (new input event type)
- a new field on `RuntimeConfig` (new configuration)
- a new bevy ECS resource or system within the frame schedule
- a new `MapLayer` variant or LOD rule
- a new overlay primitive in `OverlayState`
- a new field on `DiagnosticsSnapshot`

Prefer expanding an existing extension point over bypassing the design with a new cross-cutting mechanism.

## PR Review Checklist

Every implementation PR should be checked against these questions:

1. Does this logic live in the correct crate?
2. Does it expand an existing extension point instead of bypassing the design?
3. Is any adapter starting to own product behavior?
4. Is a new public type actually required?
5. Can this logic be tested without browser or device APIs?
6. Would wasm and firmware still behave the same after this change?
7. Does `RuntimeFrameOutput` expose query intent instead of geometry buffers?

## Regression Test Patterns

Use the smallest test that can lock the invariant:
- **Contract tests:** schema, normalization, and canonical fixture validity.
- **State-machine regression tests:** repeated activation, reset, and re-entry behavior.
- **Fixture-validity tests:** fail fast when demo or bridge data drifts from shared-core expectations.
- **End-to-end scenario tests:** cross-subsystem behaviors (reroute loops, alert transitions, bridge orchestration).

Keep tests colocated with the subsystem they protect. Prefer deterministic scenario inputs over manual visual verification when behavior depends on ordering, timing, or progression thresholds.

## Anti-Patterns

- Adapter-local camera state machines
- Adapter-local gesture recognition or control hit testing
- Product policy in TypeScript or board code
- Monolithic shared state structs mixing motion, camera, input, and rendering
- Public APIs that expose ECS internals
- Features that require edits across all crates to land one behavior

## Companion App Guardrails

When editing `companion-apps/ios` or `companion-apps/android`:

1. Keep Home as the single primary surface; do not reintroduce tabbed primary navigation.
2. Keep feature state in feature-scoped view models or state holders, not in one monolithic app object.
3. Keep views free of provider, import, BLE, and persistence logic.
4. Keep `RoutePackage` canonical; native UI models may wrap it but must not fork route semantics.
5. Keep Settings `Routes` lightweight and recent-oriented; do not turn it into a heavy route library.
6. Keep one universal route detail page for imports and recents unless a source genuinely requires a different recovery surface.
7. Keep share/import handling on the fast path to Home route preview when parsing is clear, and use Route Detail as the fallback.
