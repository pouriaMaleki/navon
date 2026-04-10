# Agent Workspace Guide

## Canonical Specs
- Main project spec: `/work/docs/project-spec.md`
- Emulator spec: `/work/emulator/docs/project-spec.md`
- Converter spec: `/work/map-vector-cli/docs/project-spec.md`

## Architecture Boundaries
- Main project (`/work`): runtime camera/render/input behavior.
- Converter (`/work/map-vector-cli`): source map conversion + `.svm` standard.
- Do not move source-conversion concerns into firmware runtime.

## Map Folders
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data`

## Core Commands
```bash
cargo xtask prepare-map
cargo xtask emu
cargo xtask bundle-device
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Current Product Direction (Bike Minimap)
- Center-follow user location.
- Heading-up orientation.
- Pinch zoom and temporary pan.
- Smooth auto-recenter after pan idle.

## Navigation Test Guide (Minimal)
- Run `cargo xtask emu` and open the URL printed by Vite.
- Grant browser location permission to test GPS-follow mode.
- Drag to pan, pinch to zoom, then release and wait for auto-recenter.
- Validate heading-up by moving device/position and checking map rotation alignment.

## Process
1. Update spec.
2. Update plan.
3. Implement.
4. Reconcile docs and validate commands.

## Invariant Checklist
- Identify the authoritative data source before editing behavior that can be represented in more than one way.
- Write down the invariants touched by the change before changing logic.
- Add or extend regression tests for every touched invariant, especially around ordering, timing, and state-machine reset behavior.
- Validate bridge, demo, and fixture data against shared-core expectations instead of trusting duplicated labels or hand-maintained semantics.


## Emulator Dev Notes (LLM Quick Rules)
- Emulator is a hardware/runtime simulator (`emulator/web`), not product-specific UI logic.
- Keep canonical emulator requirements in `/work/emulator/docs/project-spec.md`; other emulator docs should reference it.
- Frontend stack and conventions live in `/work/emulator/docs/frontend-stack.md` (React + MobX + CSS Modules + Biome).
- Keep shared emulator TS contracts in `/work/emulator/web/src/types.ts`.
- Prefer neutral naming in emulator APIs (`wasmProgram`, `WasmRuntimeState`), avoid feature/product-coupled names.
- Do not reintroduce Cordova/external bridge logic into emulator.
- Do not implement product camera policy in emulator TS. Riding/stopped/north-up behavior must be Rust-owned in `runtime-core` and surfaced via wasm bindings.
- If emulator and firmware behavior differ, fix shared Rust logic first; treat emulator-specific behavior forks as bugs.

### Emulator Validation Before Finish
Run in `/work/emulator/web`:
```bash
npm run lint
npm run typecheck
npm run build
```

## Companion App Checklist
- Preserve the single-surface Home plus full-screen Settings navigation model.
- Keep feature modules separate; do not route new work through a monolithic app state object.
- Keep platform-native UX conventions for map, search, share, and document-picker flows.
- Reuse canonical route and sync contracts; do not invent platform-specific route semantics.
- Keep universal Route Detail shared across imports and recents unless a source genuinely needs unique behavior.
