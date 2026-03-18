# ESP32-P4 Web Emulator

Browser emulator for the ESP32 bike minimap, backed by the shared Rust runtime and renderer.

## Purpose
- Simulate the target display shape and browser/device input path before flashing firmware.
- Reuse the same shared Rust behavior as firmware for camera, motion, query, and overlay logic.
- Keep emulator TypeScript focused on input forwarding, lifecycle, and presentation.

## Shared Behavior
- Browser GPS or manual bike simulation feed shared runtime inputs.
- Shared Rust owns:
  - riding/stopped camera transitions
  - heading-confidence handling
  - pan, pinch, and rotate behavior
  - auto-recenter and follow-lock
  - compass preview/lock interactions
- Current camera UX:
  - confident movement uses smooth `Travel-Up Auto`
  - stopped view returns to centered north-up
  - single tap enters temporary `North Preview`
  - double tap enters `North Locked`

## Quick Start
Prerequisites:
- Rust toolchain
- `wasm-pack`
- Node.js `>=20.19`

Run from repo root:
```bash
cargo xtask emu
```

Then open the local URL printed by Vite.

## Common Workflow
1. Start the emulator with `cargo xtask emu`.
2. Grant location permission to test live GPS mode.
3. Use arrow keys or on-screen controls for deterministic bike simulation.
4. Drag to pan, pinch to zoom, rotate only while in `Travel-Up Auto`, and wait for recenter.
5. Use the bike-physics controls to tune simulation behavior when needed.

## Web Checks
Run in `emulator/web`:
```bash
npm run lint
npm run typecheck
npm run build
```

## Documentation
- Emulator spec: [`./docs/project-spec.md`](./docs/project-spec.md)
- Camera UX: [`../docs/camera-rotation-design.md`](../docs/camera-rotation-design.md)
- Architecture: [`./docs/architecture.md`](./docs/architecture.md)
- Frontend stack: [`./docs/frontend-stack.md`](./docs/frontend-stack.md)
