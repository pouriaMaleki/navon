# Companion Android

Native Android companion app scaffold for route planning, BLE sync, and reroute orchestration.

## Structure
- `app/src/main/java/.../app`: Compose shell and state holder
- `app/src/main/java/.../domain`: route/session/provider models
- `app/src/main/java/.../integration`: HSL, BLE, persistence, and diagnostics integrations

## Build
Open `companion-android` in Android Studio and let it sync the Gradle Kotlin DSL project.

Command-line build on a machine with Android SDK:
```bash
cd companion-android
./gradlew assembleDebug
```

## Current State
- Full app shell is scaffolded with provider picker, planning, preview, device, ride, and settings surfaces.
- HSL and BLE layers are explicit integration seams with demo-backed implementations.
- Real BLE protocol and provider networking still need production implementation.

## Automation
GitHub Actions automation is documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).
