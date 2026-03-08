# Web Emulator Module

Browser module that emulates the minimap product output for rapid iteration.

## Product Description
- Emulates target output style for Waveshare ESP32-P4 LCD profile (`800x800`).
- Provides high-speed visual feedback loop for rendering changes before hardware runs.
- Uses the same shared Rust renderer core as firmware via WASM (`render-core-wasm`).
- Designed as a reusable framework for other ESP32 projects using the same screen profile.

## Technology
- Language: TypeScript
- Toolchain: Vite 7 + TypeScript 5 (fast HMR/dev startup)
- Runtime: Browser canvas
- Shared renderer: Rust `render-core` via WASM bridge (`render-core-wasm`)

## Framework API
- Emulator runtime: `Esp32ScreenEmulator`
- Reusable framebuffer: `FrameBuffer`
- Screen profiles: Waveshare ESP32-P4 `800x800` and `720x720`
- Program interface: pluggable render/update/input lifecycle for project-specific logic

## Structure
- `docs/project-spec.md`: emulator specification.
- `docs/current-plan.md`: emulator execution plan and status.
- `web/`: TypeScript web app.
- `run.sh`: Rust-first launcher (delegates to `cargo xtask emu`).

## Run
```bash
cargo run -p xtask -- emu
```

Open:
`http://localhost:5173`

## Rust Workflow
Primary developer flow is a single cargo command from repository root:
```bash
cargo xtask emu
```

Implemented behavior:
- build shared Rust renderer to WASM
- sync web emulator assets
- start local emulator server

Release-style preview:
```bash
cargo xtask emu --release
```
