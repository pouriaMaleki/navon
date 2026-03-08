# Agent Workspace Guide

## Canonical Project Definition
- Main project spec:
  - `/work/docs/project-spec.md`
- Emulator spec:
  - `/work/emulator/docs/project-spec.md`
- Map conversion spec:
  - `/work/map-vector-cli/docs/project-spec.md`
- Agents must read the relevant spec before making plan/implementation changes.

## External Sample Repository Alias
### `minimap/`
- Path: `/tmp/Video_Game_Mini_Maps-fork`
- Source: `https://github.com/garagetinkering/Video_Game_Mini_Maps`
- Reference-only; not part of this repository.

## Process Style
1. Update spec first.
2. Update current plan.
3. Implement.
4. Reflect scope changes in docs.

Apply this process in each project:
- Main: `/work/docs/*`
- Emulator: `/work/emulator/docs/*`
- Converter: `/work/map-vector-cli/docs/*`

## Source Of Truth (Code)
- Shared renderer core: `/work/render-core`
- Firmware app: `/work/firmware`
- WASM adapter: `/work/render-core-wasm`
- Web emulator: `/work/emulator/web`
- Orchestration (`xtask`): `/work/xtask`
- Map converter CLI: `/work/map-vector-cli`

## Map Folders
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data`

## Core Commands
Emulator flow:
```bash
cargo xtask emu
```

Map prep only:
```bash
cargo xtask prepare-map
```

Device bundle:
```bash
cargo xtask bundle-device
```

Deploy firmware (requires espflash):
```bash
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Phase Rules
- Do not parse MBTiles in firmware runtime.
- Use `/work/map-vector-cli` for source conversion.
- Use `.svm` as canonical intermediate vector map format.
- Keep converter and runtime concerns separated.
