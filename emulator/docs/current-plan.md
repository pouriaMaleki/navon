# Emulator Current Plan

Spec reference: [`project-spec.md`](./project-spec.md)

## Plan
1. Keep emulator focused on hardware simulation: display + GPS + touch gestures.
2. Keep rendering parity with shared Rust/WASM renderer.
3. Keep interaction behavior simple: pinch zoom, drag pan, smooth recenter.
4. Maintain stable web build/typecheck workflow.
5. Keep docs open-source friendly and easy to onboard from.

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
- [ ] Add explicit on-screen debug HUD (lat/lon/zoom/heading) toggle.
- [ ] Add replay mode for deterministic movement scenarios.
