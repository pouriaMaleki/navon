# Framework Execution Guide

Spec reference: [`project-spec.md`](./project-spec.md)
Plan reference: [`current-plan.md`](./current-plan.md)

## Goal
Lead the project as a framework-first Rust buildout where shared runtime behavior is implemented once and consumed by both firmware and wasm adapters.

## Leadership Approach
1. Make architecture constraints executable early.
2. Prefer small stable public contracts over broad convenience APIs.
3. Keep runtime behavior in shared Rust, not in adapter glue.
4. Validate structure and behavior continuously so future features do not erode boundaries.
5. Deliver in short milestones with hard exit criteria.

## Where To Start
1. Restore the declared workspace crate graph:
   - `runtime-core`
   - `render-core`
   - `render-core-wasm`
2. Make the workspace compile before implementing full product behavior.
3. Define the core public contracts:
   - `RuntimeInputFrame`
   - `TouchContact`
   - `TouchContactFrame`
   - `RuntimeFrameOutput`
   - `RuntimeConfig`
   - `MapQuerySpec`
   - `DiagnosticsSnapshot`
   - map-source and map-query traits
4. Build the deterministic frame schedule skeleton:
   - input ingest and shared contact interpretation
   - motion fusion
   - camera policy
   - map query
   - output build
5. Add scenario tests before deep adapter integration.

## Recommended Project Sequence
### Milestone 1: Skeleton
- Create the missing crates and module skeletons.
- Keep the code compiling with stubbed internals.
- Freeze ownership boundaries before feature expansion.

### Milestone 2: Runtime Behavior
- Move shared touch/contact interpretation, motion estimation, and camera policy into `runtime-core`.
- Keep the runtime deterministic from ordered input frames.
- Implement follow-lock, pan, pinch, rotate, tap handling, recenter, and compass interaction policy in shared Rust from normalized contact inputs.

### Milestone 3: Rendering Integration
- Route `RuntimeFrameOutput` and queried geometry into `render-core`.
- Keep rendering stateless and pure.
- Keep `RuntimeFrameOutput` limited to camera/query/overlay state; do not embed geometry buffers in it.
- Ensure coarse bbox + LOD lookup lives behind `MapSource` implementations while final visibility/clipping and styling stay in `render-core`.

### Milestone 4: Adapter Hookup
- Make firmware feed GPS and normalized touch contact frames into `RuntimeInputFrame`.
- Make wasm feed browser/emulator inputs as the same normalized touch contact frames into `RuntimeInputFrame`.
- Route both adapters through the same shared map query backend and stateless renderer.
- Keep both adapters thin and behavior-free.

### Milestone 5: Hardening
- Add diagnostics and regression fixtures.
- Prepare the next shared `.svm` loading path beyond the current embedded `map-runtime` bridge.
- Add performance checks and parity validation between targets.

## What To Validate At Each Step
### Structural Validation
- Dependency direction is correct.
- `runtime-core` owns behavior and state.
- `render-core` owns pure rendering only.
- `MapSource` implementations own coarse data lookup only.
- Adapters do not own camera policy, LOD logic, gesture recognition, or product-control hit testing.
- `RuntimeFrameOutput` carries query intent and overlay state, not queried geometry buffers.
- Public APIs stay small and adapter-safe.

### Behavioral Validation
- Ordered input frames produce expected runtime outputs.
- Identical normalized contact-frame fixtures resolve to the same gesture/tap semantics.
- Scenario tests cover transitions and edge cases:
  - ride to stop
  - stop to ride
  - pan to idle to recenter
  - compass preview, lock, unlock, and acknowledgement
  - zoom bucket changes

### Parity Validation
- The same input sequence should produce the same runtime output semantics for wasm and firmware paths.
- Adapter differences must stop at hardware/browser I/O and output presentation.

### Operational Validation
- `cargo check` at the workspace level.
- Unit tests for pure math and policy logic.
- Scenario tests for runtime behavior.
- Emulator web lint/typecheck/build when wasm-facing code changes.

## Invariant-Driven Implementation Rules
1. Use a single source of truth for behavior-critical semantics.
   - Do not let timing come from one representation and classification or labels come from another unless there is an explicit validation or canonicalization step.
   - If multiple representations exist, document which one is authoritative before editing logic.
2. Prefer derived state over duplicated state.
   - When direction, classification, or progress can be derived from geometry or canonical runtime state, derive it instead of hand-maintaining duplicate labels.
3. Design state machines for re-entry, not only first activation.
   - Any `pending`, `applied`, `active`, or `locked` flag must have explicit reset conditions and tested re-entry behavior.
4. Hold demo and test fixtures to production-like invariants.
   - Demo routes, emulator helpers, and fixtures must satisfy the same invariants as shared-core inputs.
   - Fixture builders should fail fast when geometry, metadata, or state assumptions drift.
5. Define ordered-geometry progression rules explicitly.
   - When rejoining, snapping, or projecting onto ordered geometry, choose whether behavior prefers nearest, previous, or next progress and add edge tests near boundaries.
6. Close every bugfix with an executable guard.
   - Any bugfix that changes logic must add or extend at least one regression test for the failure mode.

## Regression Test Patterns
Use the smallest test that can lock the invariant:
- Contract tests: schema, normalization, and canonical fixture validity.
- State-machine regression tests: repeated activation, reset, and re-entry behavior.
- Fixture-validity tests: fail fast when demo or bridge data drifts from shared-core expectations.
- End-to-end scenario tests: cross-subsystem behaviors such as reroute loops, alert transitions, and bridge orchestration.

Keep tests colocated with the subsystem they protect. Prefer deterministic scenario inputs over manual visual verification whenever behavior depends on ordering, timing, or progression thresholds.

## Architecture Review Rules
Every implementation PR should be checked against these rules:
1. Does this logic live in the correct crate?
2. Does it expand an existing extension point instead of bypassing the design?
3. Is any adapter starting to own product behavior?
4. Is a new public type actually required?
5. Can this logic be tested without browser or device APIs?
6. Would wasm and firmware still behave the same after this change?
7. Does runtime output expose query intent instead of geometry buffers?

## Extension Points
New functionality should enter through one of these paths:
- a new input event type
- a new config/resource field
- a new ECS system within the frame schedule
- a new map metadata or LOD rule
- a new render overlay primitive

## Anti-Patterns To Reject
- Adapter-local camera state machines
- Adapter-local gesture recognition or control hit testing
- Product policy inside TypeScript or board code
- Monolithic shared state structs mixing motion, camera, input, and rendering
- Public APIs that expose ECS internals
- Features that require edits across all crates to land one behavior

## Immediate Next Actions
1. Wire actual ESP-IDF peripheral acquisition and live hardware handles into the firmware provider layer without moving behavior out of shared Rust.
2. Replace the remaining device-side `xtask` placeholders once bundling and deploy flows have real implementations behind them.
3. Profile the shared embedded `.svm` bridge so later direct loading work is driven by measured cost rather than guesswork.
4. Design the next direct `.svm` runtime loading path after the embedded bridge is stable enough to compare against.
