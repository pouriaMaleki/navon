# Runtime ECS Architecture

## Decision
- Architecture: **Design 1 (ECS runtime core)**.
- Framework: `bevy_ecs` (`0.17.2` on Rust 1.88 toolchain).

## Crate Graph
- `runtime-core`: owns runtime state/policy and map query/LOD decisions.
- `render-core`: stateless renderer primitives (`CameraView` + queried world geometry -> framebuffer).
- `firmware`: ESP32 adapter for touch/GPS input and framebuffer presentation.
- `render-core-wasm`: wasm adapter for browser/emulator input/output plus the current embedded `.svm` query bridge.

## ECS Model
### Resources
- `FrameTime`: deterministic frame clock (`dt`, `total`, `tick`).
- `RuntimeConfig`: viewport, anchor defaults, zoom bounds, and resilience thresholds.
- `PendingInput`: per-frame adapter sample ingress before shared contact interpretation.
- `DerivedInputState`: per-frame shared gesture/tap output built from normalized contacts.
- `MotionState`: latest GPS-derived world focus, motion state, heading, and GPS-gap tracking.
- `CameraState`: camera controller snapshot built in shared Rust.
- `MapQuerySpec`: current query bounds + zoom bucket + LOD mask.
- `RuntimeOutput`: built output snapshot for adapters.

### Components
- The current minimal ECS implementation keeps frame-global state in resources.
- Entity/component state should be introduced once shared gesture state, follow-lock, and richer interaction lifecycles need per-domain isolation.
- Planned component domains remain:
  - `Rider`
  - `Camera`
  - `InteractionState`
  - `FollowLock`
  - `MapQuery`

### Public Input Samples
- `TouchContact`
- `TouchContactFrame`
- optional GPS samples

### Internal Derived Events
- `GestureEvent` (`Pan`, `Pinch`, `Rotate`)
- `TapEvent`
- `NorthUpRequest` derived from shared tap/control-hit handling or synthetic tests

## Deterministic Schedule Order
1. `InputIngestSet`
2. `MotionFusionSet`
3. `CameraPolicySet`
4. `MapQuerySet`
5. `OutputBuildSet`

## Runtime I/O Contract
- Input: `RuntimeInputFrame` (dt + optional GPS + optional `TouchContactFrame` and other shared normalized samples). Adapters do not emit product gesture or control-hit semantics.
- Output: `RuntimeFrameOutput` containing:
  - camera state snapshot
  - `MapQuerySpec`
  - overlay/style state
  - optional `DiagnosticsSnapshot`
- `RuntimeFrameOutput` does not contain queried geometry buffers.

## Current Foundation Guarantees
- Internal execution uses a fixed `bevy_ecs` schedule order that matches the declared frame stages.
- Shared Rust derives one-finger pan, two-finger pinch/rotate, and tap semantics from `TouchContactFrame`.
- Follow-lock, auto-recenter, and north-up override live in shared runtime camera policy.
- Query bounds account for camera rotation so heading-up views do not under-query corners.
- Zoom clamping preserves the configured minimum and maximum range.
- Brief GPS gaps preserve prior motion state until the configured timeout elapses.
- The current wasm slice now steps runtime, queries embedded `.svm` geometry, and renders a deterministic framebuffer through shared Rust only.

## Query and Render Handoff
1. `runtime-core` builds `MapQuerySpec` from camera state and LOD policy.
2. A `MapSource` implementation performs coarse bbox + LOD candidate lookup.
3. `render-core` applies final screen-space visibility/clipping and rasterizes the queried geometry.
4. Firmware and wasm adapters orchestrate the calls only; they do not own query policy or camera behavior.
