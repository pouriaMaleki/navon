# ESP32 Bike Minimap Platform

Rust-based minimap platform for ESP32, aimed at a bike-mounted game-style map experience.

## Product Split
- Main ESP32 project (`/work`): runtime minimap behavior (GPS follow, heading-up, zoom/pan).
- Map conversion project (`/work/map-vector-cli`): host-side city map conversion into `.svm`.

## Runtime Architecture
- `runtime-core`: ECS runtime orchestration (camera policy, gesture integration, map query/LOD).
- `render-core`: stateless render primitives (camera view + visible lines -> framebuffer).
- `firmware` and `render-core-wasm`: platform adapters that translate input/output only.

## Current Runtime Behavior
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
- Current runtime bridge: `.svm` -> generated Rust module for wasm renderer integration.
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

## Specs and Plans
- Main spec: `/work/docs/project-spec.md`
- Main plan: `/work/docs/current-plan.md`
- CVE tracking plan: `/work/docs/cve-tracking-plan.md`
- Main phase 4 plan: `/work/docs/phase-4-riding-mode-plan.md`
- Main TODO list: `/work/docs/todo.md`
- Overview-mode design: `/work/docs/overview-mode-design.md`
- Converter spec: `/work/map-vector-cli/docs/project-spec.md`
- Converter plan: `/work/map-vector-cli/docs/current-plan.md`
- Emulator spec: `/work/emulator/docs/project-spec.md`
- Emulator plan: `/work/emulator/docs/current-plan.md`
