# Web Emulator Module

Browser module for validating bike-minimap behavior quickly with the shared Rust/WASM renderer.

## Product Description
- Emulates target output style for Waveshare ESP32-P4 LCD profile (`800x800`).
- Uses browser geolocation to drive user location when permission is granted.
- Supports touch/pointer pinch zoom and temporary pan with smooth auto-recenter.
- Uses the same shared Rust renderer core as firmware via WASM (`render-core-wasm`).

## Interaction Quick Guide
- Allow location permission in browser to enable live GPS-follow behavior.
- GPS status is shown in the control bar (`live` vs `simulated` fallback).
- Drag with one finger/mouse to temporarily pan.
- Pinch with two fingers to zoom.
- Stop panning and wait briefly to see smooth auto-recenter to GPS position.

## Technology
- Language: TypeScript
- Toolchain: Vite 7 + TypeScript 5 (fast HMR/dev startup)
- Runtime: Browser canvas
- Shared renderer: Rust `render-core` via WASM bridge (`render-core-wasm`)

## Framework API
- Emulator runtime: `Esp32ScreenEmulator`
- Reusable framebuffer: `FrameBuffer`
- Screen profiles: Waveshare ESP32-P4 `800x800` and `720x720`
- Program interface: pluggable render/update/input lifecycle for project-specific logic

## Structure
- `docs/project-spec.md`: emulator specification.
- `docs/current-plan.md`: emulator execution plan and status.
- `web/`: TypeScript web app.
- `run.sh`: Rust-first launcher (delegates to `cargo xtask emu`).

## Run
Prerequisite:
- `wasm-pack` installed (`cargo install wasm-pack`)
- JS package manager available (`npm` recommended; `pnpm`/`bun` also supported)

```bash
cargo run -p xtask -- emu
```

VS Code tasks:
- `Emulator: ensure deps` installs `wasm-pack` if missing.
- `Emulator: run` depends on `Emulator: ensure deps`.

Open:
`http://localhost:5173` (or next available port shown in terminal output)

## Rust Workflow
Primary developer flow is a single cargo command from repository root:
```bash
cargo xtask emu
```

Implemented behavior:
- build shared Rust renderer to WASM
- sync web emulator assets
- start local emulator server

Release-style preview:
```bash
cargo xtask emu --release
```
