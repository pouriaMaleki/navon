# ESP32 Bike Minimap Platform

Rust-based minimap platform for ESP32, aimed at a bike-mounted game-style map experience.

## Product Split
- Main ESP32 project (`/work`): runtime minimap behavior (GPS follow, heading-up, zoom/pan).
- Map conversion project (`/work/map-vector-cli`): host-side city map conversion into `.svm`.

## Phase 3 Features (Current)
- Heading-up camera transform in shared Rust renderer.
- Zoom + pan camera controls.
- Smooth auto-recenter after temporary pan.
- Emulator geolocation support using browser location APIs.
- Emulator pinch/pan touch interactions (works on mobile browsers).

## Navigation Behavior (Current)
- Default: follow mode (user stays centered).
- One-finger drag: temporarily pan map.
- Two-finger pinch: zoom in/out.
- After brief idle: smooth auto-return to current GPS center.

## Map Data Flow
- Source maps: `/work/map-src` (`*.mbtiles`)
- Converted maps: `/work/map-data` (`city.svm`)
- Current runtime bridge: `.svm` -> generated Rust module for wasm renderer integration.

## Commands
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
- Converter spec: `/work/map-vector-cli/docs/project-spec.md`
- Converter plan: `/work/map-vector-cli/docs/current-plan.md`
- Emulator spec: `/work/emulator/docs/project-spec.md`
- Emulator plan: `/work/emulator/docs/current-plan.md`
