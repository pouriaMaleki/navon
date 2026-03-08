# Current Plan

## Phase 2.5 Plan (Corrected Split + Bike Minimap Direction)
1. Separate conversion concerns
- Move all source map conversion responsibilities to `/work/map-vector-cli`.
- Keep main project focused on runtime rendering and behavior.

2. Standardize map IO paths
- Use `/work/map-src` for input maps.
- Use `/work/map-data` for converted `.svm` outputs.

3. Build/deploy workflow
- Ensure map conversion runs before emulator/build flows.
- Provide device bundle command including firmware + map data.
- Provide deploy command scaffold for flashing firmware.

4. Runtime integration bridge
- Use `.svm` as canonical city map artifact.
- Continue using generated Rust window module as temporary bridge.
- Plan direct `.svm` runtime loading as next step.

5. Bike minimap behavior roadmap
- Center-follow player mode.
- Heading-up orientation.
- Zoom levels and smooth transitions.
- Temporary pan with smooth auto-recenter.

## TODO
- [x] Separate converter logic into standalone project.
- [x] Use `/work/map-src` and `/work/map-data` folders.
- [x] Add `xtask prepare-map` flow using converter CLI.
- [x] Add `xtask bundle-device` command.
- [x] Add `xtask deploy-device` command (espflash-based).
- [x] Update product descriptions/specs for both projects.
- [ ] Implement direct `.svm` runtime loader in ESP32 path.
- [ ] Implement heading-follow camera and recenter behavior in runtime renderer.
- [ ] Implement zoom and temporary pan behavior (touch-enabled stage).
