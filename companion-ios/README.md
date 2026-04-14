# Companion iOS

Native iOS companion app scaffold for route planning, BLE sync, and reroute orchestration.

## Structure
- `CompanionApp/App`: app entry and shared app model
- `CompanionApp/Presentation`: SwiftUI screens and shell
- `CompanionApp/Domain`: route/session/provider models and protocols
- `CompanionApp/Integration`: HSL, BLE, persistence, and diagnostics integrations

## Build
This scaffold uses XcodeGen so the repo can keep the project definition in text.

```bash
brew install xcodegen
cd companion-ios
cp Config/Signing.local.example.xcconfig Config/Signing.local.xcconfig
# Edit Config/Signing.local.xcconfig and set your Apple Development Team ID.
xcodegen generate
open ESP32MapCompanion.xcodeproj
```

`Config/Signing.local.xcconfig` is ignored by git. After you set it once, `git pull` and `xcodegen generate` should no longer clear the local signing team in Xcode.

## Current State
- Full app shell is scaffolded.
- HSL planning can run in live Digitransit mode when a subscription key is configured, with explicit sample fallback when live routing is unavailable.
- GPX routes can be imported through the iOS document picker and flow through the same preview and sync path as provider-backed routes.
- Route alternatives can be selected before sync, and reroute publishing can be triggered from an editable rider location in the ride/device surfaces.
- BLE transport now uses real CoreBluetooth packet IO when an ESP32 route-sync peripheral is available, while preserving the simulated fallback path when no BLE connection is active.

## Automation
GitHub Actions automation and MacBook self-hosted runner setup are documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).

## Signed Device Builds
Use the `Companion iOS` GitHub Actions workflow with `build_kind=signed_device` to create an installable signed IPA on your self-hosted MacBook runner. The full setup and iPhone install steps are documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).

## TestFlight Builds
Use the `Companion iOS TestFlight` workflow to archive a Release IPA on a GitHub-hosted macOS runner and upload it to TestFlight. The required GitHub secrets, provisioning profiles, and App Store Connect API-key setup are documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).
