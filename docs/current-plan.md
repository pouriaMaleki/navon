# Main Current Plan

Spec reference: [`project-spec.md`](./project-spec.md)
Execution guide: [`framework-execution-guide.md`](./framework-execution-guide.md)

## Plan
1. Build the shared Rust framework first so firmware and wasm consume one runtime behavior model.
2. Restore the intended workspace crate graph by introducing `runtime-core`, `render-core`, and `render-core-wasm` with strict boundaries.
3. Define small stable public contracts for `RuntimeInputFrame`, `TouchContact`, `TouchContactFrame`, `RuntimeFrameOutput`, `RuntimeConfig`, `MapQuerySpec`, `DiagnosticsSnapshot`, and map query handoff before writing feature logic. Status: completed.
4. Implement deterministic runtime stepping in `runtime-core` for shared touch/contact interpretation, motion fusion, camera policy, follow-lock, recentering, and `MapQuerySpec` generation. Status: completed.
5. Keep `render-core` stateless and pure so it owns projection, final visibility/clipping, styling, and rasterization and can be tested independently. Status: emulator-facing render-core MVP is completed.
6. Integrate firmware and wasm as thin adapters that translate GPS and normalized touch contact frames only, while keeping coarse map lookup behind behavior-free `MapSource` implementations. Status: wasm/emulator slice is completed; firmware host-side runtime/query/render slice is completed; firmware board-facing platform boundary and concrete `esp_idf` provider modules are in place; real ESP-IDF peripheral acquisition remains pending.
7. Add scenario tests and diagnostics early so future features can extend the framework without regressions.
8. Prepare the runtime for shared direct `.svm` loading after the current embedded `map-runtime` bridge path is stable.
9. Design and implement a declarative zoom-aware map presentation system with richer feature classes, multiple presentation bands, and converter-owned profiles.

## Current Focus
- Wire actual ESP-IDF peripheral acquisition and hardware handles into the new firmware `esp_idf` providers without reintroducing adapter-owned behavior.
- Replace the remaining device-oriented `xtask` stubs once firmware bundling and deploy flows have real implementations behind them.
- Design the next direct `.svm` runtime loading layer after the shared embedded bridge path has parity coverage.
- Plan the next map-system foundation so runtime/query/render can move beyond the current flat road/path model into richer zoom-aware presentation bands.

## Recent Progress
- `runtime-core::api` now exposes stable config, input, output, diagnostics, and map-query contract modules.
- `runtime-core` now owns a deterministic `bevy_ecs` schedule that produces shared gesture/tap interpretation, filtered motion heading, interaction-aware camera snapshots, and `MapQuerySpec` output.
- Firmware and wasm bridge helpers now build shared `RuntimeInputFrame` values instead of staying as empty placeholders.
- `render-core` now owns the shared camera projection, visibility/clipping, overlay drawing, and grayscale framebuffer path used by the emulator.
- `render-core` now consumes runtime-owned `MapQuerySpec.meters_per_pixel` values directly instead of recomputing zoom policy.
- `map-runtime` now owns the shared embedded `.svm` reader and coarse bbox + LOD lookup backend used by adapters.
- `map-runtime` now rejects invalid `.svm` magic/version/header payloads before parsing map geometry.
- `render-core-wasm` now queries geometry through `map-runtime` and exposes a frame-driven `step_frame` bridge for emulator consumption.
- Emulator web now forwards raw GPS and touch contacts into shared Rust and presents Rust-generated pixels without TS-owned camera interaction policy.
- Emulator presentation now uses a round clipped screen viewport, mobile touch forwarding, and desktop wheel-to-pinch synthesis while keeping gesture semantics Rust-owned.
- Emulator GPS normalization now preserves unknown heading as `null` and forwards browser-provided accuracy into shared runtime inputs.
- `cargo xtask emu` now rebuilds `render-core-wasm` and starts the Vite emulator server as the repository-root entrypoint required by the emulator spec.
- Firmware now runs a host-side shared `runtime-core` -> `map-runtime` -> `render-core` frame loop with tests covering query/render output and touch forwarding.
- `parity-fixtures` now provides canonical frame sequences, a deterministic fixture `MapSource`, and frame-by-frame parity assertions across firmware and wasm normalization/query/render paths.
- Firmware now exposes a board-facing platform loop, GT9271 report decoding/normalization, board config for Waveshare defaults, and RGB565 display upload helpers while keeping product behavior in shared Rust.
- Firmware touch normalization now explicitly targets the logical viewport size instead of assuming controller extent equals display extent.
- Firmware now also exposes concrete `esp_idf` provider modules for GT9271 register access, panel upload, NMEA GPS parsing, and device-platform assembly on top of the existing provider traits.
- A new map presentation design direction is now defined: richer feature classes, four zoom presentation bands, declarative converter profiles, and a preference for one multi-LOD regional map package.

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
