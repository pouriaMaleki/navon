# Main Current Plan

Spec reference: [`project-spec.md`](./project-spec.md)
Execution guide: [`framework-execution-guide.md`](./framework-execution-guide.md)
Structure guide: [`source-tree.md`](./source-tree.md)

## Plan
1. Build the shared Rust framework first so firmware and wasm consume one runtime behavior model.
2. Restore the intended workspace crate graph by introducing `runtime-core`, `render-core`, and `render-core-wasm` with strict boundaries.
3. Define small stable public contracts for `RuntimeInputFrame`, `TouchContact`, `TouchContactFrame`, `RuntimeFrameOutput`, `RuntimeConfig`, `MapQuerySpec`, `DiagnosticsSnapshot`, and map query handoff before writing feature logic. Status: completed.
4. Implement deterministic runtime stepping in `runtime-core` for shared touch/contact interpretation, motion fusion, camera policy, follow-lock, recentering, and `MapQuerySpec` generation. Status: ECS-backed runner alignment and reviewed foundation fixes are complete; gesture/recenter/full policy remains pending.
5. Keep `render-core` stateless and pure so it owns projection, final visibility/clipping, styling, and rasterization and can be tested independently.
6. Integrate firmware and wasm as thin adapters that translate GPS and normalized touch contact frames only, while keeping coarse map lookup behind behavior-free `MapSource` implementations.
7. Add scenario tests and diagnostics early so future features can extend the framework without regressions.
8. Prepare the runtime for direct `.svm` loading after the generated Rust map bridge path is stable.

## Current Focus
- Build the next layer of shared runtime behavior on top of the now-aligned ECS foundation: touch interpretation, gestures, follow-lock, recentering, and richer motion confidence.
- Keep reference docs accurate as architecture references while reserving status tracking for this plan and the todo list.

## Recent Progress
- `runtime-core::api` now exposes stable config, input, output, diagnostics, and map-query contract modules.
- `runtime-core` now owns a deterministic `bevy_ecs` schedule that produces riding/stopped camera snapshots, projected world focus, and `MapQuerySpec` output.
- Firmware and wasm bridge helpers now build shared `RuntimeInputFrame` values instead of staying as empty placeholders.
- Runtime contract tests cover duplicate-touch rejection, stopped defaults, riding heading-up transition, and query-bound sizing.

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
