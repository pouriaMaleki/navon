# Bike Minimap TODO

## ECS Runtime Refactor (Design 1)
- [x] Add `runtime-core` crate with `bevy_ecs` schedule and runtime API (`new/step/reset`).
- [x] Add runtime resources/components for rider, camera, interaction, follow-lock, map query, frame time.
- [x] Add event-oriented runtime input model (`GpsFixEvent`, `GestureEvent`, `TapEvent`, `NorthUpRequest`).
- [x] Move runtime camera orchestration usage from adapters into `runtime-core`.
- [x] Add map-source abstraction and query-based bbox/LOD filtering.
- [x] Wire firmware runtime adapter to render only visible lines from `runtime-core` output.
- [x] Wire wasm runtime adapter to render only visible lines from `runtime-core` output.
- [x] Add initial runtime tests for camera transitions, pan-lock stability, and LOD filtering.
- [x] Add deterministic replay trace harness and parity assertions between native/wasm adapters.
- [x] Add perf and allocation profiling for ECS runtime under representative map loads.
- [x] Split `render-core` internals into dedicated modules while preserving public API.

## Immediate
- [x] Add explicit camera mode enum (`Riding`, `StoppedNorthUp`, `TemporaryNorthUp`).
- [x] Add movement detection thresholds and idle timers for transitions.
- [x] Implement rider anchor shift between lower-quarter and centered positions.
- [x] Implement delayed smooth rotation to north-up on stop.
- [x] Add top-right north indicator icon with tap-to-override behavior.
- [x] Add auto-return from temporary north-up to riding mode during sustained movement.
- [x] Lock camera follow target during manual pan so rider marker stays anchored until recenter completes.

## Zoom and Scale
- [x] Define zoom-in clamp that keeps visible area around rider near 100 m.
- [x] Define zoom-out clamp that preserves line readability.
- [x] Document overview-mode trigger conditions for future declutter design.

## Visuals
- [x] Add glowing yellow-green directional rider marker for riding mode.
- [x] Add larger stopped marker treatment for non-moving state.
- [x] Add dark-map styling profile with major/minor road hierarchy.
- [x] Add north indicator visual style aligned with minimap frame.

## Performance
- [x] Precompute camera transform constants once per frame.
- [x] Add viewport clipping before line rasterization.
- [x] Add adaptive quality mode while panning.
- [x] Instrument frame-time metrics in emulator for pan/zoom scenarios.

## Docs and Validation
- [x] Keep `/work/docs/project-spec.md` and `/work/emulator/docs/project-spec.md` in sync with implemented behavior.
- [ ] Validate interaction flow on desktop and mobile browser emulator (manual QA run).
- [x] Reconcile final behavior notes in root and emulator README files.

## Real Device Touch
- [x] Add Waveshare `GT9271` touch board config and driver module for the `ESP32-P4-WIFI6-Touch-LCD-3.4C`.
- [x] Implement raw touch frame readout over the board I2C bus.
- [x] Add firmware gesture recognition for pan, pinch, rotate, and tap.
- [x] Add firmware rotate gesture input path into shared camera controller.
- [x] Replace mock touch scaffolding in `firmware/src/bin/main.rs` with real touch input.
- [ ] Wire optional `TP_INT` / `TP_RST` GPIO reset sequence in firmware main after confirming exact Waveshare schematic mapping (`TouchInput::new_with_reset` hook exists).
- [ ] Validate touch mapping, indicator taps, and gesture smoothness on real hardware.
