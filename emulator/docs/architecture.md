# Emulator Architecture

## Purpose
Fast browser simulator for ESP32-P4 minimap rendering and GPS/touch interaction checks.

Canonical requirements are defined in [`project-spec.md`](./project-spec.md).

## Modules
1. `render-core` (Rust)
- Shared renderer used by firmware and emulator.

2. `render-core-wasm` (Rust + wasm-bindgen)
- WASM bridge exposing renderer to web runtime.

3. `emulator/web` (TypeScript)
- Runtime shell for input, frame scheduling, and canvas presentation.
- React UI components and MobX stores.

4. `xtask` (Rust CLI)
- Builds WASM and runs emulator tooling workflow.

## Data Flow
Browser input/geolocation -> MobX store state -> WASM camera state -> Rust renderer pixel buffer -> canvas upload.

## Frontend Organization
- `ui/`: React components (view layer).
- `stores/`: MobX state, browser API integration, and lifecycle logic.
- `programs/`: program-specific bridge from emulator state into WASM renderer API.
- `core/`: rendering surface, canvas target, timing/input plumbing.
- `types.ts`: shared TypeScript contracts across modules.

See [`frontend-stack.md`](./frontend-stack.md) for implementation conventions.
