# Bike Minimap TODO

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
- [ ] Add dark-map styling profile with major/minor road hierarchy.
- [x] Add north indicator visual style aligned with minimap frame.

## Performance
- [x] Precompute camera transform constants once per frame.
- [x] Add viewport clipping before line rasterization.
- [x] Add adaptive quality mode while panning.
- [x] Instrument frame-time metrics in emulator for pan/zoom scenarios.

## Docs and Validation
- [x] Keep `/work/docs/project-spec.md` and `/work/emulator/docs/project-spec.md` in sync with implemented behavior.
- [ ] Validate interaction flow on desktop and mobile browser emulator.
- [ ] Reconcile final behavior notes in root and emulator README files.

## Real Device Touch
- [x] Add Waveshare `GT9271` touch board config and driver module for the `ESP32-P4-WIFI6-Touch-LCD-3.4C`.
- [x] Implement raw touch frame readout over the board I2C bus.
- [x] Add firmware gesture recognition for pan, pinch, rotate, and tap.
- [x] Add firmware rotate gesture input path into shared camera controller.
- [x] Replace mock touch scaffolding in `firmware/src/bin/main.rs` with real touch input.
- [ ] Add optional `TP_INT` / `TP_RST` controlled reset path after confirming exact GPIO mapping from the Waveshare schematic.
- [ ] Validate touch mapping, indicator taps, and gesture smoothness on real hardware.
