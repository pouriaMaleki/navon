# Current Plan

## Phase 4 Plan (Riding Mode + Visual Upgrade)
1. Camera and mode state model
- Add explicit camera mode state: `Riding`, `StoppedNorthUp`, `TemporaryNorthUp`.
- Define movement detection thresholds and dwell timers for mode transitions.
- Add smooth interpolation for heading and camera anchor transitions.

2. Zoom policy and scale limits
- Define max zoom-in target to show roughly 100 m around rider.
- Clamp zoom-out before vector clutter destroys readability.
- Reserve a separate future overview mode where vector density is intentionally reduced.

3. Riding/stopped behavior
- Riding: heading-up orientation and lower-quarter rider anchor.
- Stopped: centered rider and delayed smooth return to north-up.
- Temporary north-up override from indicator tap with auto-return to riding mode while moving.
- During manual pan, freeze follow target at pan start so rider marker stays screen-stable while camera offset changes.

4. UI affordances and map styling
- Add north indicator icon in top-right and interaction handling.
- Redesign rider marker:
  - Riding marker: glowing yellow-green forward shape.
  - Stopped marker: larger, game-style presence.
- Update vector palette/style toward dark map background and high-contrast roads.

5. Performance + render quality pass
- Precompute per-frame camera transform constants.
- Add viewport line clipping before raster stepping.
- Add adaptive rendering quality during active pan.
- Define follow-up index/windowing work for large vector sets.

6. Validation and reconciliation
- Validate behavior on desktop/mobile emulator gestures.
- Confirm docs/spec alignment across root and emulator modules.
- Keep converter/runtime boundaries unchanged.

7. Bike map data profile
- Keep bike-relevant streets/paths and exclude ferry/boat/water transport lanes.
- Ensure `xtask prepare-map` uses bike profile defaults for generated runtime map data.

## Phase 5 Plan (CVE Tracking + Security Automation)
1. Security policy and ownership
- Define CVE triage SLOs and ownership model in repository docs.
- Confirm component coverage for Rust workspace, emulator web, and CI dependencies.

2. Dependency alerting automation
- Configure Dependabot for Cargo, npm, and GitHub Actions ecosystems.
- Enable security updates and routine version update cadence.

3. PR/scheduled vulnerability scanning
- Add GitHub Actions workflow for `cargo audit` and `npm audit`.
- Run on pull requests and scheduled cadence to catch newly published CVEs.

4. Static analysis security checks
- Add CodeQL workflow for Rust and JavaScript/TypeScript.
- Ensure findings surface in GitHub Security tab.

5. Notification and triage workflow
- Route alerts to maintainers via GitHub notifications and review ownership.
- Record accepted-risk exceptions with explicit expiry and revisit date.

## Phase 6 Plan (Real Device Touch Integration)
1. Board config and driver bring-up
- Add Waveshare board touch config with `GT9271`, display size, I2C bus pins, and touch control pins.
- Implement low-level touch reset/read path in firmware.

2. Gesture recognition layer
- Convert raw contacts into pan, pinch, rotate, and tap events.
- Preserve gesture semantics already proven in emulator.

3. Firmware runtime wiring
- Feed gestures into `BikeMinimapState` instead of mock touch.
- Keep camera logic inside shared Rust core.

4. Hardware validation
- Validate coordinate mapping, north-indicator hit testing, and gesture stability on the real board.

## Phase 7 Plan (ECS Runtime Refactor, Design 1)
1. Runtime-core introduction
- Add `runtime-core` crate with `bevy_ecs` schedule, resources, components, and event-style input frame.
- Move runtime orchestration ownership out of firmware/wasm adapters.

2. Camera and interaction policy migration
- Keep existing behavior parity (riding/stopped/temp-north-up, pan recenter, follow lock).
- Execute camera update as deterministic ECS system set.

3. Visibility and LOD foundation
- Add runtime map query resource (bbox + zoom bucket + LOD mask).
- Add map-source query abstraction with zoom-dependent filtering support.

4. Adapter wiring
- Firmware uses `runtime-core::Runtime` as thin orchestrator.
- WASM bridge uses `runtime-core::Runtime` for parity with firmware behavior.

5. Cleanup and hardening
- Add replay/parity tests and document architecture boundaries.
- Follow with phased rendering-core modularization and performance tuning.

## TODO
- [x] Redesign shared camera rotation model using filtered travel heading and correct map-heading sign.
- [x] Derive riding-mode camera heading from movement vector in shared Rust controller.
- [x] Add camera mode state machine and transition timers in shared runtime model.
- [x] Add movement detection and delayed stop transition logic.
- [x] Implement lower-quarter rider anchor for riding mode.
- [x] Implement smooth stop-to-north-up recenter animation.
- [x] Add temporary north-up override and auto-return policy.
- [x] Add top-right north indicator icon and tap interaction.
- [x] Keep rider marker screen-stable during manual pan by locking follow target to pan-start position.
- [x] Clamp zoom-in to approximately 100 m visible context target.
- [x] Clamp zoom-out to readability-safe limit (pre-overview mode).
- [x] Implement riding marker glow + directional shape.
- [x] Implement larger stopped marker style.
- [x] Add dark-theme vector style presets (major/minor hierarchy).
- [x] Implement renderer optimizations: precomputed transform + clipping + pan-time quality mode.
- [x] Define overview-mode design doc for future vector suppression strategy.
- [x] Add bike-focused map conversion profile and wire `xtask prepare-map` to use it.
- [x] Add repository CVE/security tracking plan document (`docs/cve-tracking-plan.md`).
- [x] Add Dependabot configuration for Cargo, npm, and GitHub Actions.
- [x] Add GitHub Actions workflow for `cargo audit` + `npm audit`.
- [x] Add CodeQL workflow for Rust and JavaScript/TypeScript.
- [x] Add security ownership + triage policy docs (`CODEOWNERS` + `SECURITY.md` + risk-acceptance template).
- [x] Enforce security audit checks by default (`SECURITY_ENFORCE=true` default in workflow).
- [ ] Enable branch protection to require Security Audit and CodeQL checks.
- [ ] Enable secret scanning and push protection in GitHub repository settings.
- [x] Add Waveshare board touch config module for GT9271 + display constants.
- [x] Implement GT9271 bus-only detect/read path in firmware.
- [x] Add firmware gesture recognizer for pan, pinch, rotate, and tap.
- [x] Add `apply_rotate_gesture(...)` to firmware minimap state.
- [x] Replace mock touch input in `firmware/src/bin/main.rs` with real touchscreen input polling path.
- [ ] Validate touch coordinate mapping and north-indicator taps on real hardware.
- [x] Create `runtime-core` crate and wire workspace membership.
- [x] Add `bevy_ecs` dependency (pinned) and baseline no-`std` runtime types.
- [x] Define runtime input/output structs and public API (`new/step/reset`).
- [x] Implement ECS schedule sets and deterministic ordering.
- [x] Port camera mode logic into ECS runtime orchestration path.
- [x] Port pan-follow lock and recenter behavior into ECS runtime orchestration path.
- [x] Add map query resource and zoom-bucket LOD mask policy.
- [x] Update firmware adapter to consume `runtime-core` output.
- [x] Update wasm adapter to consume `runtime-core` output.
- [x] Add initial runtime-core unit tests for transition/LOD/pan-lock behavior.
- [x] Add runtime trace replay test harness (move/stop/pan/pinch/rotate/tap).
- [x] Add firmware-vs-wasm parity tests for mode/heading/anchor progression.
- [x] Split `render-core` into internal modules (`raster`, `style`, `visibility`, `math`) while keeping API compatibility.
- [x] Add perf sanity checks and allocation profile review for ECS runtime.
- [x] Update architecture docs for ECS design and ownership boundaries.
