# Current Plan

## Phase 3 Plan (GPS + Touch Bike Minimap)
1. Shared camera model in Rust renderer
- Add heading-up camera transform.
- Add zoom and pan offsets.
- Keep player marker rendering stable.

2. Firmware runtime behavior scaffold
- Add runtime state for GPS fixes and touch gestures.
- Support center-follow + temporary pan + smooth recenter.
- Keep no_std-compatible interfaces for later hardware drivers.

3. Emulator browser integration
- Use browser geolocation for user position.
- Use pointer multitouch gestures for pinch zoom and pan.
- Smoothly recenter after pan idle timeout.

4. Converter/runtime bridge updates
- Export world bounds metadata needed for GPS projection.
- Keep map conversion ownership in map-vector-cli.

5. Validation and next stage prep
- Validate emulator behavior on desktop and phone browser.
- Prepare next step: direct `.svm` runtime loading on ESP32.

## TODO
- [x] Add camera-based renderer path (heading + zoom + pan).
- [x] Add firmware bike minimap state scaffold for GPS/touch behavior.
- [x] Add emulator geolocation + multitouch controls.
- [x] Add map world-bounds metadata in generated map bridge.
- [x] Ensure `xtask emu` devcontainer bootstrap installs `wasm-pack`.
- [ ] Integrate real GPS/touch drivers on ESP32 hardware path.
- [ ] Implement direct `.svm` loader in firmware (replace generated module bridge).
- [ ] Add heading smoothing and user-configurable follow modes.
