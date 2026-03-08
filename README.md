# ESP32 Bike Minimap Platform

Rust-based minimap platform for ESP32, aimed at a bike-mounted game-style map experience.

## Product Split
- `main ESP32 project` (`/work`): renders a game-style minimap around rider location.
- `map conversion project` (`/work/map-vector-cli`): host-side CLI that converts large map sources into compact street-vector data.

This separation is intentional:
- Converter owns source map decoding and format normalization.
- ESP32 app owns real-time camera, rendering style, and interaction behavior.

## Bike Minimap Product Direction (Main Project)
- Rider location remains centered by default.
- Map orientation follows movement direction (game-like heading-up behavior).
- Zoom interaction target: two-finger zoom (future hardware support).
- Temporary pan/peek support, with smooth auto-recenter.

## Map Data Flow
- Source maps: `/work/map-src` (`*.mbtiles` for now)
- Converted city vectors: `/work/map-data` (`city.svm`)
- Runtime integration (current stage): generated Rust map module from `.svm`

## Commands
Prepare maps only:
```bash
cargo xtask prepare-map
```

Run emulator (includes map prep):
```bash
cargo xtask emu
```

Create deploy bundle (`firmware.elf` + `city.svm`):
```bash
cargo xtask bundle-device
```

Flash firmware to device (requires `espflash`):
```bash
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Docs
- Main spec: `/work/docs/project-spec.md`
- Main plan: `/work/docs/current-plan.md`
- Converter spec: `/work/map-vector-cli/docs/project-spec.md`
- Converter plan: `/work/map-vector-cli/docs/current-plan.md`
- Converter format options: `/work/map-vector-cli/docs/vector-format-options.md`
