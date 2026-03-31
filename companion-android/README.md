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
- HSL planning can run in live Digitransit mode when a subscription key is configured, with explicit sample fallback when live routing is unavailable.
- Route alternatives can be selected before sync, and reroute publishing can be triggered from an editable rider location in the ride/device surfaces.
- BLE transport now uses real Android BLE/GATT packet IO when an ESP32 route-sync peripheral is available, while preserving the simulated fallback path when no BLE connection is active.

## Automation
GitHub Actions automation is documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).
