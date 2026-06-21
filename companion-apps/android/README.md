# Companion Android

Native Android companion app for route planning, BLE sync, GPX import, and reroute orchestration.

## Structure
- `app/src/main/java/.../app`: Compose app shell and state holder
- `app/src/main/java/.../domain`: route/session/provider contracts and models
- `app/src/main/java/.../integration`: HSL, GPX, BLE, persistence, diagnostics, and sample adapters
- `app/src/test/java/...`: JVM unit tests for codec and helper logic
- `gradle/libs.versions.toml`: centralized plugin and dependency versions
- `gradle/verification-metadata.xml`: checksum-based dependency verification metadata

## Build
Open `companion-android` in Android Studio and let it import the committed Gradle Wrapper project.

Local command-line validation on a machine with Java 17 and the Android SDK:
```bash
cd companion-android
./gradlew lintDebug testDebugUnitTest assembleDebug
```

The repo intentionally relies on the committed Gradle Wrapper. Do not use a machine-wide Gradle install for this module.

## Local Setup
- Java 17 is the supported JDK for local builds and CI.
- Android SDK packages required by CI are:
  - `platform-tools`
  - `platforms;android-35`
  - `build-tools;35.0.0`
- Keep provider credentials out of the repo; the HSL server-side proxy (`companion-apps/server/`) holds the Digitransit subscription key.

## Current State
- Compose app shell covers launch, planning, preview, device, ride, and settings surfaces.
- HSL planning uses the server-side Digitransit proxy at `/api/hsl/routing` with no client-side key needed.
- GPX routes can be imported through Android's document picker and flow through the same preview and sync path as provider-backed routes.
- Route alternatives can be selected before sync, and reroute publishing can be triggered from an editable rider location in the ride/device surfaces.
- BLE transport uses real Android BLE/GATT packet IO when an ESP32 route-sync peripheral is available, while preserving the simulated fallback path when no BLE connection is active.

## Automation
GitHub Actions automation is documented in [`../docs/companion-builds.md`](../docs/companion-builds.md).
