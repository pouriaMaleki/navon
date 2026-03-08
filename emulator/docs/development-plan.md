# Emulator Development Plan

## Milestone 1: Shared Core Parity
- [x] Move rendering code into `render-core`
- [x] Use same code from firmware
- [x] Provide deterministic sample scene and tick-based player movement

Acceptance:
- `render-core` tests pass
- firmware builds with `render-core` dependency

## Milestone 2: Browser Execution via WASM
- [x] Add `render-core-wasm`
- [x] Expose minimal API for stepping and framebuffer access
- [x] Replace TS renderer logic with WASM-backed rendering

Acceptance:
- web dev server starts with WASM module
- visible output updates in realtime

## Milestone 3: Rust-First Developer UX
- [x] Add `xtask emu` command
- [x] Make `emulator/run.sh` delegate to `xtask`
- [ ] Add `xtask emu --release` docs and smoke-check target

Acceptance:
- one command from repo root starts emulator stack

## Milestone 4: Device-Spec Validation
- [x] Assert target device dimensions in tests
- [x] Add deterministic frame checksum test
- [x] Add style-mask corner-clearing test
- [ ] Add multi-profile fixture tests (800x800 + 720x720 snapshots)

Acceptance:
- tests detect accidental deviations from target visual spec

## Milestone 5: OSS Readiness
- [x] Add license
- [x] Separate architecture docs
- [ ] Add README examples for integrating custom render programs
- [ ] Add contribution guide and semantic versioning policy
- [ ] Publish package metadata and release workflow

Acceptance:
- external user can install, run, and extend emulator without project-specific assumptions
