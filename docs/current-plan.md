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

## TODO
- [ ] Add camera mode state machine and transition timers in shared runtime model.
- [ ] Add movement detection and delayed stop transition logic.
- [ ] Implement lower-quarter rider anchor for riding mode.
- [ ] Implement smooth stop-to-north-up recenter animation.
- [ ] Add temporary north-up override and auto-return policy.
- [ ] Add top-right north indicator icon and tap interaction.
- [ ] Clamp zoom-in to approximately 100 m visible context target.
- [ ] Clamp zoom-out to readability-safe limit (pre-overview mode).
- [ ] Implement riding marker glow + directional shape.
- [ ] Implement larger stopped marker style.
- [ ] Add dark-theme vector style presets (major/minor hierarchy).
- [ ] Implement renderer optimizations: precomputed transform + clipping + pan-time quality mode.
- [ ] Define overview-mode design doc for future vector suppression strategy.
- [x] Add bike-focused map conversion profile and wire `xtask prepare-map` to use it.
- [x] Add repository CVE/security tracking plan document (`docs/cve-tracking-plan.md`).
- [x] Add Dependabot configuration for Cargo, npm, and GitHub Actions.
- [x] Add GitHub Actions workflow for `cargo audit` + `npm audit`.
- [x] Add CodeQL workflow for Rust and JavaScript/TypeScript.
- [x] Add security ownership + triage policy docs (`CODEOWNERS` + `SECURITY.md` + risk-acceptance template).
- [x] Enforce security audit checks by default (`SECURITY_ENFORCE=true` default in workflow).
- [ ] Enable branch protection to require Security Audit and CodeQL checks.
- [ ] Enable secret scanning and push protection in GitHub repository settings.
