# ESP32 Bike Minimap

This is a bike and hike map application, focused mainly on ease of use when on the bike. An ESP32 based map screen (like a bike computer or a handheld GPS) is an optional and aditional tool that should help navigating instead of using your phone.

Main features of this application is:

- Proper camera for riding (heading direction up) and exploring mode (north up)
- Visible useful POIs for cycling and hiking
- Audio cues for navigation
- iOS and Android navigation guide from lock screen
- Using BRouter.de and OSM router to get best cycling and walking routes
- Support for importing GPX files or Google maps links (link to destination, not the route)
- Support using Digitransit (HSL) route planner in Finland to get best cycling and walking routes
- Secure connection to ESP32 handheld and passing route to it
- Many languages support (Machine translated)

## Demo

- Web version of the app (There might be issues with background locations, due to OS limitations): <https://map.fiksu.me>
- ESP32 Web Emulator: <https://map.fiksu.me/emulator>
- iOS app: `companion-apps/ios/`
- Android app: `companion-apps/android/`

<img src="demo/demo-2.jpg" alt="Web app screenshot" width="160" /> <img src="demo/demo-3.png" alt="Web app screenshot route planning" width="160" /> <img src="demo/demo-4.png" alt="iOS App screenshot route planning" width="160" /> <img src="demo/demo-5.png" alt="iOS app settings page screenshot" width="160" /> <img src="demo/demo-6.png" alt="iOS lock screen guides screenshot" width="160" /> <img src="demo/demo-1.jpg" alt="ESP32-P4 with 3.4inch screen as a handheld device rendering map" width="160" />

## Dev Setup (Dev Container)

Recommended flow is VS Code Remote SSH + Dev Containers.

1. Copy `.env.example` to `.env` and fill in your API keys (gitignored).
2. Open this repo in VS Code.
3. Run `Dev Containers: Reopen in Container`.
4. Use the container terminal for all commands below.

## Run Dev (All Projects)

From repo root:

ESP32 Firmware:

- Emulator (Rust + web): `cargo xtask emu`
- Firmware bundle image: `cargo xtask bundle-device`
- Firmware direct flash: `cargo xtask deploy-device --port /dev/ttyUSB0`

Companion projects:

- Web companion:
  ```bash
  cd companion-apps/web
  npm install
  npm run dev
  ```
- iOS companion:
  ```bash
  cd companion-apps/ios
  brew install xcodegen
  cp Config/Signing.local.example.xcconfig Config/Signing.local.xcconfig
  xcodegen generate
  open ESP32MapCompanion.xcodeproj
  ```
- Android companion:
  ```bash
  cd companion-apps/android
  ./gradlew lintDebug testDebugUnitTest assembleDebug
  ```

## Docker Compose

`compose.yaml` serves both apps from one container:

- `/` -> companion web
- `/emulator/` -> emulator

Commands:

```bash
docker compose up --build -d
docker compose logs -f emulator-web
docker compose down
```

Default port is `4173` (override with `EMULATOR_PORT`).

## Translations (i18n)

- Main source language file: `i18n/locales/en.json`
- Locale config: `i18n/catalog.config.json`

Before commit, run:

```bash
cargo xtask i18n-gen
cargo xtask i18n-sync-all
cargo xtask i18n-gen
```

For one locale only:

```bash
cargo xtask i18n-sync --locale fi
```

## Hardware [Optional]

- Target device: Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C (3.4")
- Product page: <https://www.waveshare.com/esp32-p4-wifi6-touch-lcd-3.4c.htm>

## Make a Map for ESP32 Handheld Device

Source dataset (MapTiler):
<https://www.maptiler.com/on-prem-datasets/europe/finland/helsinki/>

1. Download your map `.mbtiles` file.
2. Put it in `data/map-src/` (example: `data/map-src/helsinki.mbtiles`).
3. Convert to runtime map:
   ```bash
   cargo run -p map-vector-cli -- \
     convert-mbtiles \
     --input data/map-src/helsinki.mbtiles \
     --output data/map-data/city.svm \
     --target-zoom 16 \
     --profile bike
   ```
4. Create smaller flash-friendly map:
   ```bash
   cargo run -p map-vector-cli --release -- \
     shrink-svm --input data/map-data/city.svm \
               --output data/map-data/city-small.svm \
               --max-segments 400000
   ```

## Flash Firmware to ESP32-P4

General flow:

1. Build firmware image:
   ```bash
   cargo xtask bundle-device
   ```
2. Flash app image to device:
   ```bash
   espflash write-bin --chip esp32p4 --port <PORT> 0x0 .xtask/device/firmware-release-app.bin
   ```
3. Flash map partition:
   ```bash
   espflash write-bin --chip esp32p4 --port <PORT> 0x400000 data/map-data/city-small.svm
   ```
4. Monitor logs:
   ```bash
   espflash monitor --chip esp32p4 --port <PORT>
   ```

Copy city.svm to SD Card if you want larger size map. Flashed map is very small and provided only as a backup (or for testing).

## Folder Structure

```text
.
├─ companion-apps/            User-facing companion apps (web + native).
│  ├─ web/                    Companion web app.
│  ├─ ios/                    Companion iOS app and tests.
│  └─ android/                Companion Android app and tests.
│
├─ device/                    Device-side runtime stack and simulator.
│  ├─ firmware/               ESP32 firmware and hardware integrations.
│  ├─ emulator/               Browser-based runtime emulator/simulator UI.
│  └─ core/                   Shared Rust runtime/render crates used by device flows.
│     ├─ runtime-core/        Runtime logic (camera, motion, routing, input).
│     ├─ render-core/         Rendering pipeline and visual assets.
│     ├─ render-core-wasm/    WASM bridge exposing runtime/render APIs.
│     ├─ map-runtime/         Shared map/runtime interfaces.
│     └─ route-import-gpx/    GPX-to-route importer (used via WASM import path).
│
├─ data/                      Shared datasets and generated fixture/map artifacts.
│  ├─ parity-fixtures/        Cross-platform parity fixture data and tests.
│  ├─ map-src/                Input map datasets (usually .mbtiles files).
│  └─ map-data/               Generated runtime map files (.svm).
│
├─ tools/                     Developer tooling and conversion CLIs.
│  ├─ xtask/                  Workspace task runner (build, emu, fixtures, i18n).
│  └─ map-vector-cli/         Map conversion CLI (.mbtiles -> .svm and shrink).
│
├─ docs/                      Product specs, plans, architecture notes, contracts.
├─ i18n/                      Translation catalogs and localization tooling data.
├─ infra/                     Devcontainer and local environment setup scripts.
└─ demo/                      Screenshots and demo media for documentation.
```
