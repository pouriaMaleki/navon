# Project Specification

## Product Definition
ESP32 bike minimap renderer in a video-game style UI. Similar devices: Garmin Bike Computer

## Main Runtime Behavior
- Map is rendered as vectors, on the visible and non visible area. Map is rendered according to accurate GPS data.
- Two primary camera states:
  - Riding mode: heading-up orientation with user anchor in lower quarter of screen.
  - Stopped mode: centered user and north-up orientation after a delay.
- Pinch zoom supports close detail and broad context with bounded limits.
- Two-finger rotate temporarily offsets heading while moving.
- Temporary pan is allowed and smoothly recenters after pan idle timeout.
- During manual pan, camera follow is locked to pan-start rider position so the rider location marker stays anchored; only camera offset moves until recenter completes.
- North indicator is shown at top-right and supports temporary mode override.

## Architecture Separation
- Main ESP32 project (`/work`): runtime camera/render/input behavior.
- Small nicely separated tree like modular code design. Built like lego. Maintains separation of concerns deep in the code layers.
- Map conversion project (`/work/map-vector-cli`): city-scale source conversion and `.svm` format.

## Runtime Architecture Decision
- Chosen design: ECS runtime with fixed deterministic frame schedule and event-driven input ingestion.
- Framework: `bevy_ecs` (pinned `0.17.2`) in new `runtime-core` crate.
- Rust toolchain constraint: `bevy_ecs 0.18.x` currently requires Rust `1.89+`; project is pinned to Rust `1.88`, so `0.17.2` is the compatible baseline.

## Runtime Boundaries
- `runtime-core`: owns runtime behavior and policy:
  - camera mode transitions,
  - shared touch/contact interpretation,
  - follow-lock/recenter behavior,
  - map query + zoom-bucket LOD policy.
- `render-core`: stateless rendering primitives (`CameraView` + queried world geometry -> framebuffer).
- `MapSource` implementations: coarse world-space geometry lookup from `MapQuerySpec`; the current embedded `.svm` bridge in `render-core-wasm` and future shared direct readers both fit here.
- `firmware`: platform adapter (GPS/touch drivers -> `RuntimeInputFrame` with normalized shared input samples, framebuffer presentation on device).
- `render-core-wasm`: wasm adapter (browser/emulator inputs -> `RuntimeInputFrame` with normalized shared input samples, output pixels for canvas).

## Framework Foundation Requirements
- First implementation step is not product polish; it is a durable shared Rust framework that lets firmware and wasm use the same runtime behavior.
- The framework must be explicit about ownership:
  - `runtime-core` owns stateful runtime policy and deterministic frame progression.
  - `runtime-core` input modules own gesture recognition, tap recognition, and product-control hit testing from shared normalized contact samples.
  - `render-core` owns pure render math, visibility filtering, styling, and framebuffer generation.
  - adapters own only platform I/O translation, sample normalization into shared input contracts, and scheduling hooks.
  - `MapSource` implementations own data access only; they do not own camera policy, LOD policy, or styling.
- Public contract types must stay small and stable:
  - `RuntimeInputFrame`
  - `TouchContact`
  - `TouchContactFrame`
  - `RuntimeFrameOutput`
  - `RuntimeConfig`
  - `MapQuerySpec`
  - `DiagnosticsSnapshot`
  - map-source/query traits
- `bevy_ecs` must remain an internal implementation detail of `runtime-core`; adapters must not manipulate ECS state directly.
- All product behaviors must be configurable through Rust config/resources, not hardcoded in firmware or emulator glue.

## Core Runtime Contracts
- `TouchContact`: normalized contact sample containing stable contact identity plus shared coordinate data. It must not contain browser or device handles.
- `TouchContactFrame`: ordered touch snapshot for one frame. Adapters emit this shared contact form rather than product-level `Pan`, `Pinch`, `Rotate`, or `Tap` semantics.
- `RuntimeInputFrame`: ordered per-frame input envelope containing `dt`, optional GPS samples, optional `TouchContactFrame`, and other normalized interaction data. It must not contain browser or device handles, and adapters must not classify product gestures or control hits.
- `RuntimeFrameOutput`: stable runtime snapshot containing camera view/state, `MapQuerySpec`, overlay/style inputs, and optional `DiagnosticsSnapshot`. It must not contain queried geometry buffers or adapter-owned state.
- `MapQuerySpec`: coarse query intent produced by the runtime, including world bounds, zoom bucket, LOD mask, and any flags needed by the current embedded `.svm` bridge or a future direct `.svm` reader.
- `DiagnosticsSnapshot`: read-only parity/debug surface exported from `runtime-core::api`; internal counters and history remain in `runtime-core::diagnostics`.
- `MapSource::query(&MapQuerySpec)`: coarse world-space candidate lookup only. Implementations may live beside the current embedded `.svm` bridge or a later shared map reader, but they do not own camera policy, styling, or screen-space clipping.

## Required Crate and Module Shape
- `runtime-core` should be introduced as a library crate with the following internal module split:
  - `api`: public input/output/query/config/diagnostic types exposed to adapters and `MapSource` implementations.
  - `schedule`: world creation, system set registration, deterministic frame stepping.
  - `input`: shared touch/contact validation, gesture and tap derivation, and per-frame input staging.
  - `motion`: GPS sample validation, speed estimation, travel-heading smoothing.
  - `camera`: riding/stopped state machine, pan/pinch/rotate integration, follow-lock, recentering.
  - `map`: world-bounds projection, visible query bounds, zoom bucket and LOD mask selection.
  - `output`: adapter-ready frame snapshot assembly.
  - `diagnostics`: optional debug counters/state snapshots plus `DiagnosticsSnapshot` assembly hooks that can be compiled into emulator and firmware builds.
- `render-core` should remain stateless and pure, with modules grouped by concern:
  - `math`
  - `camera_view`
  - `visibility`
  - `style`
  - `raster`
  - `overlay`
- The initial framework may add a small shared map abstraction crate later if direct `.svm` loading and embedded/shared map readers need the same reader/types, but this must remain independent from platform adapters.

## Extensibility Rules
- New behaviors must enter the runtime through one of these extension points:
  - new input event type,
  - new resource/config field,
  - new ECS system in an existing schedule set,
  - new map-layer metadata or LOD rule,
  - new render overlay primitive.
- Future features such as routes, POIs, recording, sensors, and alternate map themes must compose without rewriting the main frame loop.
- Runtime features must be additive and capability-driven:
  - missing GPS should not break manual pan/zoom,
  - missing touch should not break GPS follow,
  - missing motion confidence should degrade to stable last-known heading behavior.
- Avoid monolithic "bike computer state" structs with mixed concerns. Persistent state should be separated by domain so tests can isolate motion, camera, and query behavior independently.

## Deterministic Runtime Rules
- Runtime stepping must use a fixed schedule order every frame even if some inputs are absent.
- Input ingestion must be edge-safe:
  - contact-count changes reset shared gesture recognizer state,
  - stale GPS samples are ignored,
  - invalid timestamps cannot poison heading smoothing.
- The same ordered `TouchContactFrame` sequence must resolve to the same pan/pinch/rotate/tap semantics on firmware and wasm.
- Camera policy must consume motion confidence rather than raw heading whenever possible.
- `MapQuerySpec` must be a pure function of runtime camera state, viewport, and LOD policy.
- Final screen-space visibility/clipping must remain a pure `render-core` operation for the same `CameraView` and queried geometry candidates.
- Output assembly must produce the same result for the same ordered input frames on firmware and wasm.

## Verification Strategy
- Pure math and geometry live behind unit tests in `render-core` and `runtime-core`.
- Runtime behavior must have scenario tests that feed ordered `RuntimeInputFrame` sequences and assert:
  - shared contact interpretation parity,
  - riding/stopped transitions,
  - heading smoothing,
  - pan/follow-lock behavior,
  - north-up override timing,
  - zoom bucket and LOD selection.
- Shared regression fixtures should cover the same input sequence on both firmware-facing and wasm-facing adapters to catch parity drift.
- The framework is not complete until the emulator and device can both consume the same runtime outputs without product-specific forks.

## Framework Delivery Order
- Phase 1: establish crate skeletons and public contracts.
- Phase 2: move touch/contact interpretation, motion, and camera policy into `runtime-core`.
- Phase 3: route map query and render requests through stable runtime outputs.
- Phase 4: attach firmware and wasm adapters to the shared runtime.
- Phase 5: add tests, diagnostics, and direct `.svm` loading groundwork.

## Supporting Design Docs
- Execution and validation workflow: `/work/docs/framework-execution-guide.md`
- Recommended crate and module layout: `/work/docs/source-tree.md`
- ECS runtime decision summary: `/work/docs/runtime-ecs-architecture.md`
- Device touch integration details: `/work/docs/device-touch-integration-plan.md`

## Data Flow
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data/city.svm`
- Current bridge for renderer integration: embedded `.svm` bytes with a coarse query index in `render-core-wasm`.
- Target direction: direct `.svm` runtime loading in firmware.
- `cargo xtask prepare-map` uses a bike-oriented conversion profile that excludes ferry/boat/water transport segments.

## Current Phase Notes
- `runtime-core::api` now exposes stable config, input, output, query, and diagnostics contract modules for adapters.
- `runtime-core` now includes an internal `bevy_ecs` deterministic frame schedule that projects GPS into world-space focus, derives shared gesture/tap semantics, and emits interaction-aware camera snapshots plus `MapQuerySpec`.
- Firmware bridge helpers now build shared `RuntimeInputFrame` values, but firmware is not yet wired into a product-complete loop.
- `render-core` and `render-core-wasm` now provide the emulator-facing end-to-end step/query/render slice from shared Rust.
- Shared camera foundation now supports one-finger pan, two-finger pinch/rotate, follow-lock, auto-recenter, riding/stopped transitions, stopped north-up settle, rotated query coverage, bounded zoom configuration, and short GPS-dropout resilience.
- ECS runtime architecture is documented in `/work/docs/runtime-ecs-architecture.md`.
- Device touch integration is documented in `/work/docs/device-touch-integration-plan.md`.
- Emulator web shell uses React UI + MobX stores, captures browser geolocation/raw touch contacts, and forwards them into shared Rust through a frame-driven wasm bridge.
- Firmware uses a `GT9271` bus-polled touch path; the target boundary is normalized contact frames from firmware with gesture and tap interpretation owned by `runtime-core`.
- Firmware needs board-level validation and optional `TP_INT` / `TP_RST` reset wiring in `main.rs` after schematic confirmation (`TouchInput::new_with_reset` hook is available).
- Real-device touch target is Waveshare `ESP32-P4-WIFI6-Touch-LCD-3.4C` with `GT9271` capacitive controller on the board touch/display assembly.
- Emulator developer tooling requires `wasm-pack` in host/devcontainer PATH.
- Emulator web toolchain requires Node.js `>=20.19` (devcontainer pins Node 22).
- Converter behavior is profile-driven for transport filtering; bike profile keeps roads/cycle paths and drops water transport lanes.

## Security and CVE Management
- The repository must track dependency vulnerabilities across Rust workspace crates and emulator web dependencies.
- CVE detection must run on pull requests and on a scheduled cadence to catch newly disclosed issues.
- The project should use GitHub-native alerting so maintainers can subscribe and get notified when new vulnerabilities are detected.
- Dependency update automation should create reviewable pull requests for security patches where possible.
- Vulnerability triage should be severity-aware, with faster turnaround for critical/high findings.

## Phase 4 Behavior Targets
- Zoom in:
  - Maximum pinch-in target is local context only (about 100 m around user visible area).
  - Keep user readable and roads separable at max zoom.
- Zoom out:
  - Allow broad coverage but clamp before dense vector overdraw makes roads unreadable.
- Riding mode:
  - Active movement rotates map so actual direction of travel points up.
  - Riding-mode heading should be derived from movement direction when motion data is available, with smooth rotation like driving navigation.
  - User marker is not centered; it stays near lower quarter lane for forward look-ahead.
- Stopped mode:
  - On stop, recenters user and after a short delay smoothly rotates map back to north-up.
  - Transition must be continuous for both position and rotation.
- Manual pan behavior:
  - By panning user can see other parts of the map.
  - While panning, user location, which is beased on GPS point remains in actual GPS location of the rendered map. Only camera moves.
  - After pan idle timeout, camera recenters smoothly and follow resumes current rider position.
- North indicator:
  - Show a small north indicator icon at top of the screen.
  - Tap toggles temporary north-up override when map is in heading-up.
  - If movement continues, auto-return to riding mode after timeout.
- Player marker visuals:
  - Riding mode marker should include a glowing yellow-green forward-facing shape.
  - Stopped mode marker should be larger and game-map readable.
- Vector visual style:
  - Dark/navy base with high-contrast major roads and subdued secondary roads.
  - Preserve circular minimap mask and strong ring border treatment.
- Campera moving and Travel Heading Estimation
  - Only trust motion deltas once movement exceeds minimum threshold.
  - When movement is too small/noisy:
    - keep previous stable travel heading
    - do not snap to a new value
  - While moving:
    - blend filtered travel vector toward latest motion vector
    - derive heading from filtered vector
  - Other specs
    - Riding east should rotate the map so east is up and north indicator points left.
    - Riding west should rotate the map so west is up and north indicator points right.
    - Quick left/right wiggles should not cause fast camera twitching.
    - Stopping should preserve current heading briefly, then smoothly return to north-up.

## Zoom-Level Detail Extensibility
- Runtime map query computes `MapQuerySpec` per frame from camera output.
- LOD policy is runtime-owned and maps zoom buckets to allowed layer classes.
- `MapSource` implementations use `MapQuerySpec` for coarse bbox + LOD candidate selection before handing geometry to `render-core`.
- `render-core` performs final screen-space visibility/clipping and rasterization for the queried geometry.
- Future overview mode must extend runtime LOD policy and map-source metadata, not platform adapters.
