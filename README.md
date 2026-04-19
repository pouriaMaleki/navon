# ESP32 Bike Minimap Platform

Rust-based bike minimap platform with shared runtime behavior for firmware and emulator.

## Production (home lab)
The production stack is a single `docker compose` service from [compose.yaml](/host/esp32-map/compose.yaml). One nginx container serves both web apps:
- `/` — the companion web app (`companion-web/`, OSM-tiled planner).
- `/emulator/` — the device emulator (`emulator/web/`, Vite + `render-core-wasm`).

Start or update deployment:
```bash
docker compose up --build -d
```

View service logs:
```bash
docker compose logs -f emulator-web
```

Pull/build lifecycle:
```bash
docker compose pull
docker compose build --pull
docker compose up -d
```

Notes:
- Public port mapping is `0.0.0.0:${EMULATOR_PORT:-4173}:4173` for edge routing (for example `map.fiksu.me` upstream). The service name is still `emulator-web` so existing upstream routing keeps working even though it now hosts both apps.
- Migration from old stacks:
  - Stop legacy stacks if they are still running:
    - `docker compose -f docker-compose.yml down`
    - `docker compose -f compose.code-server.yaml down`
  - Remove obsolete code-server data volume only if you do not need it:
    - `docker volume rm esp32-code-server-data`

## Development (VS Code Remote SSH)
Canonical development flow is VS Code Remote SSH -> Reopen in Container.

Steps:
1. SSH to your Linux host from VS Code.
2. Open this repository.
3. Run `Dev Containers: Reopen in Container`.
4. In the devcontainer terminal:
   - `cargo xtask prepare-map`
   - `cargo xtask emu`
5. Open the forwarded URL on your local machine:
   - `http://localhost:5173`
6. Optional emulator route import checkpoint:
   - use `Import GPX` in the emulator controls, or the `GPX` button in fullscreen web mode, to load a `.gpx` file through the shared Rust importer

The devcontainer pins `CARGO_TARGET_DIR=/work/target/devcontainer` to isolate container build artifacts from host builds.
The devcontainer also persists full container home state at `~/.devcontainer-homes/esp32-map` on the host by mounting it to `/home/vscode`.
On first run after this change, host auth directories are migrated once when present:
- `~/.codex` -> `~/.devcontainer-homes/esp32-map/.codex`
- `~/.claude` -> `~/.devcontainer-homes/esp32-map/.claude`
- `~/.gemini` -> `~/.devcontainer-homes/esp32-map/.gemini`
Host SSH keys are mounted from `~/.ssh` to `/home/vscode/.ssh` for Git operations.
Devcontainer runtime is pinned to DNS servers `1.1.1.1` and `8.8.8.8` to avoid host resolver issues.
Production deployment remains unchanged and still uses [compose.yaml](/host/esp32-map/compose.yaml) only.

## Native Companion Apps
Native companion app scaffolds now live in `companion-ios` and `companion-android`.

They define the first app shell, HSL provider seam, BLE sync seam, reroute orchestration seam, and persistence/diagnostics boundaries for the routing program.

Current companion checkpoint:
- in-app HSL planning can run in live Digitransit mode when a subscription key is configured
- native GPX import now works in both companion apps through the platform document pickers
- sample fallback remains available when live planning is disabled or unavailable
- route alternatives can be chosen before sync
- reroute requests can be driven from an editable rider location in the app shell

Build notes:
- iOS uses XcodeGen to keep project configuration in text.
- Android uses Gradle Kotlin DSL for Android Studio / CLI builds on machines with Android SDK installed.
- Automated mobile build setup is documented in [`docs/companion-builds.md`](./docs/companion-builds.md).
