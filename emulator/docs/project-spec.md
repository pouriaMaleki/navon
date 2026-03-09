# Emulator Project Specification

## Goal
Validate bike-minimap runtime behavior quickly in browser with parity to Rust renderer logic.

## Required Phase 3 Behavior
- Use browser geolocation when available.
- Keep location-follow minimap behavior by default.
- Rotate map using heading-up orientation.
- Support touch/pointer pinch zoom.
- Support temporary pan and smooth auto-recenter.

## Technical Direction
- Emulator remains TypeScript + Vite shell.
- Rendering stays driven by Rust/WASM renderer core.
- Input/geolocation are browser-owned and fed into WASM state.
- Emulator build path requires `wasm-pack` as a host-side tool.
