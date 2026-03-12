# Runtime ECS Architecture

## Decision
- Architecture: **Design 1 (ECS runtime core)**.
- Framework: `bevy_ecs` (`0.17.2` on Rust 1.88 toolchain).
- Upgrade note: `bevy_ecs 0.18.x` is deferred until Rust toolchain is raised to `1.89+`.
- Rollout: **Hybrid** (runtime orchestration move first, phased cleanup/perf next).

## Crate Graph
- `runtime-core`: owns runtime state/policy and map query/LOD decisions.
- `render-core`: stateless renderer primitives (`CameraView` + visible lines -> framebuffer).
- `firmware`: ESP32 adapter for touch/GPS input and framebuffer presentation.
- `render-core-wasm`: wasm adapter for browser/emulator input/output.

## ECS Model
### Resources
- `FrameTime`: deterministic frame clock (`dt_ms`, `total_ms`, `tick`).
- `RuntimeConfig`: viewport, bounds, background, anchor defaults.
- `LodPolicy`: zoom thresholds and layer masks.
- `PendingInput`: per-frame event payload ingress.
- `RuntimeOutput`: built output snapshot for adapters.

### Components
- `Rider`: world position, heading, speed.
- `Camera`: camera controller state.
- `InteractionState`: aggregated pan/pinch/rotate deltas.
- `FollowLock`: lock-state snapshot for diagnostics/parity checks.
- `MapQuery`: current query bounds + zoom bucket + LOD mask.

### Input Event Types
- `GpsFixEvent`
- `GestureEvent` (`Pan`, `Pinch`, `Rotate`)
- `TapEvent`
- `NorthUpRequest`

## Deterministic Schedule Order
1. `InputIngestSet`
2. `MotionFusionSet`
3. `CameraPolicySet`
4. `MapQuerySet`
5. `OutputBuildSet`

## Runtime I/O Contract
- Input: `RuntimeInputFrame` (dt + optional GPS + gesture list + tap + north-up request).
- Output: `RuntimeFrameOutput` containing:
  - `CameraView`
  - camera mode
  - map query spec
  - visible line buffer filtered by bbox + LOD.

## Testing Strategy
- Unit/system tests in `runtime-core` validate:
  - moving/stopped mode transitions,
  - pan-lock stability behavior,
  - LOD filtering behavior.
- Runtime replay harness validates deterministic outputs for repeated traces.
- Adapter parity tests compare output time-series between native and wasm adapters on shared traces.
- Profiling tests (ignored by default) cover representative map-load performance and hot-path allocation pressure:
  - `cargo test -p runtime-core -- --ignored`

## Implementation Notes
- Platform adapters must not own camera policy decisions.
- New behavior (overview/declutter, route overlays, additional sensor fusion) should enter as ECS systems/resources in `runtime-core`.
- `render-core` should continue converging toward pure render-only responsibilities.
