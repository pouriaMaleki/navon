# ESP32 Bike Minimap Platform

Rust-based minimap platform for ESP32, aimed at a bike-mounted game-style map experience.

## Product Split
- Main ESP32 project (`/work`): runtime minimap behavior (GPS follow, heading-up, zoom/pan).
- Map conversion project (`/work/map-vector-cli`): host-side city map conversion into `.svm`.

## Runtime Architecture
- `runtime-core`: shared runtime foundation with stable adapter-facing contracts and an internal `bevy_ecs` deterministic frame schedule for motion, camera, and map-query output.
- `render-core`: stateless render primitives (`CameraView` + queried world geometry -> framebuffer).
- `map-runtime`: shared embedded `.svm` reader and coarse bbox/LOD query backend used by adapters.
- `MapSource` implementations: coarse bbox/LOD data lookup behind `MapQuerySpec`.
- `firmware` and `render-core-wasm`: platform adapters that translate device/browser I/O into shared normalized input contracts and output surfaces only.

## Current Slice Status
- `runtime-core::api` defines stable shared contracts for config, GPS/touch input, camera/query output, diagnostics, and map-query handoff.
- `runtime-core` now steps a deterministic ECS runner that derives gestures/taps, filtered heading, interaction-aware riding/stopped camera state, and `MapQuerySpec` from shared inputs.
- The current foundation fixes rotated query coverage for heading-up cameras, keeps the configured lower zoom range reachable, and preserves motion state across brief GPS gaps.
- Shared Rust now owns one-finger pan, two-finger pinch/rotate, follow-lock, auto-recenter, north-indicator tap handling, and stopped north-up settle behavior.
- Firmware and wasm bridge helpers can construct `RuntimeInputFrame` values from raw adapter samples.
- `render-core` now performs shared camera projection, clipping, overlay drawing, and deterministic grayscale framebuffer generation.
- `map-runtime` now owns the shared embedded `/work/map-data/city.svm` reader plus coarse spatial query index used by both firmware and wasm.
- `render-core-wasm` now steps `runtime-core`, queries shared map geometry through `map-runtime`, renders pixels, and exposes a frame-driven wasm API to the emulator.
- Emulator web now forwards raw GPS and normalized touch contacts only; TypeScript no longer owns pan/pinch/rotate/recenter product policy.
- `cargo xtask emu` now rebuilds `render-core-wasm` and starts the emulator dev server from the repository root.
- Firmware now has a host-side shared runtime/query/render loop for parity work, but board-level device wiring, shared parity fixtures, and device-side `xtask` flows are still pending.

## Target Runtime Behavior
- Heading-up camera transform in shared Rust renderer.
- Riding mode camera:
  - heading-up
  - rider anchor near lower quarter of display
- Stopped mode camera:
  - centered rider
  - delayed smooth north-up settle
- Zoom policy:
  - close zoom-in target around 100 m context
  - zoom-out bounded for readability
- Temporary north-up override from top-right north indicator.
- Marker style upgrade (glowing directional riding marker + larger stopped marker).
- Dark high-contrast vector style with major/minor road hierarchy for circular minimap UI.
- Emulator geolocation support using browser location APIs.
- Emulator pinch/pan touch interactions (works on mobile browsers).

## Render Core Internals
- `render-core` public API is stable and stateless for adapters.
- Internal modules are split by concern:
  - `raster`
  - `style`
  - `visibility`
  - `math`

## Navigation Behavior
- Default while moving: riding mode with heading-up and lower-quarter rider anchor.
- Default while stopped: centered rider and north-up after delay.
- One-finger drag: temporarily pan map.
- Two-finger pinch: zoom in/out.
- Two-finger rotate: temporarily rotate map heading while moving.
- After brief idle: smooth auto-return to current GPS focus.
- North indicator tap: temporary north-up override; auto-return to riding mode while moving.

## Map Data Flow
- Source maps: `/work/map-src` (`*.mbtiles`)
- Converted maps: `/work/map-data` (`city.svm`)
- Current runtime bridge: embedded `.svm` bytes + shared `map-runtime` coarse query index for firmware and emulator integration.
- `cargo xtask prepare-map` applies a bike profile that excludes ferry/boat/water transport lanes.

## Commands
Prerequisites:
- `wasm-pack` for emulator builds (`cargo install wasm-pack`)
- JS package manager for emulator web deps (`npm` recommended; `pnpm`/`bun` supported)

Prepare maps:
```bash
cargo xtask prepare-map
```

Run emulator:
```bash
cargo xtask emu
```

Bundle firmware + map for device:
```bash
cargo xtask bundle-device
```

Deploy firmware (requires `espflash`):
```bash
cargo xtask deploy-device --port /dev/ttyUSB0
```

Current status: `cargo xtask emu` is live; `prepare-map`, `bundle-device`, and `deploy-device` still need real command bodies.

## Specs and Plans
- Main plan: `/work/docs/current-plan.md`
- Main TODO list: `/work/docs/todo.md`
- Main spec: `/work/docs/project-spec.md`
- Framework execution guide: `/work/docs/framework-execution-guide.md`
- Source tree guide: `/work/docs/source-tree.md`
- CVE tracking plan: `/work/docs/cve-tracking-plan.md`
- Converter spec: `/work/map-vector-cli/docs/project-spec.md`
- Converter plan: `/work/map-vector-cli/docs/current-plan.md`
- Emulator spec: `/work/emulator/docs/project-spec.md`
- Emulator plan: `/work/emulator/docs/current-plan.md`
