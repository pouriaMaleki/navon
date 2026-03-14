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
  - touch gesture integration,
  - follow-lock/recenter behavior,
  - map query + zoom-bucket LOD policy.
- `render-core`: stateless rendering primitives (camera view + visible line set -> framebuffer).
- `firmware`: platform adapter (GPS/touch drivers -> runtime input events, framebuffer presentation on device).
- `render-core-wasm`: wasm adapter (browser/emulator inputs -> runtime events, output pixels for canvas).

## Data Flow
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data/city.svm`
- Current bridge for renderer integration: generated Rust map module.
- Target direction: direct `.svm` runtime loading in firmware.
- `cargo xtask prepare-map` uses a bike-oriented conversion profile that excludes ferry/boat/water transport segments.

## Current Phase Notes
- Shared camera transform supports heading-up, zoom, and pan.
- ECS runtime architecture is documented in `/work/docs/runtime-ecs-architecture.md`.
- Device touch integration is documented in `/work/docs/device-touch-integration-plan.md`.
- Emulator web shell uses React UI + MobX stores and keeps browser geolocation/touch logic in store layer, including manual bike-sim fallback controls for deterministic GPS movement.
- Firmware uses a `GT9271` bus-polled touch path and firmware-side gesture recognition for pan, pinch, rotate, and tap.
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
- Runtime map query computes visible world bounds per frame from camera output.
- LOD policy is runtime-owned and maps zoom buckets to allowed layer classes.
- Map-source adapters apply both bbox + LOD filtering before calling renderer.
- Future overview mode must extend runtime LOD policy and map-source metadata, not platform adapters.
