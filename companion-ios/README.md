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
xcodegen generate
open ESP32MapCompanion.xcodeproj
```

## Current State
- Full app shell is scaffolded.
- HSL and BLE layers are stubbed behind explicit interfaces.
- Route package publishing, reroute handling, and persistence boundaries are defined but not production-complete yet.

## Automation
GitHub Actions automation and MacBook self-hosted runner setup are documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).
