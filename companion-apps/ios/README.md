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
open Navon.xcodeproj
```

In Xcode, select scheme `Navon` and run.

## Test (CLI)
```bash
xcodebuild test \
  -project Navon.xcodeproj \
  -scheme Navon \
  -destination 'platform=iOS Simulator,name=iPhone 15'
```
