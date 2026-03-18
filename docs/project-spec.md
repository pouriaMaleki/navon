# Project Specification

## Product
ESP32 bike minimap with a round, game-like map presentation for riding. The product follows GPS position, renders vector map data, and keeps firmware and emulator behavior aligned through shared Rust runtime code.

## User-Facing Behavior
- The camera has two top-level motion modes:
  - `Riding`
  - `Stopped`
- Shared Rust exposes five orientation substates:
  - `Stopped North-Up`
  - `Heading Acquisition`
  - `Travel-Up Auto`
  - `North Preview`
  - `North Locked`
- `Travel-Up Auto` is the default moving presentation when heading confidence is good:
  - trusted travel direction rotates toward the top of the screen
  - rider anchor moves to the lower quarter for look-ahead
- `Heading Acquisition`, `North Preview`, `North Locked`, and `Stopped North-Up` keep the rider centered.
- Single tap on the north indicator enters temporary `North Preview`.
- Double tap on the north indicator enters `North Locked`.
- Tapping again unlocks north-up and returns to auto-follow when heading confidence is ready.
- Tapping the north indicator while already north-up should not change mode, but should give a brief acknowledgement pulse.
- Low-confidence movement must hold the last trusted camera angle instead of following noisy raw GPS course data.
- One-finger pan, two-finger pinch, and two-finger rotate are shared-Rust behaviors; rotate is only active in `Travel-Up Auto`.
- Manual pan temporarily releases follow, keeps the rider marker map-anchored, and smoothly recenters after idle.
- The canonical orientation UX lives in [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md).

## Visual Palette
- Shared map and overlay rendering use this palette:
  - `#050B12`
  - `#051E24`
  - `#10132B`
  - `#103B48`
  - `#12A3A3`
  - `#D7FF3F`
- Background stays near-black.
- Map geometry uses the dark blue and teal range.
- Rider markers use `#D7FF3F`.
- Emulator and device should render from the same shared-Rust palette choices whenever possible.

## Architecture Boundaries
- `/work` owns runtime camera, motion, input, query, and render behavior.
- `/work/map-vector-cli` owns host-side map conversion and `.svm` format generation only.
- `runtime-core` owns stateful runtime policy:
  - motion estimation
  - gesture and tap interpretation
  - camera state transitions
  - follow-lock and recenter
  - map-query policy
- `render-core` owns stateless rendering:
  - projection
  - visibility and clipping
  - styling
  - overlay drawing
  - framebuffer generation
- `map-runtime` owns shared `.svm` parsing and coarse query lookup.
- `firmware` and `render-core-wasm` are adapters:
  - they translate platform I/O into shared contracts
  - they must not own product camera policy

## Map Presentation Direction
- The map system should evolve from a flat road-layer model into a zoom-aware presentation system with richer feature classes and declarative profiles.
- The preferred direction is one generated regional map package with multiple internal feature classes and LOD slices, rather than many unrelated zoom-specific files.
- Shared Rust runtime should choose the active presentation band from zoom and camera state.
- Converter-owned declarative profiles should decide which features exist for `bike`, `car`, `transit`, and future map modes.
- The canonical design lives in [`/work/docs/map-presentation-system-design.md`](/work/docs/map-presentation-system-design.md).

## Shared Runtime Contracts
- `RuntimeInputFrame`: ordered per-frame input envelope containing `dt`, optional GPS, and optional normalized touch contacts.
- `TouchContact` and `TouchContactFrame`: stable normalized touch input shared by firmware and wasm.
- `RuntimeFrameOutput`: runtime snapshot containing camera state, overlay state, `MapQuerySpec`, and optional diagnostics.
- `MapQuerySpec`: runtime-owned world/query intent used for coarse map lookup.
- `DiagnosticsSnapshot`: read-only debug surface for parity and runtime inspection.
- Shared public contracts must stay small, adapter-safe, and free of platform handles or ECS internals.

## Determinism And Validation
- The same ordered input frames must produce the same runtime behavior on firmware and wasm.
- Camera policy must prefer trusted movement-derived heading over raw GPS course whenever possible.
- `MapQuerySpec` must be a pure function of runtime state, viewport, and LOD policy.
- Final visibility and clipping stay in `render-core`, not in adapters or map query backends.
- Runtime and render logic must be covered by:
  - unit tests for math and policy
  - scenario tests for ride/stop/pan/recenter/compass flows
  - parity fixtures for firmware and wasm paths

## Current Implementation Direction
- Shared Rust already owns:
  - riding/stopped camera policy
  - heading-confidence handling
  - pan/pinch/rotate
  - compass preview/lock behavior
  - stopped north-up settle
  - auto-recenter
- Emulator web forwards raw GPS and normalized touch contacts only.
- Firmware follows the same runtime/query/render pipeline, with real hardware integration still being completed behind the adapter boundary.

## Supporting References
- Camera orientation UX: [`/work/docs/camera-rotation-design.md`](/work/docs/camera-rotation-design.md)
- Map presentation system: [`/work/docs/map-presentation-system-design.md`](/work/docs/map-presentation-system-design.md)
- Runtime architecture decision: [`/work/docs/runtime-ecs-architecture.md`](/work/docs/runtime-ecs-architecture.md)
- Framework execution guide: [`/work/docs/framework-execution-guide.md`](/work/docs/framework-execution-guide.md)
- Device touch integration: [`/work/docs/device-touch-integration-plan.md`](/work/docs/device-touch-integration-plan.md)
- Main plan: [`/work/docs/current-plan.md`](/work/docs/current-plan.md)
- Main TODO: [`/work/docs/todo.md`](/work/docs/todo.md)
