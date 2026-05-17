# Agent Workspace Guide

> Local overrides: `AGENTS.local.md` (gitignored, machine-specific).

## Canonical Specs
- Main project spec: `/work/docs/project-spec.md`
- Emulator spec: `/work/device/emulator/docs/project-spec.md`
- Converter spec: `/work/tools/map-vector-cli/docs/project-spec.md`

## Architecture Boundaries
- Main project (`/work`): runtime camera/render/input behavior.
- Converter (`/work/tools/map-vector-cli`): source map conversion + `.svm` standard.
- Do not move source-conversion concerns into firmware runtime.

## Map Folders
- Source maps: `/work/data/map-src`
- Converted maps: `/work/data/map-data`

## Core Commands
```bash
cargo xtask prepare-map
cargo xtask emu
cargo xtask bundle-device
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Self-Hosted CI

The dev container runs a GitHub Actions runner (label `self-hosted, Linux`)
for all non-iOS, non-CVE CI jobs. The Mac runner (label `self-hosted, macOS`)
handles iOS only.

**First-time setup:** add `GITHUB_RUNNER_TOKEN=<token>` to `/work/.env`,
then rebuild. Get a token from:
https://github.com/pouriaMaleki/navon/settings/actions/runners/new

**Rebuilds:** the runner restarts automatically via `post-create.sh`.
Credentials live in `~/actions-runner/` (persistent home volume).

## Pre-Push Checks

Run these before pushing any change. All must exit clean.

```bash
cd companion-apps/web
npx tsc --noEmit                     # TS: 0 errors
npx vitest run                       # Tests: all pass
npx biome check                      # Lint + format: 0 errors
npm audit --production               # CVEs: 0

cargo xtask i18n-gen --check         # i18n: outputs match source
cargo xtask companion-ios-test       # iOS: build + XCTests green
```

Android (requires JDK + SDK):
```bash
cd companion-apps/android
./gradlew :app:testDebugUnitTest     # Tests: all pass
```

## Process
1. Update spec or add missing spec.
2. Update tests or add missing tests.
3. Update plan.
4. Implement.
5. Reconcile docs (very simple and easy to read yet detailed in what's critical) and validate commands.

## Invariant Checklist
- Identify the authoritative data source before editing behavior that can be represented in more than one way.
- Write down the invariants touched by the change before changing logic.
- Add or extend regression tests for every touched invariant, especially around ordering, timing, and state-machine reset behavior.
- Validate bridge, demo, and fixture data against shared-core expectations instead of trusting duplicated labels or hand-maintained semantics.

## Emulator Dev Notes (LLM Quick Rules)
- Emulator is a hardware/runtime simulator (`device/emulator/web`), not product-specific UI logic.
- Keep canonical emulator requirements in `/work/device/emulator/docs/project-spec.md`; other emulator docs should reference it.
- Frontend stack and conventions live in `/work/device/emulator/docs/frontend-stack.md` (React + MobX + CSS Modules + Biome).
- Keep shared emulator TS contracts in `/work/device/emulator/web/src/types.ts`.
- Prefer neutral naming in emulator APIs (`wasmProgram`, `WasmRuntimeState`), avoid feature/product-coupled names.
- Do not implement product camera policy in emulator TS. Riding/stopped/north-up behavior must be Rust-owned in `runtime-core` and surfaced via wasm bindings.
- If emulator and firmware behavior differ, fix shared Rust logic first; treat emulator-specific behavior forks as bugs.
