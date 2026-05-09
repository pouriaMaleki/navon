# Agent Workspace Guide

## Canonical Specs
- Main project spec: `/work/docs/project-spec.md`
- Emulator spec: `/work/device/emulator/docs/project-spec.md`
- Converter spec: `/work/tools/map-vector-cli/docs/project-spec.md`

## Architecture Boundaries
- Main project (`/work`): runtime camera/render/input behavior.
- Converter (`/work/tools/map-vector-cli`): source map conversion + `.svm` standard.
- Do not move source-conversion concerns into firmware runtime.

## Map Folders
- Source maps: `/work/data/map-src`
- Converted maps: `/work/data/map-data`

## Core Commands
```bash
cargo xtask prepare-map
cargo xtask emu
cargo xtask bundle-device
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Process
1. Update spec or add missing spec.
2. Update tests or add missing tests.
3. Update plan.
4. Implement.
5. Reconcile docs (very simple and easy to read yet detailed in what's critical) and validate commands.

## Invariant Checklist
- Identify the authoritative data source before editing behavior that can be represented in more than one way.
- Write down the invariants touched by the change before changing logic.
- Add or extend regression tests for every touched invariant, especially around ordering, timing, and state-machine reset behavior.
- Validate bridge, demo, and fixture data against shared-core expectations instead of trusting duplicated labels or hand-maintained semantics.

## Emulator Dev Notes (LLM Quick Rules)
- Emulator is a hardware/runtime simulator (`device/emulator/web`), not product-specific UI logic.
- Keep canonical emulator requirements in `/work/device/emulator/docs/project-spec.md`; other emulator docs should reference it.
- Frontend stack and conventions live in `/work/device/emulator/docs/frontend-stack.md` (React + MobX + CSS Modules + Biome).
- Keep shared emulator TS contracts in `/work/device/emulator/web/src/types.ts`.
- Prefer neutral naming in emulator APIs (`wasmProgram`, `WasmRuntimeState`), avoid feature/product-coupled names.
- Do not implement product camera policy in emulator TS. Riding/stopped/north-up behavior must be Rust-owned in `runtime-core` and surfaced via wasm bindings.
- If emulator and firmware behavior differ, fix shared Rust logic first; treat emulator-specific behavior forks as bugs.
