# ESP32 Rust Dev Container

A Git-first, declarative ESP32 Rust development environment for VS Code.

## What you get
- Dev container image with Rust, `rustfmt`, `clippy`, and `espup`
- Idempotent `postCreateCommand` that installs ESP toolchain and wires shell exports
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
