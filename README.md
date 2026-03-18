# ESP32 Bike Minimap Platform

Rust-based bike minimap platform with shared runtime behavior for firmware and emulator.

## Product Summary
- GPS-following round minimap with vector rendering.
- Shared camera UX:
  - `Heading Acquisition`
  - `Travel-Up Auto`
  - `North Preview`
  - `North Locked`
  - `Stopped North-Up`
- Confident movement uses smooth travel-up rotation and lower-quarter rider anchor.
- Stopped, heading-acquisition, and moving north-up states keep the rider centered.
- Shared Rust owns pan, pinch, rotate, follow-lock, auto-recenter, compass interaction, and heading-confidence behavior.

## Workspace Layout
- `/work`: main runtime/render/input behavior.
- `/work/map-vector-cli`: host-side map conversion into `.svm`.
- `runtime-core`: shared runtime policy and deterministic frame stepping.
- `render-core`: stateless renderer and overlay drawing.
- `map-runtime`: shared `.svm` parsing and coarse map query backend.
- `render-core-wasm`: wasm adapter for the emulator.
- `firmware`: device adapter over the shared runtime/query/render path.

## Common Commands
Prepare maps:
```bash
cargo xtask prepare-map
```

Run emulator:
```bash
cargo xtask emu
```

Bundle firmware:
```bash
cargo xtask bundle-device
```

Deploy firmware:
```bash
cargo xtask deploy-device --port /dev/ttyUSB0
```

## Documentation
- Main spec: [`/work/docs/project-spec.md`](/work/docs/project-spec.md)
- Camera UX: [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md)
- Map presentation system: [`/work/docs/map-presentation-system-design.md`](/work/docs/map-presentation-system-design.md)
- Runtime architecture: [`/work/docs/runtime-ecs-architecture.md`](/work/docs/runtime-ecs-architecture.md)
- Main plan: [`/work/docs/current-plan.md`](/work/docs/current-plan.md)
- Main TODO: [`/work/docs/todo.md`](/work/docs/todo.md)
- Emulator spec: [`/work/emulator/docs/project-spec.md`](/work/emulator/docs/project-spec.md)
- Converter spec: [`/work/map-vector-cli/docs/project-spec.md`](/work/map-vector-cli/docs/project-spec.md)
