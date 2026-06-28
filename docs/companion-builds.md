# Companion Build Automation

This repo uses a split mobile build path:
- Android builds on GitHub-hosted Ubuntu runners.
- iOS builds on a self-hosted macOS runner (your Mac).

## Why This Setup
- Android can build cleanly on Linux CI.
- iOS requires macOS and Xcode, so the easiest controlled setup is to let GitHub Actions dispatch iOS jobs to your own Mac.
- This avoids GitHub-hosted macOS runner costs (10x Linux minutes) while keeping the Linux home server as the main development environment.

## Workflows
- `.github/workflows/companion-android.yml`
  - Validates the committed Gradle Wrapper
  - Runs Android lint, JVM unit tests, and debug assembly on `ubuntu-latest`
  - Produces a debug APK artifact
- `.github/workflows/companion-ios.yml`
  - Runs on self-hosted macOS runner (your MacBook)
  - `validate`: builds the app shell with an unsigned simulator build and runs unit tests
  - `deploy`: exports a signed App Store IPA, uploads to TestFlight, and distributes to a test group
- `.github/workflows/security-audit.yml`
  - Runs cargo-audit (Rust), npm-audit (web + emulator + homepage), and dependency-review on PRs
  - Enforces dependency manifest coverage — only approved ecosystems are allowed

## TestFlight Prerequisites

Repository variables:
- `IOS_DEVELOPMENT_TEAM` — Apple Developer team ID (`NG769F7J64`)
- `APPSTORE_API_KEY_ID` — App Store Connect API key ID
- `APPSTORE_ISSUER_ID` — App Store Connect API issuer ID

Repository secrets:
- `APPSTORE_CERTIFICATES_FILE_BASE64`
  - base64-encoded exported `.p12` containing your Apple Distribution certificate and private key
- `APPSTORE_CERTIFICATES_PASSWORD`
  - password used when exporting the `.p12`
- `APPSTORE_API_PRIVATE_KEY`
  - contents of the App Store Connect API key `.p8`
- `IOS_APP_STORE_PROFILE_BASE64`
  - base64-encoded App Store provisioning profile for `app.navon.bike`
- `IOS_SHARE_EXTENSION_PROFILE_BASE64`
  - base64-encoded App Store provisioning profile for `app.navon.bike.share`
- `IOS_LIVEACTIVITY_PROFILE_BASE64`
  - base64-encoded App Store provisioning profile for `app.navon.bike.liveactivity`

Signing requirements:
- All three embeddable targets (main app, share extension, live activity) need App Store provisioning profiles
- The share extension profile must include the same app-group capability used by the app target
- The certificate and all profiles must belong to the same Apple team

## Local iPhone Signing Prerequisites

Before running the app on a physical iPhone from Xcode:
1. Sign into Xcode with the Apple account you want to use for device signing:
   - `Xcode -> Settings -> Accounts`
2. Make sure that account has a valid team available.
3. Connect your iPhone to the Mac with USB once.
4. Unlock the phone and trust the Mac if prompted.
5. Enable `Developer Mode` on the iPhone if iOS asks for it.
6. In Xcode, confirm the phone appears under:
   - `Window -> Devices and Simulators`
7. If you plan to use `ad-hoc` export, ensure your phone is registered in the provisioning profile for that team.
8. Persist the team for local Xcode builds:
   ```bash
   cd companion-apps/ios
   cp Config/Signing.local.example.xcconfig Config/Signing.local.xcconfig
   ```
9. Edit `companion-apps/ios/Config/Signing.local.xcconfig` and set:
   ```xcconfig
   DEVELOPMENT_TEAM = NG769F7J64
   ```
10. Regenerate the project after pulls when needed:
    ```bash
    xcodegen generate
    ```

`Signing.local.xcconfig` is ignored by git, so your local team selection survives repository updates and Xcode project regeneration.

## What Runs Automatically

### Android
On pushes to `main`, pull requests, or manual dispatch:
```bash
cd companion-apps/android
./gradlew lintDebug testDebugUnitTest assembleDebug
```
The resulting debug APK is uploaded as a GitHub Actions artifact.

Android dependency tracking also uses the committed Gradle Wrapper so GitHub dependency graph submission resolves the same toolchain and repositories as local development.

### iOS Validation
On pushes to `main`, pull requests, or manual dispatch with `action=validate`:
```bash
cd companion-apps/ios
xcodegen generate
xcodebuild \
  -project Navon.xcodeproj \
  -scheme Navon \
  -configuration Debug \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO \
  build
xcodebuild test \
  -project Navon.xcodeproj \
  -scheme Navon \
  -only-testing:NavonTests \
  -destination 'platform=iOS Simulator,name=iPhone 16 Pro,OS=latest' \
  CODE_SIGNING_ALLOWED=NO
```
This validates the app shell and runs unit tests without requiring provisioning profiles or App Store signing.

## How To Deploy A Beta Build To TestFlight

Run the `Navon iOS` workflow manually in GitHub Actions with:
- `action`: `deploy`
- `test_group`: the TestFlight group to distribute to (default: `Internal Testers`)

The workflow will:
1. Generate the Xcode project on the self-hosted macOS runner.
2. Import the Apple Distribution certificate from GitHub secrets.
3. Install provisioning profiles for all three embeddable targets (main app, share extension, live activity).
4. Set the build number to the GitHub run number (monotonically increasing).
5. Archive and export a Release IPA.
6. Upload the IPA and xcarchive as workflow artifacts.
7. Upload to TestFlight via fastlane pilot and distribute to the specified test group.

Use this path when:
- you want a clean remote beta-distribution flow
- the tester can install through TestFlight
- you do not need a directly sideloaded IPA onto one specific phone

## Build Number Management

Build numbers (`CFBundleVersion`) are auto-incremented to the GitHub run number via `agvtool new-version`. This guarantees unique, monotonically increasing build numbers per App Store Connect requirements.

Marketing versions (`CFBundleShortVersionString`) are set manually in each target's Info.plist and updated when a new release is cut.

## fastlane Setup

The TestFlight upload and distribution uses fastlane (`companion-apps/ios/fastlane/`). The fastlane Gemfile is whitelisted in the security audit workflow as a managed dependency path.

If fastlane or its dependencies need updating:
```bash
cd companion-apps/ios
bundle update fastlane
# commit the updated Gemfile.lock
```

## Operational Notes
- iOS validation and deployment run on the self-hosted macOS runner — keep the Mac powered on and awake when you want iOS workflows to run.
- If the Mac runner is offline, only the iOS workflow will wait/fail; Android and the rest of CI remain unaffected.
- The TestFlight deployment uses `fastlane pilot` which handles build processing wait and test group distribution automatically.
- The Linux home server remains the main place for shared Rust, emulator, and documentation work.
- iOS UI development is still easiest from Xcode on the Mac, but CI builds and TestFlight distribution run on the same machine triggered remotely from GitHub Actions.
