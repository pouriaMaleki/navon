# Agent Workspace Guide

## Canonical Project Definition
- The source of truth for project definition is:
  - `/work/docs/project-spec.md`
- Emulator source of truth is:
  - `/work/emulator/docs/project-spec.md`
- Agents (Codex or others) must read and follow that file before proposing plans or implementation changes.
- If a user updates project definition, update `docs/project-spec.md` first, then update plan docs.

## External Sample Repositories

### `minimap/`
- Meaning: shorthand alias used in this workspace for the sample repo:
  - `/tmp/Video_Game_Mini_Maps-fork`
- Source: `https://github.com/garagetinkering/Video_Game_Mini_Maps`
- Purpose: reference/sample code only. Do not treat it as part of this repository.

## Usage Convention
- If a prompt says "check minimap repo" or "look in minimap/", use:
  - `/tmp/Video_Game_Mini_Maps-fork`
- Keep changes for this project inside `/work` unless explicitly asked to edit `minimap/`.

## If Missing
If `/tmp/Video_Game_Mini_Maps-fork` does not exist, clone it again:
```bash
git clone https://github.com/garagetinkering/Video_Game_Mini_Maps.git /tmp/Video_Game_Mini_Maps-fork
```

## Project Process Style
- For this project, agents should follow this workflow:
  - `1)` capture/update project definition in `docs/project-spec.md`
  - `2)` create or refresh `docs/current-plan.md`
  - `3)` implement in small phases
  - `4)` update both docs as scope evolves
- For emulator work, mirror the same flow under:
  - `/work/emulator/docs/project-spec.md`
  - `/work/emulator/docs/current-plan.md`
- Treat this process as default unless the user explicitly asks to skip it.

## Developer Playbook

### Source Of Truth (Code)
- Shared renderer core:
  - `/work/render-core`
- Firmware app:
  - `/work/firmware`
- WASM adapter:
  - `/work/render-core-wasm`
- Web emulator:
  - `/work/emulator/web`
- Orchestration command:
  - `/work/xtask`

### Build And Test
- Renderer core tests:
```bash
cargo test -p esp32-screen-render-core
```
- WASM adapter tests:
```bash
cargo test -p render-core-wasm
```
- Firmware build (run from firmware dir to use ESP override toolchain):
```bash
cd /work/firmware
. "$HOME/export-esp.sh"
cargo build
```

### Emulation
- Primary emulator flow (Rust-first):
```bash
cargo xtask emu
```
- Release preview:
```bash
cargo xtask emu --release
```
- If port `5173` is in use, Vite picks next free port (usually `5174`).

### Debug Notes
- Wokwi does not emulate ESP32-P4 LCD hardware stack; use it only as fallback serial preview.
- Browser emulator is the primary visual path and uses Rust renderer via WASM.
- If `cargo xtask emu` fails, check prerequisites:
  - `wasm-pack` installed
  - Node 20+ available
  - `npm install` works in `/work/emulator/web`
