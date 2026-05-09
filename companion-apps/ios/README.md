# iOS Build (Simple)

## Prerequisites
- Xcode 15+
- `xcodegen` (`brew install xcodegen`)

## Build
```bash
cd companion-apps/ios
cp Config/Signing.local.example.xcconfig Config/Signing.local.xcconfig
# Set your team id in Config/Signing.local.xcconfig (IOS_DEVELOPMENT_TEAM)
xcodegen generate
open ESP32MapCompanion.xcodeproj
```

In Xcode, select scheme `ESP32MapCompanion` and run.

## Test (CLI)
```bash
xcodebuild test \
  -project ESP32MapCompanion.xcodeproj \
  -scheme ESP32MapCompanion \
  -destination 'platform=iOS Simulator,name=iPhone 15'
```
