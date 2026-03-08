# Agent Workspace Guide

## Canonical Specs
- Main project spec: `/work/docs/project-spec.md`
- Emulator spec: `/work/emulator/docs/project-spec.md`
- Converter spec: `/work/map-vector-cli/docs/project-spec.md`

## Architecture Boundaries
- Main project (`/work`): runtime camera/render/input behavior.
- Converter (`/work/map-vector-cli`): source map conversion + `.svm` standard.
- Do not move source-conversion concerns into firmware runtime.

## Map Folders
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data`

## Core Commands
```bash
cargo xtask prepare-map
cargo xtask emu
cargo xtask bundle-device
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Current Product Direction (Bike Minimap)
- Center-follow user location.
- Heading-up orientation.
- Pinch zoom and temporary pan.
- Smooth auto-recenter after pan idle.

## Navigation Test Guide (Minimal)
- Run `cargo xtask emu` and open the URL printed by Vite.
- Grant browser location permission to test GPS-follow mode.
- Drag to pan, pinch to zoom, then release and wait for auto-recenter.
- Validate heading-up by moving device/position and checking map rotation alignment.

## Process
1. Update spec.
2. Update plan.
3. Implement.
4. Reconcile docs and validate commands.
