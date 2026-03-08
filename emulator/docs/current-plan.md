# Emulator Current Plan

## Planning Basis
- Source spec: `/work/emulator/docs/project-spec.md`
- Hardware counterpart:
  - `/work/docs/project-spec.md`
  - `/work/docs/current-plan.md`

## Implementation Plan
1. Toolchain modernization
- Migrate emulator frontend to TypeScript.
- Use Vite dev/build pipeline.

2. Rendering parity baseline
- Keep sample vectors, world bounds, and movement pattern aligned with firmware.
- Render device-style circular minimap in canvas.

3. Realtime capability hardening
- Keep stable animation loop at interactive frame rates.
- Track render/update separation for future optimizations.

4. Input-readiness architecture
- Keep code structured so touch/pointer events can be routed through a common input state model.

5. Shared-core migration path
- Prepare interfaces to swap TypeScript renderer internals with Rust/WASM renderer later.

6. Rust-first command workflow
- Add `cargo xtask` commands so Rust developers run emulator with familiar cargo UX.
- Keep invocation as close as possible to existing build/sim workflows.

## Rust/WASM Migration Plan
1. Extract shared renderer core
- Move rendering logic from `firmware/src/lib.rs` into a new workspace crate (e.g. `/work/render-core`).
- Keep crate `no_std`-compatible and free of HAL/display-specific dependencies.

2. Add WASM adapter crate
- Add crate (e.g. `/work/render-core-wasm`) with `wasm-bindgen`.
- Expose:
  - init/state creation
  - step/update
  - render into shared framebuffer
  - pointer to framebuffer bytes for JS canvas upload

3. Wire emulator frontend to WASM
- Replace TypeScript minimap rendering internals with WASM calls.
- Keep TypeScript runtime for:
  - browser input
  - scheduling
  - presenting pixels on canvas

4. Add `cargo xtask` orchestration
- Add `/work/xtask` crate with commands:
  - `cargo xtask emu`
  - `cargo xtask emu --release`
- `xtask emu` responsibilities:
  - build WASM
  - prepare web assets
  - start Vite dev server

5. Add parity verification
- Golden frame fixtures from shared Rust core.
- Validate same fixture hashes in native and wasm execution.
- Integrate parity check into CI.

6. Add input roadmap hooks
- Define shared input event schema (`tap`, `drag`, `pinch/zoom` placeholders).
- Ensure API supports future touch-centric interactions without breaking renderer core.

## Current Status
- `step 1`: implemented (TypeScript + Vite setup)
- `step 2`: implemented (canvas renderer parity baseline)
- `step 3`: implemented (requestAnimationFrame loop + controls)
- `step 4`: implemented baseline (pointer input pipeline + control events)
- `step 5`: implemented (web now uses Rust/WASM renderer output)
- `step 6`: implemented baseline (`cargo xtask emu`)
- `Rust/WASM migration`: implemented v1

## Next Actions
1. Add pointer gesture semantics (pan/zoom/tap events on top of pointer baseline).
2. Add multi-scene fixtures and CI parity gates.
3. Package and publish OSS docs/examples for non-minimap projects.
