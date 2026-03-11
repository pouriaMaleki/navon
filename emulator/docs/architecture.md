# Emulator Architecture

## Purpose
Fast browser simulator for ESP32-P4 minimap rendering and GPS/touch interaction checks.

Canonical requirements are defined in [`project-spec.md`](./project-spec.md).

## Ownership Rule
- Emulator is a hardware/runtime harness, not the owner of product navigation behavior.
- Product camera logic must be implemented in shared Rust (`render-core`) and consumed through `render-core-wasm`.
- Browser TypeScript code should forward inputs and render outputs, not define independent riding/stopped policy.
- Emulator-only bike controls may move simulated GPS position and publish simulated course heading as GPS input, but must not directly drive camera rotation state.

## Modules
1. `render-core` (Rust)
- Shared renderer used by firmware and emulator.

2. `render-core-wasm` (Rust + wasm-bindgen)
- WASM bridge exposing renderer to web runtime.

3. `emulator/web` (TypeScript)
- Runtime shell for input, frame scheduling, and canvas presentation.
- React UI components and MobX stores.
- Manual simulated bike movement is handled by a separate physics module and store wiring.
- `GeoStore` owns GPS source state and forwards either browser GPS or simulated bike samples.
- `BikeSimStore` owns keyboard/on-screen control state and per-frame simulation updates.

4. `xtask` (Rust CLI)
- Builds WASM and runs emulator tooling workflow.

## Data Flow
Browser input/geolocation -> MobX store state -> WASM camera state -> Rust renderer pixel buffer -> canvas upload.

## Frontend Organization
- `ui/`: React components (view layer).
- `stores/`: MobX state, browser API integration, and lifecycle logic.
- `simulation/`: deterministic bike movement/physics helpers for emulator-only fallback input.
- `programs/`: program-specific bridge from emulator state into WASM renderer API.
- `core/`: rendering surface, canvas target, timing/input plumbing.
- `types.ts`: shared TypeScript contracts across modules.

See [`frontend-stack.md`](./frontend-stack.md) for implementation conventions.
