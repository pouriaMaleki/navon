# Session Handoff

## Current Goal
Land the three-platform companion parity pass: a new web companion, real device GPS across all three apps, HSL gating by key and region, Where-to? URL paste + dismiss UX, and a single-container production deployment that serves both the companion and the emulator behind one hostname.

## Product Direction
- Home is the only primary surface on every platform.
- Settings is secondary; Route Planner owns HSL configuration.
- Web companion is phone-guidance only (no device/BLE).
- Rider position is real device GPS; the planner always knows where the rider is or explicitly shows that it is locating.
- HSL only appears when it can actually plan a trip (key configured AND both endpoints inside Uusimaa); otherwise only OSM is offered.
- Pasted http(s) URLs (Google Maps / OSM) resolve to a destination inline.
- GPX import is first-class.
- Unknown or unsupported imports still go to `Import Diagnostics`.

## Current Status
- Three commits pushed to `origin/main` on top of `9b4c0ba`:
  - `40a00b1` — Add companion-web app and extend CVE coverage.
  - `d308082` — Refresh iOS and Android companions with real GPS, HSL gating, and Where to? improvements.
  - `5bb3f52` — Serve companion-web and emulator-web from one nginx container.
- Working tree is clean.
- GitHub reports 2 moderate Dependabot alerts on the default branch (unreviewed in this session).

## Implemented In This Branch
- Web companion (`companion-web/`):
  - Vite + React 19 + `mobx-react-lite` with a single `RootStore` and sub-stores per concern.
  - MapLibre GL JS on OpenStreetMap raster tiles.
  - HSL Digitransit + OSRM bike providers, GPX import via DOMParser, sample fallback.
  - Photon (typeahead) + Nominatim (reverse geocode) place search.
  - localStorage persistence under `companion.*` keys that match the native apps.
  - URL paste in Where-to? uses a public proxy fallback chain (r.jina.ai then api.allorigins.win) to resolve Google Maps short links without a backend.
- Cross-platform GPS:
  - New `LocationService` seam with `CoreLocationService` (iOS), `AndroidLocationService` (Android, FusedLocationProvider), and `BrowserLocationService` + `LocationStore` (web).
  - All three apps persist the last fix under `companion.lastKnownRider` and fall back to that then a static default when permission is missing.
  - Locate/recenter control renders a spinner until the first fix arrives.
- HSL gating:
  - `isHslLiveConfigured` (toggle on + non-empty key) and `isHslApplicableForRequest` (both endpoints inside the Uusimaa bounding box 59.8–60.8°N, 23.3–26.7°E) combine into `isHslAvailable`.
  - When false, the source picker collapses to OSM on all three apps; mixed-mode planning skips the HSL race; the persisted default is normalized to OSM when the key is removed.
  - Route Planner settings has a help paragraph and a link to https://portal-api.digitransit.fi/.
- Where-to? UX:
  - Outside tap / Escape / back-button dismisses the dropdown (platform-appropriate).
  - Pasted http(s) URLs run through a shared `UrlDestinationResolver` (inline-coord regex first; then platform-native redirect following on iOS/Android, proxy chain on web). Loading and error rows show in the search panel.
- Production deployment:
  - `emulator/web/Dockerfile` is now a four-stage build that ships a single `nginx:1.27-alpine` runtime hosting companion-web at `/` and emulator-web at `/emulator/`.
  - Emulator Vite reads `VITE_BASE_PATH` so its assets resolve under the prefix in the combined image while local dev still uses `/`.
  - Service name, port, and external routing are unchanged.
- CVE tracking:
  - `npm audit` job matrixes over `emulator/web` and `companion-web`.
  - Dependency-manifest guard allows both lockfiles; docs and `project-spec.md` reflect the new tracked source.

## Partial / In-Progress Work
- `companion-android`: the Gradle build could not be verified on the current host (no Android SDK). `./gradlew :app:assembleDebug` must run on the CI host or a local macOS/Linux with the SDK before shipping.
- Web URL-paste short-link expansion depends on two public CORS proxies (r.jina.ai and api.allorigins.win). A self-hosted proxy would remove that external dependency.
- 2 moderate Dependabot alerts on the default branch remain untriaged.

## Known Risks
- Third-party proxy dependency for Google Maps short links on web only; native apps follow redirects directly and are unaffected.
- Uusimaa bounding box is coarse. If the product expands HSL coverage (or HSL itself does), the box will need to move.
- `normalizeSourceModeForHslAvailability` only normalizes the *persisted default* when the key is missing; per-trip geographic non-applicability flips the active mode for that planning session but preserves the user's persisted preference.

## Next Steps
1. Triage the 2 moderate Dependabot alerts on `origin/main`.
2. Run `./gradlew :app:assembleDebug` on a host with the Android SDK to confirm the Android build is clean end to end.
3. Manual smoke on a real device for each platform (permission prompt, first-fix spinner flip, HSL collapse when key is empty, HSL collapse for a trip outside Uusimaa, pasted `maps.app.goo.gl/...` resolving to a plan).
4. Consider a repo-owned URL-expansion endpoint if the public proxy dependency becomes a reliability issue.

## Validation
Passed:
- iOS `xcodebuild -sdk iphonesimulator26.4` Debug build succeeded.
- Web: `npm run typecheck`, `npm run lint`, `npm run test` (35 tests), `npm run build` all clean.
- Web dev server boots and serves on `http://localhost:5173/`.

Not verified:
- Android `assembleDebug` on this host (no SDK).
- Hardware runs of BLE / real-device share flows.
- Actual Dependabot alert coverage on the default branch.

## Environment Notes
- No Android SDK on the current host; Gradle build is deferred to CI or a real dev machine.
- iOS simulator is out-of-sync relative to the installed Xcode (`CoreSimulator is out of date`); only the SDK-compile path was verified, not a simulator run.

## Files To Inspect First
- `companion-web/src/stores/PlanningStore.ts` — HSL gating, Uusimaa check, URL-paste orchestration.
- `companion-web/src/integrations/shareImport/UrlExpander.ts` — proxy chain and coord regexes.
- `companion-ios/CompanionApp/App/AppModel.swift` — `isHslAvailable`, source-mode normalization, mixed-mode race skipping HSL.
- `companion-android/app/src/main/java/me/fiksu/esp32map/companion/app/CompanionAppState.kt` — same shape on Android.
- `emulator/web/Dockerfile` and `emulator/web/nginx.conf` — production deployment.

## Commands To Resume
```bash
git status --short
cd companion-web && npm install && npm run test && npm run build
cd companion-ios && xcodegen generate && xcodebuild -project ESP32MapCompanion.xcodeproj -scheme ESP32MapCompanion -sdk iphonesimulator26.4 -configuration Debug -derivedDataPath /tmp/companion-ios-build build CODE_SIGNING_ALLOWED=NO
cd companion-android && ./gradlew :app:assembleDebug  # needs Android SDK on the host
docker compose up --build -d
```
