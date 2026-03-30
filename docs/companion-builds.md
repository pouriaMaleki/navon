# Companion Build Automation

This repo uses a split mobile build path:
- Android builds on GitHub-hosted Ubuntu runners.
- iOS builds on a self-hosted macOS runner, which can be your MacBook Pro.

## Why This Setup
- Android can build cleanly on Linux CI.
- iOS requires macOS and Xcode, so the easiest controlled setup is to let GitHub Actions dispatch iOS jobs to your own Mac.
- This keeps the Linux home server as the main development environment while making mobile builds mostly automatic after every push or pull request.

## Workflows
- `.github/workflows/companion-android.yml`
  - Builds `companion-android` on `ubuntu-latest`
  - Produces a debug APK artifact
- `.github/workflows/companion-ios-self-hosted.yml`
  - Builds `companion-ios` on a self-hosted macOS runner
  - Generates the Xcode project with XcodeGen
  - Runs an unsigned simulator build to validate the app shell without release signing

## MacBook Pro Self-Hosted Runner Setup
1. Install Xcode from the App Store.
2. Open Xcode once and accept the license.
3. Install command line tools:
   ```bash
   xcode-select --install
   ```
4. Install Homebrew if it is not already installed.
5. Install XcodeGen:
   ```bash
   brew install xcodegen
   ```
6. In GitHub, open the repository settings:
   - `Settings -> Actions -> Runners -> New self-hosted runner`
7. Choose macOS and follow the setup commands on the MacBook.
8. Start the runner as a service so it survives logout/reboot.
9. Keep the Mac awake and on power when you want iOS workflows to run.

## Recommended MacBook Settings
- Disable automatic sleep while on power.
- Keep enough free disk space for Xcode derived data.
- Make sure `xcodebuild -version` works in the same shell context as the runner service.
- If you use multiple self-hosted runners later, add a custom label and then tighten the workflow `runs-on` labels.

## What Runs Automatically
### Android
On pushes to `main`, pull requests, or manual dispatch:
```bash
.gradle-installed-by-action
sdkmanager platforms;android-35 build-tools;35.0.0
gradle :app:assembleDebug
```
The resulting debug APK is uploaded as a GitHub Actions artifact.

### iOS
On pushes to `main`, pull requests, or manual dispatch:
```bash
cd companion-ios
xcodegen generate
xcodebuild \
  -project ESP32MapCompanion.xcodeproj \
  -scheme ESP32MapCompanion \
  -configuration Debug \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO \
  build
```
This validates the app shell without requiring provisioning profiles or App Store signing.

## Release Build Path Later
The current automation is intentionally unsigned and validation-first.

Later steps can add:
- Android release `aab` generation
- iOS archive/export with signing
- TestFlight upload
- Firebase App Distribution or similar beta distribution

## Operational Notes
- If the MacBook runner is offline, only the iOS workflow will wait/fail; Android and the rest of CI remain unaffected.
- The Linux home server remains the main place for shared Rust, emulator, and documentation work.
- iOS UI development is still easiest from Xcode on the Mac, but builds can be triggered remotely by pushing commits.
