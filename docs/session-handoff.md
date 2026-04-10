# Session Handoff

## Current Goal
Finish the share-import refactor by removing stale partner-integration references from shared contracts and docs, then validate the remaining native import wiring.

## Product Direction
- Home is the only primary surface.
- Settings is secondary.
- No partner account integrations.
- Share/import is the inbound path.
- GPX is first-class.
- Unknown or unsupported imports must go to `Import Diagnostics`.

## Current Status
- Docs updated toward the share-import product shape.
- Native code already implements the main iOS and Android share-import flows.
- Shared Rust provider contract no longer carries Google/Garmin-specific provider variants.
- Branch is dirty.
- No commit pushed from this cleanup pass yet.

## Implemented In This Branch
- iOS:
  - Share extension target and app-group queue added.
  - Main app consumes pending shared imports on launch/foreground.
  - Settings now exposes `Import Diagnostics` instead of `Connections`.
- Android:
  - `ACTION_SEND`, `ACTION_SEND_MULTIPLE`, and `ACTION_VIEW` import handling added.
  - Shared imports can fast-path into Home preview or fall back into `Import Diagnostics`.
  - Settings now exposes `Import Diagnostics` instead of `Connections`.
- Shared/runtime/docs:
  - Removed stale Google/Garmin provider variants from shared `RouteProvider`.
  - Updated companion docs and workspace guidance away from partner-integration language.

## Partial / In-Progress Work
- `companion-ios/project.yml`: share extension target exists but still needs native-toolchain verification on macOS/Xcode.
- `companion-ios/CompanionShareExtension/ShareViewController.swift`: broad `TRUEPREDICATE` activation rule may need tightening once real payload coverage is confirmed.
- `companion-android/app/src/main/java/me/fiksu/esp32map/companion/integration/share/AndroidShareImportParser.kt`: intent parsing exists but still needs runtime verification against real share sources.

## Known Risks
- Old payloads carrying `google_ingest` / `garmin_*` provenance now decode as `Unknown(...)` rather than first-class provider variants.
- iOS share extension embedding/signing has not been verified on a Mac yet.
- Android share-intent behavior has not been runtime-tested with Google Maps and file-manager apps yet.

## Next Steps
1. Run formatting and targeted validation.
2. Verify iOS share extension build on macOS/Xcode.
3. Verify Android share-intent flows with real sources.
4. Commit and push once validation is stable.

## Validation
Passed:
- `git diff --check`
- `cargo test -p runtime-core -p render-core-wasm -p firmware`

Not verified:
- iOS build on Mac
- Android runtime on device/emulator share flows
- Android Gradle unit tests in this environment (`JAVA_HOME` unset; no `java` on `PATH`)

## Environment Notes
- Local shell works normally in `/work`.
- Network access is restricted in the agent sandbox.
- Android Gradle validation is blocked here until a JDK is available.

## Files To Inspect First
- `docs/current-plan.md`
- `runtime-core/src/api/route.rs`
- `companion-ios/project.yml`
- `companion-android/app/src/main/java/me/fiksu/esp32map/companion/integration/share/AndroidShareImportParser.kt`

## Commands To Resume
```bash
git status --short
rg -n "Connections|Strava|Garmin|Komoot|partner|google_ingest|garmin_api|garmin_file" .
git diff --check
