# Project Specification

## Product Definition
ESP32 bike minimap renderer in a video-game style UI.

## Main Runtime Behavior
- Two primary camera states:
  - Riding mode: heading-up orientation with user anchor in lower quarter of screen.
  - Stopped mode: centered user and north-up orientation after a delay.
- Pinch zoom supports close detail and broad context with bounded limits.
- Two-finger rotate temporarily offsets heading while moving.
- Temporary pan is allowed and smoothly recenters after pan idle timeout.
- During manual pan, camera follow is locked to pan-start rider position so the rider marker stays anchored; only camera offset moves until recenter completes.
- North indicator is shown at top-right and supports temporary mode override.

## Architecture Separation
- Main ESP32 project (`/work`): runtime camera/render/input behavior.
- Map conversion project (`/work/map-vector-cli`): city-scale source conversion and `.svm` format.
- Camera mode/state behavior is shared in Rust (`render-core`) and consumed by firmware and wasm emulator bindings.

## Data Flow
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data/city.svm`
- Current bridge for renderer integration: generated Rust map module.
- Target direction: direct `.svm` runtime loading in firmware.
- `cargo xtask prepare-map` uses a bike-oriented conversion profile that excludes ferry/boat/water transport segments.

## Current Phase Notes
- Shared camera transform supports heading-up, zoom, and pan.
- Camera rotation design is documented in `/work/docs/camera-rotation-design.md`.
- Emulator web shell uses React UI + MobX stores and keeps browser geolocation/touch logic in store layer, including manual bike-sim fallback controls for deterministic GPS movement.
- Firmware now includes a `GT9271` bus-polled touch path and firmware-side gesture recognition for pan, pinch, rotate, and tap.
- Firmware still needs board-level validation and optional `TP_INT` / `TP_RST` reset wiring after schematic confirmation.
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
  - Future overview mode design is documented in `/work/docs/overview-mode-design.md`.
- Riding mode:
  - Active movement rotates map so actual direction of travel points up.
  - Riding-mode heading should be derived from movement direction when motion data is available, with smooth rotation like driving navigation.
  - User marker is not centered; it stays near lower quarter lane for forward look-ahead.
- Stopped mode:
  - On stop, recenters user and after a short delay smoothly rotates map back to north-up.
  - Transition must be continuous for both position and rotation.
- Manual pan behavior:
  - While panning, follow target is frozen at pan start to avoid rider marker drift from live GPS updates.
  - After pan idle timeout, camera recenters smoothly and follow resumes current rider position.
- North indicator:
  - Show a small north indicator icon at top-right.
  - Tap toggles temporary north-up override when map is in heading-up.
  - If movement continues, auto-return to riding mode after timeout.
- Player marker visuals:
  - Riding mode marker should include a glowing yellow-green forward-facing shape.
  - Stopped mode marker should be larger and game-map readable.
- Vector visual style:
  - Shift toward dark/navy base, high-contrast bright major roads, subdued secondary roads.
  - Preserve circular minimap mask and strong ring border treatment.
