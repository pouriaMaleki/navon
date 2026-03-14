# Main TODO

Spec reference: [`project-spec.md`](./project-spec.md)
Plan reference: [`current-plan.md`](./current-plan.md)

## Framework Bootstrap
- [x] Create `runtime-core` crate with `api`, `schedule`, `input`, `motion`, `camera`, `map`, `output`, and `diagnostics` modules.
- [x] Create `render-core` crate with pure `math`, `camera_view`, `visibility`, `style`, `raster`, and `overlay` modules.
- [x] Create `render-core-wasm` crate as a thin wasm adapter over shared Rust runtime/render crates.
- [x] Restore a valid workspace build for the declared crate graph in the root `Cargo.toml`.

## Public Contracts
- [x] Define `RuntimeInputFrame`, `TouchContact`, `TouchContactFrame`, `RuntimeFrameOutput`, `RuntimeConfig`, `MapQuerySpec`, and runtime diagnostics snapshot types.
- [x] Define map query and map-source traits so runtime policy can stay independent from generated Rust maps vs future direct `.svm` loading.
- [x] Keep `RuntimeFrameOutput` limited to camera/query/overlay state and diagnostics rather than queried geometry buffers.
- [x] Keep adapter APIs free of `bevy_ecs` types and world mutation details.

## Runtime Core
- [x] Implement shared contact-frame ingestion plus gesture and tap derivation in `runtime-core::input`.
- [x] Implement a deterministic runtime runner skeleton with ordered schedule stages and stable output assembly.
- [x] Implement deterministic ECS schedule sets: input ingest, motion fusion, camera policy, map query, output build.
- [x] Replace the temporary hand-rolled runner with an internal `bevy_ecs`-backed schedule so implementation matches the architecture docs.
- [x] Fix rotated `MapQuerySpec` coverage for heading-up cameras.
- [x] Fix zoom lower-bound handling so the configured minimum is reachable.
- [x] Fix north-up output semantics so riding north is not mislabeled as north-up mode.
- [x] Make motion state resilient to brief GPS dropouts instead of forcing an immediate stopped transition.
- [x] Add rider motion confidence and filtered travel-heading estimation from GPS deltas.
- [x] Implement riding/stopped camera state transitions with smooth north-up settle on stop.
- [x] Implement pan, pinch, rotate, north-indicator override, and smooth auto-recenter behavior.
- [x] Implement follow-lock behavior so manual pan preserves map-relative rider position until recenter.
- [x] Implement zoom bucket and LOD mask selection in runtime-owned policy.

## Render Core
- [x] Move or establish shared camera-view math and heading-up projection in `render-core`.
- [x] Implement final screen-space visibility/clipping against runtime-provided camera view over queried geometry candidates.
- [x] Define vector styling primitives for major/minor road hierarchy and rider marker overlays.
- [x] Keep framebuffer generation deterministic for identical inputs across targets.

## Adapter Integration
- [x] Add firmware bridge helpers that convert raw GPS/touch samples into `RuntimeInputFrame`.
- [x] Add wasm bridge helpers that construct `RuntimeInputFrame` values and translate `RuntimeFrameOutput` into a JS-facing snapshot.
- [x] Wire emulator wasm bridge to consume `RuntimeFrameOutput` rather than product logic in TypeScript.
- [x] Wire firmware input translation for GPS and normalized touch contact frames into `RuntimeInputFrame`.
- [x] Wire browser/emulator input translation to emit the same normalized touch contact frames into `RuntimeInputFrame`.
- [x] Keep device-specific touch drivers and browser event capture outside shared runtime logic while moving gesture and tap semantics into shared Rust.
- [x] Make `cargo xtask emu` rebuild wasm and start the emulator dev server from the repository root.
- [x] Wire a firmware host-side frame loop that steps `runtime-core`, queries map geometry, renders through `render-core`, and presents into a device-facing framebuffer abstraction.

## Map Data Evolution
- [x] Implement an embedded `.svm` bridge that answers `MapSource::query(&MapQuerySpec)` with coarse bbox + LOD candidate selection.
- [x] Reuse the same query backend for firmware and wasm through a shared `map-runtime` crate.
- [ ] Design the next direct `.svm` runtime loading layer without coupling file parsing to camera/render policy.
- [ ] Extend map metadata only through runtime/query contracts needed for future LOD and overlays.

## Validation
- [x] Add focused contract tests for touch validation, stopped defaults, riding transition, and map-query bounds.
- [x] Add regression tests for rotated query coverage, zoom lower bound, north-up semantics, and GPS-dropout resilience.
- [x] Add unit tests for motion filtering, heading smoothing, zoom bounds, and camera interpolation.
- [x] Add query/render tests for bbox selection, edge-touching geometry, projection, clipping, and deterministic framebuffer output.
- [ ] Add parity fixtures proving identical `TouchContactFrame` sequences resolve to the same gestures/taps across firmware and wasm paths.
- [x] Add scenario tests for ride, stop, pan, recenter, and north-up override sequences.
- [ ] Add parity-oriented fixtures that can be reused by wasm and firmware adapters and their shared map/query/render loop.
- [x] Run workspace validation commands once the missing crates exist and are wired.
- [x] Run emulator wasm build plus web lint/typecheck/build after the frame-driven bridge lands.
- [x] Run `cargo xtask emu` startup sanity check after wiring the real command.

## Documentation
- [x] Update main spec with framework foundation rules and delivery order.
- [x] Add main `current-plan.md`.
- [x] Add main `todo.md`.
- [x] Add framework execution and validation guide.
- [x] Add source tree and ownership guide.
- [x] Reconcile linked architecture docs to one ownership and query/render handoff model.
- [x] Reconcile README crate descriptions with the implemented crate layout once bootstrap begins.
- [x] Document the shared `map-runtime` crate and firmware host-side slice in the canonical references.
