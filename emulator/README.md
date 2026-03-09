# ESP32-P4 Web Emulator

Browser-based emulator for ESP32-P4 minimap behavior, backed by the shared Rust renderer (`render-core` via WASM).

## What This Project Does
- Simulates the target display profile (Waveshare ESP32-P4 `800x800`).
- Feeds browser GPS data (or simulation fallback) into the map camera.
- Supports drag pan, pinch zoom, wheel zoom, and smooth auto-recenter.
- Reuses the same render core as firmware for visual/behavior parity.

## Quick Start
Prerequisites:
- Rust toolchain
- `wasm-pack` (`cargo install wasm-pack`)
- Node.js `>=20.19` (Node 22 recommended)

Run from repo root:
```bash
cargo xtask emu
```

Then open the local URL printed by Vite (usually `http://localhost:5173`).

## Common Workflow
1. Start emulator with `cargo xtask emu`.
2. Grant location permission in browser to test live GPS mode.
3. Drag to pan, pinch/wheel to zoom, and wait for recenter.
4. Use `Request GPS` in UI if permission was denied initially.

## Development Commands
Run in `emulator/web`:
```bash
npm run lint
npm run lint:fix
npm run format
npm run typecheck
npm run build
```

## Documentation
- Canonical emulator spec: [`docs/project-spec.md`](./docs/project-spec.md)
- Current implementation plan: [`docs/current-plan.md`](./docs/current-plan.md)
- Architecture overview: [`docs/architecture.md`](./docs/architecture.md)
- React + MobX + CSS Modules setup: [`docs/frontend-stack.md`](./docs/frontend-stack.md)

## Repository Layout
- `web/`: Vite + React app for emulator runtime.
- `web/src/programs`: WASM-backed render program binding.
- `web/src/stores`: MobX state and input/geolocation integration.
- `web/src/ui`: React components styled with CSS Modules.
- `run.sh`: convenience wrapper delegating to `cargo xtask emu`.
