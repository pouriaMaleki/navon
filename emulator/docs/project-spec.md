# Emulator Project Specification

This is the canonical specification for the emulator module.  
Other emulator docs should reference this file instead of redefining product requirements.

## 1. Product Scope

### 1.1 Purpose
Provide a fast, browser-based simulator of ESP32-P4 minimap behavior for development and validation before flashing firmware.

### 1.2 In Scope
- Device display simulation for Waveshare ESP32-P4 `800x800`.
- GPS-driven user location updates (live browser geolocation + simulation fallback).
- Touch/pointer interactions:
  - Single-pointer drag pan.
  - Two-pointer pinch zoom.
  - Mouse wheel zoom.
- Smooth auto-recenter after pan idle timeout.
- Rendering through shared Rust renderer (`render-core`) via WASM bridge (`render-core-wasm`).

### 1.3 Out of Scope
- Source-map conversion pipeline and `.svm` schema changes (owned by `map-vector-cli`).
- Device flashing and firmware deployment workflows.
- Mobile native wrappers (not part of emulator runtime).

## 2. Functional Requirements

### 2.1 Display Simulation
- Must render to a browser canvas at `800x800` pixels.
- Must upload grayscale framebuffer data from WASM each frame.
- Must not apply emulator-only art direction that diverges from firmware output.

### 2.2 Location Input
- Must request browser geolocation when available and secure context permits.
- Must expose clear runtime status in UI (`initializing`, `requesting permission`, `live`, `simulated`, `denied`, `error`).
- Must fall back to deterministic simulated movement if live geolocation is unavailable.

### 2.3 Gesture Input
- Must support pointer drag panning while active pointer is tracked.
- Must support pinch zoom based on pointer distance ratio.
- Must support mouse wheel zoom.
- Must clamp zoom and pan to safe bounds.
- Must track last input timestamp for recenter behavior.

### 2.4 Camera Behavior
- Must pass current geo/camera values to WASM on each update tick.
- Must auto-recenter pan offsets after idle delay using smooth interpolation.
- Must reset camera state on emulator reset.

### 2.5 Runtime Controls
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
- `render-core-wasm` owns JS/WASM bridge layer only.
- Emulator must not absorb converter responsibilities from `map-vector-cli`.

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
