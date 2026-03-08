# ESP32 Minimap Platform

A Git-first ESP32 minimap project with declarative Dev Container setup and a shared Rust renderer used by both firmware and web emulator.

## What you get
- Dev container image with Rust, `rustfmt`, `clippy`, and `espup`
- Idempotent `postCreateCommand` that installs ESP toolchain and wires shell exports
- Shared Rust rendering core (`render-core`) reused by firmware and emulator (WASM)
- Preconfigured VS Code extensions via `.devcontainer/devcontainer.json`
- Simple compose service for repeatable local startup
- Clear layout:
  - `firmware/` for Rust ESP32 application code
  - `infra/devcontainer/` for container build files

## Repository bootstrap
This directory is a normal Git repository. Typical workflow:
```bash
git clone <your-remote-url>
cd <repo>
code .
```

## One-time Git identity setup (Dev Container)
Git identity inside the Dev Container can be separate from your host machine.
Before your first commit in this container environment, configure identity once:
```bash
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

If you prefer repository-local identity instead of global, run this in `/work`:
```bash
git config user.name "Your Name"
git config user.email "you@example.com"
```

## External sample alias for agents
This workspace defines `minimap/` as shorthand for an external sample repository:
- `/tmp/Video_Game_Mini_Maps-fork`
- Source: `https://github.com/garagetinkering/Video_Game_Mini_Maps`

When prompts say "check minimap repo" or "look in minimap/", they refer to that path.

## Project docs
- Canonical project spec: `/work/docs/project-spec.md`
- Current execution plan: `/work/docs/current-plan.md`
- Emulator spec: `/work/emulator/docs/project-spec.md`
- Emulator current plan: `/work/emulator/docs/current-plan.md`

## Target device and emulation
- Target display device: Waveshare ESP32-P4-WIFI6-Touch-LCD-XC (default 800x800 mode).
- Primary local emulation path: `/work/emulator` web module (`http://localhost:5173`), driven by shared Rust renderer core via WASM.
- Secondary fallback: Wokwi + serial monitor preview.
- Note: Wokwi does not currently emulate the ESP32-P4 + Waveshare LCD stack used by the reference project.

Run emulator from repo root:
```bash
cargo xtask emu
```

## Prerequisites
- VS Code
- VS Code extension: `Dev Containers` (`ms-vscode-remote.remote-containers`)
- Container runtime with Compose support:
  - `podman` with `podman compose` (or `podman-compose`), or
  - `docker` with `docker compose`
- If using Podman, set this in your VS Code **Default Profile** user settings:
  - `"dev.containers.dockerPath": "podman"`

## Start the dev environment
1. Open this folder in VS Code.
2. Run `Dev Containers: Reopen in Container`.
3. Wait for the container to build and attach.

VS Code extensions and settings are applied automatically from `.devcontainer/devcontainer.json`.

## ESP toolchain setup
On first container creation, `.devcontainer/devcontainer.json` runs:
`infra/devcontainer/scripts/post-create.sh`

It does:
- `espup install` only when `~/export-esp.sh` is missing
- Appends `source "$HOME/export-esp.sh"` to `~/.bashrc` once

If you need to re-run it manually:
```bash
bash infra/devcontainer/scripts/post-create.sh
```

## Build
Inside the container terminal:
```bash
cd /work/firmware
cargo build
```

Or use VS Code tasks:
- `Rust: cargo build`
- `Rust: cargo check`
- `Rust: cargo clippy`
- `Rust: cargo test`

## Optional host-side tasks
From VS Code task runner on the host:
- `Dev: build container image`
- `Dev: start container`
