# Query/Render Slice Review

## Findings

1. Medium: the riding marker no longer represents actual rider heading when camera orientation is temporarily decoupled from travel direction. [`runtime-core/src/output/mod.rs`](../runtime-core/src/output/mod.rs) sets `overlay.rider_heading_rad` to `camera.orientation_rad`, and [`render-core/src/overlay/mod.rs`](../render-core/src/overlay/mod.rs) renders the heading needle from `rider_heading_rad - camera.orientation_rad`. That makes the rider marker always point straight up on screen. During temporary north-up override or manual rotate, the spec expects the map orientation to diverge from actual travel direction, but the marker can no longer show that real heading. This conflicts with the temporary north-up / rotate behavior in [`project-spec.md`](./project-spec.md) and the forward-facing riding marker requirement in [`project-spec.md`](./project-spec.md).

2. Medium: emulator touch forwarding is attached to the entire panel instead of only the minimap canvas. [`emulator/web/src/stores/TouchStore.ts`](../emulator/web/src/stores/TouchStore.ts) binds pointer listeners to `canvas.parentElement ?? canvas`, while [`emulator/web/src/ui/EmulatorPanel.tsx`](../emulator/web/src/ui/EmulatorPanel.tsx) places `BikeControls` and error UI inside that same parent section. Pressing the on-screen bike controls can therefore generate normalized touch contacts relative to the map canvas, which violates the emulator boundary that browser code should forward raw map touches only and keep manual bike controls separate from runtime touch input. This conflicts with the gesture-input and strict-boundary rules in [`../emulator/docs/project-spec.md`](../emulator/docs/project-spec.md).

## Assessment

The project has made substantial progress against the spec. `runtime-core` now owns shared motion/camera/input behavior, `render-core` owns a real projection/clip/raster slice, and `render-core-wasm` plus the emulator now run an end-to-end Rust-owned query/render pipeline. Relative to the main plan, this is a real Phase 3 / emulator-side Phase 4 step, not just another skeleton pass.

The remaining issues are narrower and more product-facing than architectural. The boundaries are mostly in the right place now, but the current implementation still has one runtime-output semantic gap (actual rider heading vs camera orientation) and one emulator input-boundary bug (bike controls leaking into map touch input). Also, the repository still does not satisfy the emulator spec's one-command startup requirement yet: `cargo run -p xtask -- emu` still reports `xtask stub: emu is not implemented yet`.

## Validation

- `cargo fmt --all --check`
- `cargo test --workspace`
- `npm run lint` in `/work/emulator/web`
- `npm run typecheck` in `/work/emulator/web`
- `npm run build` in `/work/emulator/web`
- `cargo run -p xtask -- emu` (currently confirms the command is still a stub)
