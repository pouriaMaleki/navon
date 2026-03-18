# Emulator Current Plan

Spec reference: [`project-spec.md`](./project-spec.md)

## Plan
1. Keep emulator focused on hardware simulation: display + GPS + touch gestures.
2. Keep rendering parity with shared Rust/WASM renderer.
3. Add manual bike-sim GPS fallback with keyboard/on-screen controls for deterministic movement testing.
4. Keep emulator input thin and let shared Rust own pan/zoom/rotate/compass policy.
5. Maintain stable web build/typecheck workflow.
6. Keep docs open-source friendly and easy to onboard from.

## TODO
- [x] Wire browser geolocation updates to wasm map state.
- [x] Wire touch/pointer pan and pinch zoom controls.
- [x] Add smooth auto-recenter after pan idle timeout.
- [x] Replace monolithic DOM script with React UI components.
- [x] Add MobX app-level store composition (`AppStore` -> emulator/geo/touch stores).
- [x] Move GPS request/status logic into store layer.
- [x] Move gesture handling logic into store layer.
- [x] Switch to component CSS Modules.
- [x] Add emulator prerequisite docs and bootstrap for `wasm-pack`.
- [x] Pin devcontainer Node runtime for Vite 7 (`>=20.19`, set to Node 22).
- [x] Write dedicated frontend stack documentation (React + MobX + CSS Modules).
- [x] Consolidate complete emulator requirements into `docs/project-spec.md`.
- [x] Add dedicated bike physics module for manual simulated movement.
- [x] Add keyboard arrow control input for manual simulated GPS.
- [x] Add rendered movement controls below emulator screen.
- [x] Replace automatic circular fallback movement with controllable bike simulation.
- [x] Expose bike physics tuning controls in emulator UI (speed/accel/steer/brake).
- [x] Preserve bike speed-to-displacement consistency under low FPS using substep integration and partial catch-up.
- [x] Add reported-vs-measured speed diagnostics with mismatch warnings.
- [x] Clarify simulated bike course-heading vs camera-rotation ownership boundary.
- [x] Document current turn-visibility behavior (sim heading can lead visual camera rotation due to shared Rust smoothing path).
- [x] Rework shared camera rotation to use filtered travel heading with correct sign and slower response.
- [x] Align shared riding-mode camera heading with actual movement direction.
- [ ] Add explicit on-screen debug HUD (lat/lon/zoom/heading) toggle.
- [ ] Add replay mode for deterministic movement scenarios.
