# Emulator Project Specification

This is the canonical specification for the emulator module.
Other emulator docs should reference this file instead of redefining product requirements.

## 1. Product Scope

### 1.1 Purpose
Provide a fast, browser-based simulator of ESP32-P4 minimap behavior for development and validation before flashing firmware.

### 1.2 In Scope
- Device display simulation for Waveshare ESP32-P4 `800x800`.
- GPS-driven user location updates (live browser geolocation + simulation fallback).
- Manual simulated bike movement fallback driven by keyboard and on-screen controls.
- Touch/pointer interactions:
  - Raw pointer/touch contact forwarding into shared Rust.
  - Single-pointer drag pan through shared Rust runtime behavior.
  - Two-pointer pinch zoom through shared Rust runtime behavior.
  - Two-pointer rotate heading through shared Rust runtime behavior.
- Smooth auto-recenter through shared Rust runtime behavior.
- Rendering through shared Rust renderer (`render-core`) via WASM bridge (`render-core-wasm`).

### 1.3 Strict Boundary
- Emulator is a hardware/runtime emulator only.
- Emulator web code may collect/forward inputs and present output, but must not own product camera policy.
- Riding/stopped mode transitions, north-up policy, and orientation behavior are shared Rust responsibilities.

## 2. Functional Requirements

### 2.1 Display Simulation
- Must render to a browser canvas at `800x800` pixels.
- Must upload grayscale framebuffer data from WASM each frame.
- Must not apply emulator-only art direction that diverges from firmware output.

### 2.2 Location Input
- Must request browser geolocation when available and secure context permits.
- Must expose clear runtime status in UI (`initializing`, `requesting permission`, `live`, `simulated`, `denied`, `error`).
- Must fall back to deterministic manual bike simulation if live geolocation is unavailable.

### 2.3 Manual Bike Simulation
- Must provide keyboard arrow control of simulated GPS position.
- Must render visible arrow controls below the emulated device screen.
- `ArrowUp` must accelerate simulated bike forward with gradual ramp-up.
- Default manual bike simulation tuning should target approximately `30 km/h` max speed with natural bicycle-like acceleration.
- Releasing acceleration input must let speed decay gradually as if coasting.
- Holding `ArrowDown` must brake much harder than passive coasting.
- `ArrowLeft` and `ArrowRight` must steer gradually; turning must be smooth and bicycle-like rather than instant heading snaps.
- Steering input must bend the forward travel path over time; it must not teleport the GPS point sideways.
- Simulated movement logic must live in a separate physics module, not inline in stores or UI components.
- Manual bike physics integration must preserve wall-time displacement under low FPS using bounded substeps:
  - max substep size `50 ms`
  - max simulated catch-up per frame `500 ms`
- Large frame stalls must use partial catch-up (`500 ms` cap) to avoid teleport-like jumps.
- Manual bike physics parameters (`max speed`, acceleration/deceleration, steering limits/response, wheelbase) must be adjustable from emulator UI for quick tuning.
- Manual bike diagnostics must report `reported speed` vs `measured ground speed` with target consistency:
  - straight-line tolerance around `±10%`
  - turning tolerance around `±15%`
- Manual bike simulation may publish a course/heading value derived from simulated motion as part of GPS input.
- Manual bike simulation currently publishes `lat/lon/heading/speed` from emulator physics into shared runtime inputs.
- Final camera heading currently comes from shared Rust movement-derived heading, not directly from raw simulator heading.
- Movement-derived heading is currently computed from map-point motion deltas and smoothed, so short turns may appear visually delayed under quantization/smoothing.
- Observability: simulator heading/turn-rate logs may change before visible map rotation catches up.
- Manual bike simulation must not directly set camera rotation policy or any emulator-only camera state in the renderer.

### 2.4 Gesture Input
- Must normalize browser pointer activity into shared touch-contact frames.
- Must preserve stable pointer IDs and touch phases (`started`, `moved`, `stationary`, `ended`, `cancelled`) for the wasm bridge.
- Must not derive pan/pinch/rotate/tap semantics in TypeScript.

### 2.5 Camera Behavior
- Must pass current geo/touch frame input to WASM on each update tick through a frame-driven bridge.
- Riding-mode camera heading must track direction of travel smoothly when movement direction is available from shared runtime inputs.
- Current behavior note: travel direction used for camera heading is derived from filtered map-point movement in shared Rust camera controller.
- Auto-recenter, follow-lock, north-up override, and gesture interpretation are owned by shared Rust.
- During manual pan, rider marker should remain map-anchored while camera offset moves; follow-target lock/release policy is owned by shared Rust camera controller.
- Must reset camera state on emulator reset.

### 2.6 Runtime Controls
- Must expose controls for pause/resume, reset, and GPS permission request.
- Must display runtime errors without crashing the whole page.

## 3. Non-Functional Requirements

### 3.1 Parity
- Emulator behavior should remain aligned with shared render-core logic used by firmware.

### 3.2 Developer Experience
- One-command startup from repository root: `cargo xtask emu`.
- Type-safe TypeScript codebase with strict mode enabled.
- Deterministic fallback behavior when browser GPS is unavailable.

### 3.3 Maintainability
- Canonical product requirements live in this file.
- Architecture and implementation docs must reference this spec.
- Shared cross-module TypeScript types must be centralized.

## 4. Architecture Constraints
- `emulator/web` owns browser runtime concerns (UI, input, geolocation, canvas).
- `render-core` owns render behavior and pixel generation.
- Camera mode/state behavior is Rust-owned in shared core and wasm bindings; emulator web code feeds inputs and consumes outputs.
- `render-core-wasm` owns the JS/WASM bridge plus the current embedded `.svm` query backend for emulator use.
- Emulator must not absorb converter responsibilities from `map-vector-cli`.
- Do not add emulator-only behavior branches that diverge from firmware runtime logic.

## 5. Technology Baseline
- TypeScript 5 (strict mode).
- Vite 7.
- React 19.
- MobX 6 + `mobx-react-lite`.
- CSS Modules for component-scoped styles.
- Biome for formatting and linting.
- Rust + wasm-bindgen output consumed from `web/wasm-pkg`.

## 6. Project Structure Requirements
- `web/src/ui`: React view components only.
- `web/src/stores`: MobX state orchestration and side effects.
- `web/src/core`: emulator engine primitives and canvas target.
- `web/src/simulation`: deterministic emulator-only movement/physics helpers.
- `web/src/programs`: rendering program adapters (WASM integration).
- `web/src/types.ts`: shared emulator-wide TypeScript contracts.

## 7. Build and Validation Requirements
- Emulator runtime requires Node.js `>=20.19`.
- WASM build path requires `wasm-pack`.
- Required local checks before merge:
  - `npm run lint` in `emulator/web`
  - `npm run typecheck` in `emulator/web`
  - `npm run build` in `emulator/web`
  - `cargo xtask emu` startup sanity check

## 8. Documentation Requirements
- `README.md`: open-source quick start and common workflows.
- `docs/architecture.md`: concise module/data-flow reference.
- `docs/frontend-stack.md`: React + MobX + CSS Modules patterns and conventions.
- `docs/current-plan.md`: active execution plan and TODO state.
- Root camera rotation design lives in `/work/docs/camera-rotation-design.md`.
