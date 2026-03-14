# ESP32-P4 Touch Integration Plan

## Goal
Add a real-device touch input module for the Waveshare `ESP32-P4-WIFI6-Touch-LCD-3.4C` so firmware emits normalized touch contact frames consumed by the shared Rust runtime, which derives the same gestures and taps for firmware and wasm.

## Hardware Facts
- Target board: Waveshare `ESP32-P4-WIFI6-Touch-LCD-3.4C`.
- Display: `3.4"` round `800x800` IPS panel.
- Touch controller: `GT9271`.
- Touch capability: up to `10-point` capacitive touch.
- Display link: `MIPI DSI 2-lane`.
- Default board I2C pins from Waveshare wiki: `SCL=GPIO8`, `SDA=GPIO7`.
- The touch assembly also exposes dedicated `TP_INT` and `TP_RST` lines in the Waveshare schematic; exact GPIO mapping should be captured in a board config module when implementation starts.

## Architecture Boundary
- `runtime-core` owns camera behavior, normalized touch/contact interpretation, gesture recognition, tap recognition, north-up override rules, follow-lock/recenter policy, and tap interpretation for product controls.
- `render-core` owns overlay drawing only; it does not own touch semantics or camera state.
- Firmware touch module owns:
  - GT9271 bring-up
  - raw contact sampling
  - coordinate normalization
  - stable contact-frame packaging
  - dispatch into `RuntimeInputFrame`
- Emulator remains only a hardware/input simulator and should not define different gesture semantics.

## Proposed Module Layout
1. `firmware/src/touch.rs`
- Public entry point for board touch input.
- Owns driver state, previous-frame controller bookkeeping, and normalized contact-frame emission.

2. `firmware/src/board_config.rs`
- Stores board-specific constants:
  - `TOUCH_I2C_PORT`
  - `TOUCH_SCL_GPIO`
  - `TOUCH_SDA_GPIO`
  - `TOUCH_INT_GPIO`
  - `TOUCH_RST_GPIO`
  - display logical width and height
- Keeps Waveshare-specific wiring out of app logic.

3. `firmware/src/input_bridge.rs`
- Packages normalized GPS/touch samples into `RuntimeInputFrame`.
- Emits `TouchContactFrame` rather than app-level pan/pinch/rotate/tap semantics.

4. `runtime-core/src/input/{contacts,gestures,taps}.rs`
- Validates normalized contact frames.
- Derives shared `GestureEvent` and tap semantics from ordered contact sequences.
- Keeps control hit testing and product interaction rules in shared Rust.

## Runtime Data Flow
1. Initialize I2C master on the Waveshare defaults: `GPIO8/7`.
2. Reset and configure `GT9271` using `TP_RST` and `TP_INT`.
3. Poll or interrupt-trigger a touch frame read.
4. Decode up to 10 contacts from GT9271 report buffer.
5. Convert raw panel coordinates into normalized screen coordinates `0.0..1.0`.
6. Map normalized points into logical display coordinates for the `800x800` round screen.
7. Feed the normalized touch snapshot into `input_bridge.rs` so it builds a shared `TouchContactFrame` inside `RuntimeInputFrame`.
8. Let `runtime-core::input` derive internal pan/pinch/rotate/tap semantics from the ordered contact sequence.
9. Let `runtime-core` decide whether a tap hits the north indicator and whether camera mode changes.

## Driver Plan
1. Bus bring-up
- Use ESP-IDF/`esp-hal` I2C master with `GPIO8/7`.
- Disable internal pull-ups unless board-level testing proves they are needed; Waveshare says external pull-ups already exist.

2. GT9271 reset/address handshake
- Implement the standard reset sequence using `TP_RST` and `TP_INT`.
- Read product ID/config block to confirm the controller is alive before entering the main loop.
- Keep address and config values in one driver file, not scattered through app code.

3. Event acquisition
- Start with polling at frame cadence because it is simpler and deterministic.
- Reserve interrupt-driven wakeup via `TP_INT` as a second step if latency or CPU use becomes a problem.

4. Contact decoding
- Parse touch count and contact slots from the GT9271 report.
- Track stable finger IDs between frames because the shared recognizer depends on consistent contact identity.
- Ignore obviously invalid samples outside panel bounds.

## Shared Interaction Plan
1. Single touch
- Firmware forwards one active normalized contact in `TouchContactFrame`.
- `runtime-core` derives pan semantics, deadzone handling, and camera response from the shared contact sequence.

2. Multi-touch
- Firmware forwards two active normalized contacts with stable IDs.
- `runtime-core` derives pinch and rotate from shared distance/angle deltas.
- Centroid movement remains available for future two-finger pan work without changing adapter semantics.

3. Tap recognition
- `runtime-core` classifies short low-travel contact sequences as taps.
- `runtime-core` resolves north-indicator and future control hit testing identically on device and emulator.

4. Release behavior
- Contact-count changes reset shared recognizer state cleanly.
- Adapters must not carry app-level gesture state across finger transitions; they only forward stable contact identities and positions.

## Sources
- Waveshare product page: `ESP32-P4-WIFI6-Touch-LCD-3.4C` specs for `800x800`, `GT9271`, and `10-point` touch.
- Waveshare wiki: board overview and default I2C pins `SCL(GPIO8)` and `SDA(GPIO7)`.
