# Source Tree Guide

Spec reference: [`project-spec.md`](./project-spec.md)

## Purpose
Define the intended file structure for the main project and make crate/module ownership explicit before implementation expands.

## Top-Level Layout
```text
/work
├── Cargo.toml
├── README.md
├── docs/
├── map-runtime/
├── runtime-core/
├── render-core/
├── render-core-wasm/
├── firmware/
├── map-vector-cli/
├── xtask/
├── map-src/
├── map-data/
└── emulator/
```

## Top-Level Ownership
- `docs/`: product, architecture, planning, and workflow documentation.
- `map-runtime/`: shared embedded `.svm` reader and coarse map query backend.
- `runtime-core/`: shared runtime behavior and deterministic ECS frame pipeline.
- `render-core/`: pure rendering math, visibility, styling, overlays, and raster output.
- `render-core-wasm/`: wasm bindings and browser/emulator adapter over shared Rust crates.
- `firmware/`: ESP32 board integration, drivers, device loop, and framebuffer presentation.
- `map-vector-cli/`: offline map conversion into `.svm`.
- `xtask/`: project automation commands.
- `map-src/`: source map inputs.
- `map-data/`: converted runtime map outputs.
- `emulator/`: browser-based simulator shell and docs.

## Recommended `runtime-core` Structure
```text
runtime-core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── api/
    │   ├── mod.rs
    │   ├── config.rs
    │   ├── input.rs
    │   ├── output.rs
    │   ├── query.rs
    │   ├── events.rs
    │   └── diagnostics.rs
    ├── schedule/
    │   ├── mod.rs
    │   ├── sets.rs
    │   └── runner.rs
    ├── input/
    │   ├── mod.rs
    │   ├── staging.rs
    │   ├── contacts.rs
    │   ├── gestures.rs
    │   └── taps.rs
    ├── motion/
    │   ├── mod.rs
    │   ├── gps.rs
    │   ├── speed.rs
    │   ├── heading.rs
    │   └── confidence.rs
    ├── camera/
    │   ├── mod.rs
    │   ├── state.rs
    │   ├── riding.rs
    │   ├── stopped.rs
    │   ├── pan.rs
    │   ├── zoom.rs
    │   ├── rotate.rs
    │   ├── recenter.rs
    │   ├── follow_lock.rs
    │   └── north_up.rs
    ├── map/
    │   ├── mod.rs
    │   ├── projection.rs
    │   ├── query.rs
    │   ├── lod.rs
    │   └── source.rs
    ├── output/
    │   ├── mod.rs
    │   └── snapshot.rs
    ├── diagnostics/
    │   ├── mod.rs
    │   ├── counters.rs
    │   └── snapshot.rs
    ├── ecs/
    │   ├── mod.rs
    │   ├── components.rs
    │   ├── resources.rs
    │   └── world.rs
    └── tests/
        ├── ride_stop_transition.rs
        ├── pan_recenter.rs
        ├── north_up_override.rs
        └── lod_selection.rs
```

## `runtime-core` Ownership
- `api/`: only public adapter-facing types.
- `schedule/`: deterministic system ordering and frame stepping.
- `input/`: adapter-supplied touch contact validation, shared gesture/tap recognition, and per-frame staging.
- `motion/`: GPS sample handling, speed estimation, travel-heading filtering, motion confidence.
- `camera/`: state machine and interaction policy.
- `map/`: visible bounds, zoom bucket selection, LOD policy, source/query traits.
- `output/`: adapter-ready runtime snapshot.
- `diagnostics/`: internal counters/state history and construction of exported `DiagnosticsSnapshot` values.
- `ecs/`: internal ECS plumbing only; not part of the public API.

## Recommended `render-core` Structure
```text
render-core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── math/
    │   ├── mod.rs
    │   ├── vec2.rs
    │   ├── bounds.rs
    │   ├── angle.rs
    │   └── interpolation.rs
    ├── camera_view.rs
    ├── visibility/
    │   ├── mod.rs
    │   ├── cull.rs
    │   └── clip.rs
    ├── style/
    │   ├── mod.rs
    │   ├── palette.rs
    │   ├── roads.rs
    │   └── markers.rs
    ├── overlay/
    │   ├── mod.rs
    │   ├── rider.rs
    │   └── north_indicator.rs
    ├── raster/
    │   ├── mod.rs
    │   ├── framebuffer.rs
    │   ├── lines.rs
    │   └── circles.rs
    └── tests/
        ├── projection.rs
        ├── clipping.rs
        └── marker_render.rs
```

## `render-core` Ownership
- No long-lived product state.
- Input: `CameraView`, queried world geometry candidates, style inputs, and overlay inputs.
- Output: deterministic framebuffer or draw-ready pixel data.
- Owns final screen-space visibility/culling/clipping after coarse map queries have already run.
- Best location for projection math, clipping, styling, and marker drawing.

## Recommended `map-runtime` Structure
```text
map-runtime/
├── Cargo.toml
└── src/
    └── lib.rs
```

## `map-runtime` Ownership
- Embedded `.svm` bytes access and coarse bbox/LOD candidate lookup.
- Shared spatial indexing/query code that can be reused by firmware and wasm.
- No camera policy, render policy, or platform I/O.

## Recommended `render-core-wasm` Structure
```text
render-core-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── bindings.rs
    ├── adapter.rs
    ├── input_bridge.rs
    ├── output_bridge.rs
    └── panic_hook.rs
```

## `render-core-wasm` Ownership
- `wasm-bindgen` exports.
- JS-to-Rust input translation into shared normalized input contracts.
- Wiring into the shared `map-runtime` query backend for the current emulator slice.
- Rust-to-JS buffer/output translation.
- No product camera policy.

## Recommended `firmware` Structure
```text
firmware/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── main.rs
    ├── app.rs
    ├── board_config.rs
    ├── display.rs
    ├── esp_idf.rs
    ├── framebuffer.rs
    ├── gps.rs
    ├── touch.rs
    ├── input_bridge.rs
    ├── map_source.rs
    ├── platform.rs
    ├── power.rs
    └── logging.rs
```

## `firmware` Ownership
- `lib.rs`: firmware library surface shared by tests and the device entrypoint.
- `main.rs`: startup and main device loop.
- `board_config.rs`: pins, panel geometry, touch reset/interrupt wiring.
- `gps.rs`: hardware GPS acquisition.
- `touch.rs`: raw touch controller interaction and coordinate normalization into shared touch-contact samples.
- `input_bridge.rs`: convert device inputs into `RuntimeInputFrame` with `TouchContactFrame`.
- `display.rs` and `framebuffer.rs`: present render output on device.
- `esp_idf.rs`: concrete firmware-side provider implementations and low-level shims for GT9271 transport, panel upload, GPS serial parsing, and device-platform assembly.
- `map_source.rs`: device-side bridge into the shared `map-runtime` query backend.
- `platform.rs`: frame timing plus device input/output orchestration around the shared runtime/query/render loop.
- Any temporary firmware-local touch math must stop at controller cleanup or de-jittering; it must not classify app-level pan/pinch/rotate/tap semantics.

## Public API Rule
The main stable adapter-facing types should live in `runtime-core/src/api/`:
- `config.rs`
- `input.rs` (`RuntimeInputFrame`, `TouchContact`, `TouchContactFrame`)
- `output.rs`
- `query.rs`
- `diagnostics.rs`

Adapters should not need direct knowledge of:
- ECS components
- ECS resources
- system sets
- internal runtime scheduling details

## Query Handoff Rule
- `runtime-core` emits `MapQuerySpec`.
- A `MapSource` implementation performs coarse bbox + LOD lookup and returns world-space geometry candidates.
- `render-core` performs final screen-space visibility/clipping/rasterization from `CameraView` plus queried geometry.
- Adapters orchestrate these calls but do not own query policy or visibility logic.

## Placement Rules
- If code changes bike behavior, it belongs in `runtime-core`.
- If code performs coarse world-geometry lookup from `MapQuerySpec`, it belongs in a `MapSource` implementation.
- If code draws pixels, it belongs in `render-core`.
- If code talks to browser or device APIs, it belongs in an adapter crate.
- If code converts source map formats, it belongs in `map-vector-cli`.

## Boundaries That Must Hold
Do not place these in adapters:
- riding/stopped transitions
- follow-lock and recenter policy
- heading smoothing
- gesture recognition and tap/control hit testing
- north-up override rules
- zoom bucket and LOD policy

These belong in shared Rust runtime code.
