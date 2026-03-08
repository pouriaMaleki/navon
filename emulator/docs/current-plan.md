# Emulator Current Plan

## Phase 3 Plan
1. Add browser geolocation feed into wasm renderer state.
2. Add multitouch/pointer gesture handling (pinch + pan).
3. Add smooth pan decay / recenter behavior.
4. Verify behavior on mobile browser.

## TODO
- [x] Wire browser geolocation updates to wasm map state.
- [x] Wire touch/pointer pan and pinch zoom controls.
- [x] Add smooth auto-recenter after pan idle timeout.
- [ ] Add explicit on-screen debug HUD (lat/lon/zoom/heading) toggle.
- [ ] Add replay mode for deterministic movement scenarios.
