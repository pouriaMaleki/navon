# Current Plan

## Planning Basis
- Source spec: `/work/docs/project-spec.md`
- External reference: `minimap/` -> `/tmp/Video_Game_Mini_Maps-fork`
- Emulator module:
  - `/work/emulator/docs/project-spec.md`
  - `/work/emulator/docs/current-plan.md`

## Phase 1 Plan
1. Define rendering contract
- Choose coordinate system and map bounds for sample vector map.
- Define how world coordinates map to display pixels.

2. Build sample vector map
- Create a compact in-repo data model for lines/polygons.
- Add a small mock map (roads/walls/regions) with a few vector primitives.

3. Implement renderer core in Rust
- Render primitives into minimap-style output.
- Keep implementation deterministic and simple for ESP32 constraints.

4. Integrate with firmware entrypoint
- Hook renderer into `firmware/src/bin/main.rs`.
- Ensure update loop can redraw minimap on display.

5. Validate on target workflow
- Verify compilation and runtime behavior in dev container.
- Confirm visual output matches minimap intent for the mock sample.
- Use `/work/emulator` web emulator as primary visual feedback loop before device bring-up.

6. Document and prepare next phase
- Record constraints, performance observations, and known limitations.
- Propose Phase 2 scope (richer map data, style system, optimization).

## Current Status
- `project definition`: completed
- `implementation`: in progress
- `completed now`:
  - Step 1 (`rendering contract`): implemented with world bounds and world-to-screen mapping
  - Step 2 (`sample vector map`): implemented as static sample vector lines
  - Step 3 (`renderer core`): implemented in `firmware/src/lib.rs` minimap module
  - Step 4 (`integration`): wired into `firmware/src/bin/main.rs`
  - target-device alignment: set to Waveshare ESP32-P4 3.4-inch (800x800) style rendering profile
  - emulation paths:
    - web emulator module in `/work/emulator` (primary)
    - Wokwi serial preview mode (secondary fallback)
- `validation status`:
  - firmware `cargo check`: passing in `/work/firmware`
  - next: maintain renderer parity between firmware and web emulator
