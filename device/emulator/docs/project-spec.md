# Emulator Project Specification

This is the canonical specification for the emulator module. Other emulator docs should reference this file instead of redefining emulator requirements.

## Purpose
- Provide a fast browser-based simulator of ESP32-P4 minimap behavior before flashing firmware.
- Preserve firmware/emulator parity by routing browser inputs through the shared Rust runtime and renderer.

## In Scope
- `800x800` display simulation presented inside a round clipped viewport.
- Browser geolocation input with clear runtime status messaging.
- Deterministic manual bike simulation fallback driven by keyboard and on-screen controls.
- Raw touch/pointer forwarding into shared Rust:
  - pan
  - pinch zoom
  - rotate
  - tap handling
- Desktop wheel-to-pinch synthesis as an emulator-only convenience.
- Rendering through `render-core-wasm` using the shared runtime/query/render path.

## Strict Boundary
- Emulator web code may collect inputs and present outputs, but must not own product camera policy.
- Riding/stopped transitions, heading-confidence behavior, compass interaction, and recenter logic are shared Rust responsibilities.
- Manual bike simulation may publish `lat/lon/heading/speed`, but it must not directly control camera orientation policy.

## Camera And Interaction Requirements
- Shared Rust camera behavior must follow [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md).
- Shared-Rust orientation states are:
  - `Stopped North-Up`
  - `Heading Acquisition`
  - `Travel-Up Auto`
  - `North Preview`
  - `North Locked`
- `Travel-Up Auto` is the only moving state that uses the lower-quarter rider anchor.
- `Heading Acquisition`, `North Preview`, `North Locked`, and `Stopped North-Up` keep the rider centered.
- Low-confidence movement must hold the last trusted camera angle instead of rotating from noisy raw GPS course data.
- Two-finger rotate is only active in `Travel-Up Auto`.
- Single tap enters `North Preview`.
- Double tap enters `North Locked`.
- Tapping again unlocks north-up and returns to auto-follow when heading confidence is ready.
- Tapping the compass while already north-up should give a brief acknowledgement pulse without changing mode.
- During manual pan, the rider marker remains map-anchored while only camera offset moves.
- While riding, manual pan is temporary and auto-recenters after idle.
- While stopped, manual pan is sticky and must not auto-recenter after idle.
- While stopped and browsed away from center, tapping the north indicator must recenter to the rider location.
- While moving, the shared runtime should render the speed overlay in the bottom quarter with a solid black panel, white digits, and a `kph` / `mph` unit label.
- Tapping the speed panel must toggle units without affecting compass mode.
- Emulator adapters should restore the last selected speed unit from local storage and persist changes coming back from shared Rust.
- Emulator reset must reset shared runtime camera state.

## Visual Palette
- Emulator rendering should reflect the shared runtime/render palette rather than browser-only theme logic.
- Current shared palette:
  - `#050B12`
  - `#051E24`
  - `#10132B`
  - `#103B48`
  - `#12A3A3`
  - `#D7FF3F`
- Rider markers use `#D7FF3F`.

## Map Content Requirements
- Emulator map content should mirror shared Rust presentation-band behavior rather than emulator-specific layer toggles.
- Current POI behavior:
  - `Close Detail` shows bicycle parking, bicycle repair, supermarkets, and food
  - `Ride Detail` shows bicycle parking and bicycle repair only
  - overview bands hide POIs
- POI rendering remains part of the shared runtime/query/render path, so future device behavior should match the emulator.

## Manual Bike Simulation Requirements
- Arrow keys and on-screen controls must drive deterministic simulated movement.
- Default tuning should feel bicycle-like rather than arcade-snappy.
- Large frame stalls must use bounded substeps and capped catch-up to avoid teleport-like jumps.
- Physics parameters should remain adjustable from the emulator UI for tuning.
- Simulated heading may lead visible camera rotation because final camera heading still comes from shared movement-derived heading.

## Architecture And Structure
- `emulator/web` owns browser runtime concerns:
  - canvas
  - browser APIs
  - React UI
  - MobX stores
  - emulator-only bike physics
- `render-core-wasm` owns the wasm bridge to the shared runtime/query/render path.
- Shared cross-module TypeScript contracts live in `web/src/types.ts`.
- Architecture and ownership details are documented in [`architecture.md`](./architecture.md).

## Validation Requirements
- Required local checks:
  - `npm run lint`
  - `npm run typecheck`
  - `npm run build`
  - `cargo xtask emu` startup sanity check

## Related References
- Emulator README: [`/work/emulator/README.md`](/work/emulator/README.md)
- Camera UX: [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md)
- Frontend stack: [`./frontend-stack.md`](./frontend-stack.md)
- Emulator architecture: [`./architecture.md`](./architecture.md)
