# Companion Build Automation

This repo uses a split mobile build path:
- Android builds on GitHub-hosted Ubuntu runners.
- iOS builds on a self-hosted macOS runner, which can be your MacBook Pro.
- iOS TestFlight uploads build on GitHub-hosted macOS runners with App Store Connect signing material stored in GitHub secrets.

## Why This Setup
- Android can build cleanly on Linux CI.
- iOS requires macOS and Xcode, so the easiest controlled setup is to let GitHub Actions dispatch iOS jobs to your own Mac.
- This keeps the Linux home server as the main development environment while making mobile builds mostly automatic after every push or pull request.

## Workflows
- `.github/workflows/companion-android.yml`
  - Validates the committed Gradle Wrapper
  - Runs Android lint, JVM unit tests, and debug assembly on `ubuntu-latest`
  - Produces a debug APK artifact
- `.github/workflows/companion-ios-self-hosted.yml`
  - `simulator_validation`: validates the app shell with an unsigned simulator build
  - `signed_device`: archives and exports a signed iPhone IPA on your self-hosted MacBook runner
- `.github/workflows/companion-ios-testflight.yml`
  - manually archives a signed Release IPA on a GitHub-hosted macOS runner
  - uploads the IPA to TestFlight using an App Store Connect API key

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

## GitHub-Hosted TestFlight Prerequisites
The hosted TestFlight workflow does not need your MacBook runner, but it does need App Store Connect signing material in GitHub.

Repository variables:
- `IOS_DEVELOPMENT_TEAM`
- `APPSTORE_API_KEY_ID`
- `APPSTORE_ISSUER_ID`

Repository secrets:
- `APPSTORE_CERTIFICATES_FILE_BASE64`
  - base64-encoded exported `.p12` containing your Apple Distribution certificate and private key
- `APPSTORE_CERTIFICATES_PASSWORD`
  - password used when exporting the `.p12`
- `APPSTORE_API_PRIVATE_KEY`
  - contents of the App Store Connect API key `.p8`
- `IOS_APP_STORE_PROFILE_BASE64`
  - base64-encoded App Store provisioning profile for `me.fiksu.esp32map.companion.ios`
- `IOS_SHARE_EXTENSION_PROFILE_BASE64`
  - base64-encoded App Store provisioning profile for `me.fiksu.esp32map.companion.ios.share`

Signing requirements:
- both the app target and the share extension need App Store provisioning profiles
- the share extension profile must include the same app-group capability used by the app target
- the certificate and both profiles must belong to the same Apple team

## iPhone Signing Prerequisites On The Mac
Before the signed workflow can produce an installable IPA, do this once on the MacBook:

1. Sign into Xcode with the Apple account you want to use for device signing:
   - `Xcode -> Settings -> Accounts`
2. Make sure that account has a valid team available.
3. Connect your iPhone to the MacBook with USB once.
4. Unlock the phone and trust the Mac if prompted.
5. Enable `Developer Mode` on the iPhone if iOS asks for it.
6. In Xcode, confirm the phone appears under:
   - `Window -> Devices and Simulators`
7. Note your Apple Team ID.
   - You can find it in the Apple Developer portal or from Xcode signing settings on another app.
8. If you plan to use `ad-hoc` export, ensure your phone is registered in the provisioning profile for that team.

## Recommended MacBook Settings
- Disable automatic sleep while on power.
- Keep enough free disk space for Xcode derived data and archives.
- Make sure `xcodebuild -version` works in the same shell context as the runner service.
- If you use multiple self-hosted runners later, add a custom label and then tighten the workflow `runs-on` labels.

## What Runs Automatically
### Android
On pushes to `main`, pull requests, or manual dispatch:
```bash
cd companion-android
./gradlew lintDebug testDebugUnitTest assembleDebug
```
The resulting debug APK is uploaded as a GitHub Actions artifact.

Android dependency tracking also uses the committed Gradle Wrapper so GitHub dependency graph submission resolves the same toolchain and repositories as local development.

### iOS Simulator Validation
On pushes to `main`, pull requests, or manual dispatch with `build_kind=simulator_validation`:
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

## How To Produce A Signed iPhone Build
Run the `Companion iOS` workflow manually in GitHub Actions with:
- `build_kind`: `signed_device`
- `ios_bundle_id`: your chosen bundle id
  - default: `me.fiksu.esp32map.companion.ios`
- `ios_development_team`: optional override for your Apple Team ID
  - if omitted, the workflow uses repository variable `IOS_DEVELOPMENT_TEAM`
- `export_method`: `development` or `ad-hoc`

The workflow will:
1. Generate the Xcode project on your MacBook runner.
2. Archive a signed iPhone build using automatic signing.
3. Export an installable `.ipa`.
4. Upload the `.ipa` and `.xcarchive` as GitHub Actions artifacts.

## How To Install The Signed Build On Your iPhone From Your Mac
After the workflow finishes:

1. On the MacBook, download the `companion-ios-signed-ipa` artifact from the workflow run.
2. Connect the iPhone to the MacBook with USB.
3. Open Xcode.
4. Go to:
   - `Window -> Devices and Simulators`
5. Select your iPhone in the sidebar.
6. Drag the exported `.ipa` onto the installed apps area for that device.

Alternative:
- Use Apple Configurator on the Mac and add the `.ipa` to the connected device.

## If The App Won't Launch On The Phone
Check these first:
1. The IPA was exported with the same Apple team that can sign for your device.
2. The device is included in the provisioning profile.
3. `Developer Mode` is enabled on the iPhone.
4. The phone trusts the Mac and has been unlocked since connecting.
5. The chosen bundle identifier is unique for your Apple team.

## Release Build Path Later
Current automation now supports:
- self-hosted signed iPhone IPA export for direct device install
- GitHub-hosted TestFlight upload

Later steps can still add:
- Android release `aab` generation
- richer beta release notes / versioning flows
- App Store submission automation

## Operational Notes
- If the MacBook runner is offline, only the iOS workflow will wait/fail; Android and the rest of CI remain unaffected.
- The TestFlight workflow is independent of the self-hosted Mac and can run as long as the GitHub signing secrets remain valid.
- The Linux home server remains the main place for shared Rust, emulator, and documentation work.
- iOS UI development is still easiest from Xcode on the Mac, but signed device builds can now be triggered remotely from GitHub Actions.

## How To Upload A TestFlight Build
Run the `Companion iOS TestFlight` workflow manually in GitHub Actions.

The workflow will:
1. Generate the Xcode project on a GitHub-hosted macOS runner.
2. Import the Apple Distribution certificate from GitHub secrets.
3. Install the app and share-extension provisioning profiles.
4. Archive and export a Release IPA.
5. Upload the IPA to TestFlight.
6. Save the exported IPA and `.xcarchive` as workflow artifacts.

Use this path when:
- you want a clean remote beta-distribution flow
- the tester can install through TestFlight
- you do not need a directly sideloaded IPA onto one specific phone
