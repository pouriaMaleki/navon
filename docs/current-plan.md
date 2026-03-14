# Main Current Plan

Spec reference: [`project-spec.md`](./project-spec.md)
Execution guide: [`framework-execution-guide.md`](./framework-execution-guide.md)
Structure guide: [`source-tree.md`](./source-tree.md)

## Plan
1. Build the shared Rust framework first so firmware and wasm consume one runtime behavior model.
2. Restore the intended workspace crate graph by introducing `runtime-core`, `render-core`, and `render-core-wasm` with strict boundaries.
3. Define small stable public contracts for `RuntimeInputFrame`, `TouchContact`, `TouchContactFrame`, `RuntimeFrameOutput`, `RuntimeConfig`, `MapQuerySpec`, `DiagnosticsSnapshot`, and map query handoff before writing feature logic. Status: completed.
4. Implement deterministic runtime stepping in `runtime-core` for shared touch/contact interpretation, motion fusion, camera policy, follow-lock, recentering, and `MapQuerySpec` generation. Status: completed.
5. Keep `render-core` stateless and pure so it owns projection, final visibility/clipping, styling, and rasterization and can be tested independently. Status: emulator-facing render-core MVP is completed.
6. Integrate firmware and wasm as thin adapters that translate GPS and normalized touch contact frames only, while keeping coarse map lookup behind behavior-free `MapSource` implementations. Status: wasm/emulator slice is completed; firmware loop/device wiring remains pending.
7. Add scenario tests and diagnostics early so future features can extend the framework without regressions.
8. Prepare the runtime for shared direct `.svm` loading after the current embedded-wasm `.svm` bridge path is stable.

## Current Focus
- Reuse the new query/render slice for firmware-capable map access and parity validation.
- Replace the remaining `xtask` stubs once emulator/device flows have real implementations behind them.
- Keep reference docs accurate as architecture references while reserving status tracking for this plan and the todo list.

## Recent Progress
- `runtime-core::api` now exposes stable config, input, output, diagnostics, and map-query contract modules.
- `runtime-core` now owns a deterministic `bevy_ecs` schedule that produces shared gesture/tap interpretation, filtered motion heading, interaction-aware camera snapshots, and `MapQuerySpec` output.
- Firmware and wasm bridge helpers now build shared `RuntimeInputFrame` values instead of staying as empty placeholders.
- `render-core` now owns the shared camera projection, visibility/clipping, overlay drawing, and grayscale framebuffer path used by the emulator.
- `render-core-wasm` now embeds `/work/map-data/city.svm`, performs coarse bbox + LOD lookup, and exposes a frame-driven `step_frame` bridge for emulator consumption.
- Emulator web now forwards raw GPS and touch contacts into shared Rust and presents Rust-generated pixels without TS-owned camera interaction policy.

## Immediate Correction Pass
- Replace the hand-rolled runner with a real internal `bevy_ecs` schedule and resources. Status: completed.
- Add regression tests that pin the reviewed foundation issues so later work cannot silently reintroduce them. Status: completed.

## Exit Criteria For The Foundation Phase
- Workspace builds with the planned crate graph in place.
- `runtime-core` exposes adapter-safe public APIs without leaking ECS internals.
- Firmware and wasm adapters emit normalized contact data rather than adapter-defined pan/pinch/rotate/tap behavior.
- `render-core` owns pure render primitives and remains platform-agnostic.
- `RuntimeFrameOutput` exposes camera/query intent and diagnostics rather than queried geometry buffers.
- One deterministic frame pipeline is documented and testable from ordered input frames.
- Main docs, plan, and task list stay aligned with implementation reality and agree on ownership boundaries.
