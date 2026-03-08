# Emulator Architecture

## Objective
Create a reusable emulator stack for ESP32 projects targeting Waveshare ESP32-P4 LCD profiles, with renderer parity between firmware and browser.

## System Layers
1. `render-core` (Rust, `no_std`)
- Source-of-truth rendering logic and device profile constants.
- Owns framebuffer operations and minimap rendering rules.
- Shared by firmware and WASM adapter.

2. `render-core-wasm` (Rust + `wasm-bindgen`)
- Thin adapter exposing `render-core` into browser WASM API.
- Owns WASM-visible emulator state and pixel buffer pointers.

3. `emulator/web` (TypeScript + Vite)
- Runtime shell for input, scheduling, and presentation.
- Uses WASM API for actual rendering (no duplicated renderer logic).
- Provides reusable emulator framework (`Esp32ScreenEmulator`).

4. `xtask` (Rust CLI)
- Developer entrypoint orchestration.
- Builds WASM package and starts web server with one command.

## Data Flow
- Browser frame tick -> TS runtime `update/render` -> WASM `step()` -> `render-core` writes pixel buffer -> TS uploads grayscale pixels to canvas.

## Public API Surfaces
- Rust (firmware):
  - `render_sample_device_style(...)`
  - `sample_player_for_tick(...)`
  - device profiles (`WAVESHARE_ESP32_P4_3_4`, `...4_0`)

- WASM:
  - `MinimapWasmEmulator::new(profile)`
  - `step()`, `reset()`
  - `pixels_ptr()`, `pixels_len()`

- TS runtime:
  - `Esp32ScreenEmulator<TCustom>`
  - pluggable `RenderProgram<TCustom>`

## Verification Strategy
- Unit tests in `render-core` for:
  - device spec constants
  - deterministic frame checksum
  - style mask behavior
- Browser build/type checks in `emulator/web`.
- Cross-target parity guaranteed structurally because firmware and web both call same `render-core` logic.

## TODO
- [x] Shared Rust renderer crate extracted
- [x] Firmware wired to shared renderer
- [x] WASM adapter crate added
- [x] Web emulator consumes WASM renderer
- [x] Rust-first run command via `xtask`
- [ ] Add gesture semantics (`tap`, `drag`, `pinch`) as shared input schema
- [ ] Add CI pipeline for renderer checksum parity across fixtures
- [ ] Publish package docs/examples for third-party projects
