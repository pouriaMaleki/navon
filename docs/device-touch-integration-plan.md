# ESP32-P4 Touch Integration Plan

## Goal
Add a real-device touch input module for the Waveshare `ESP32-P4-WIFI6-Touch-LCD-3.4C` so firmware gesture behavior matches the shared Rust camera controller already used by firmware and wasm.

## Hardware Facts
- Target board: Waveshare `ESP32-P4-WIFI6-Touch-LCD-3.4C`.
- Display: `3.4"` round `800x800` IPS panel.
- Touch controller: `GT9271`.
- Touch capability: up to `10-point` capacitive touch.
- Display link: `MIPI DSI 2-lane`.
- Default board I2C pins from Waveshare wiki: `SCL=GPIO8`, `SDA=GPIO7`.
- The touch assembly also exposes dedicated `TP_INT` and `TP_RST` lines in the Waveshare schematic; exact GPIO mapping should be captured in a board config module when implementation starts.

## Architecture Boundary
- `render-core` owns camera behavior and north-indicator hit testing.
- Firmware touch module owns:
  - GT9271 bring-up
  - raw contact sampling
  - coordinate normalization
  - gesture recognition
  - dispatch into `BikeMinimapState`
- Emulator remains only a hardware/input simulator and should not define different gesture semantics.

## Proposed Module Layout
1. `firmware/src/touch.rs`
- Public entry point for board touch input.
- Owns driver state, previous-frame contacts, and gesture recognizer state.

2. `firmware/src/board_config.rs`
- Stores board-specific constants:
  - `TOUCH_I2C_PORT`
  - `TOUCH_SCL_GPIO`
  - `TOUCH_SDA_GPIO`
  - `TOUCH_INT_GPIO`
  - `TOUCH_RST_GPIO`
  - display logical width and height
- Keeps Waveshare-specific wiring out of app logic.

3. `firmware/src/gestures.rs`
- Converts contact deltas into:
  - pan delta
  - pinch zoom scale
  - rotate delta radians
  - tap events
- Stateless math helpers plus small state structs for tracking previous contacts.

## Runtime Data Flow
1. Initialize I2C master on the Waveshare defaults: `GPIO8/7`.
2. Reset and configure `GT9271` using `TP_RST` and `TP_INT`.
3. Poll or interrupt-trigger a touch frame read.
4. Decode up to 10 contacts from GT9271 report buffer.
5. Convert raw panel coordinates into normalized screen coordinates `0.0..1.0`.
6. Map normalized points into logical display coordinates for the `800x800` round screen.
7. Feed gesture outputs into `BikeMinimapState`:
   - single-finger drag -> `apply_pan_gesture(...)`
   - two-finger pinch -> `apply_pinch_gesture(...)`
   - two-finger rotation -> controller `rotate_delta_rad`
   - tap release -> `on_touch_tap_normalized(...)`

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
- Track stable finger IDs between frames.
- Ignore obviously invalid samples outside panel bounds.

## Gesture Plan
1. Single touch
- One active contact becomes pan input.
- Pan speed scaling should mirror emulator semantics but live in firmware constants, not hardcoded in app code.
- Add a deadzone so very small deltas do not drift the camera.

2. Multi-touch
- Two active contacts enable pinch and rotate simultaneously.
- Use centroid movement for optional future two-finger pan, but do not enable it initially unless it improves usability.
- Compute:
  - distance delta -> zoom scale
  - angle delta -> rotate delta

3. Tap recognition
- A short press with low travel becomes a tap.
- Feed tap into `on_touch_tap_normalized(...)` so the north indicator works identically on device and emulator.

4. Release behavior
- Gesture state resets cleanly when contact count changes.
- Avoid carrying stale pinch distance or angle across finger transitions.

## Sources
- Waveshare product page: `ESP32-P4-WIFI6-Touch-LCD-3.4C` specs for `800x800`, `GT9271`, and `10-point` touch.
- Waveshare wiki: board overview and default I2C pins `SCL(GPIO8)` and `SDA(GPIO7)`.
