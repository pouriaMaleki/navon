# Emulator Project Specification

## Goal
Provide a web emulator that reproduces the minimap product output and can evolve into a shared-core renderer with firmware.

## Product Description
- Primary purpose: emulate real device output behavior for development speed.
- Current scope: visual parity of Phase 1 minimap rendering.
- Future scope:
  - realtime stress validation
  - input model validation (touch/pan/zoom)
  - shared renderer core parity with firmware via Rust/WASM.

## Rendering Target
- Target hardware profile:
  - Waveshare ESP32-P4-WIFI6-Touch-LCD-XC
  - default mode: 800x800
- Required visual features:
  - circular minimap mask
  - ring border
  - north marker
  - moving player indicator
  - vector map primitives

## Technical Requirements
- Language for web module: TypeScript (no plain JS sources).
- Modern fast toolchain: Vite + TypeScript.
- Deterministic world-to-screen mapping aligned with firmware semantics.
- Architecture should allow migration to shared Rust renderer compiled to WASM.

## Rust Developer Experience Requirements
- Emulator should feel like existing firmware workflows (similar simplicity to Wokwi usage).
- Preferred entrypoint should be one command from project root.
- Required DX commands (target):
  - `cargo xtask emu`:
    - builds Rust renderer for WASM
    - ensures web dependencies are installed
    - starts emulator dev server
  - `cargo xtask emu --release`:
    - builds optimized WASM bundle
    - serves production-like emulator preview
- Keep firmware build flow untouched:
  - `cd /work/firmware && cargo build`
- Keep mental model simple:
  - Wokwi path for hardware-oriented runs
  - `xtask emu` path for fast visual iteration with shared Rust renderer

## Phase 1 Emulator Scope
- Render sample vector map.
- Animate sample player movement.
- Show target-like minimap output on web canvas.
- Include minimal controls for pause/resume and reset.

## Out of Scope (Current)
- Full board emulation of ESP32-P4 peripherals.
- Exact LCD controller timing/driver emulation.
