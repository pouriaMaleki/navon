# iOS plan — BLE pairing UX, paired-peripheral persistence, home chip

> **Transient planning doc.** This file is committed to the repo so a Mac-resident agent (one that can run `xcodegen`, `xcodebuild`, the iOS Simulator, and Swift tests) can pick it up. **Delete this file in the same PR that ships the iOS implementation** — it's not durable architecture documentation.

## Audience + environment

You're an agent running on a macOS workstation with:
- Xcode + Command Line Tools.
- `xcodegen` on PATH (project regen from `companion-ios/project.yml`).
- iOS Simulator runtimes for at least iPhone 16 (iOS 18+).
- Read access to the full repo (firmware + Android companions) so you can match contracts.

You **cannot** assume the firmware or Android changes are already in place. The Linux-side agent is shipping firmware + Android + cross-platform parity fixtures in parallel under the sibling plan (`/home/vscode/.claude/plans/okay-now-plan-all-sunny-owl.md`, not in the repo). Coordinate via:

1. Wire-format goldens in `parity-fixtures/data/` — `paired_peripheral.json` and `pairing_qr_v1.json`. **Both halves must decode them identically.**
2. The BLE GATT contract spelled out below.
3. UX copy strings — verbatim parity with Android.

## Context

The current iOS companion is at `companion-ios/`. Recent end-to-end BLE bring-up confirmed scan/connect/route-push works against a Waveshare 3.4C device. Open issues that this plan addresses:

- **Wrong device on busy parking lots.** `CoreBluetoothRouteSyncClient.scanForRouteSyncPeripheral` connects to the first peripheral matching the service UUID. Two riders nearby = race.
- **No home-screen affordance** for connect/status; reconnect lives behind two taps in Settings → Device.
- **No pairing.** Anyone in BLE range can write routes onto a stranger's device. Phase 3 of this plan adds OOB-confirmed pairing on top of BLE Just Works pairing.

## User-confirmed product decisions

- **Single bond, explicit unpair first.** Device rejects pairing while a bond exists. To switch phones the user must tap **Forget** in the bonded phone first; only then will the device drop back to pairing mode and show a fresh QR.
- **Re-pair entry from companion only.** No on-device gesture. If the bonded phone is permanently lost, recovery requires a future factory-reset path or reflash (out of scope).

## What you'll build (high level)

Two phases — Phase A is no-crypto UX foundation, Phase B is the actual pairing flow. Each phase is split into red-test-first steps. Each step lists files, the assertion shape of every red test, the implementation sketch, and how to verify green.

Cross-platform invariants (must match Android byte-for-byte):
- BLE GATT UUIDs, including the new `pairing_confirm` characteristic.
- `PairedPeripheralRecord` JSON schema.
- `PairingQrPayload` JSON schema.
- UX copy for chip states, settings sections, and the Forget alert.

---

## BLE GATT contract (cross-platform)

| Field | UUID |
|---|---|
| Service | `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001` |
| Chunk write (companion → device) | `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002` |
| Event notify (device → companion) | `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003` |
| **Pairing confirm (new)** (companion → device) | `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1004` |

After Phase B ships on the firmware:
- Chunk-write and event-notify use `ESP_GATT_PERM_*_ENCRYPTED` permissions; CoreBluetooth's first write to either triggers Just Works pairing automatically.
- The `pairing_confirm` characteristic is `WRITE_ENCRYPTED`, available **only** while the device is in pairing mode (no bond stored). Writing the matching ephemeral secret transitions the device to operational mode and persists the bond.

---

## Wire format

### `PairedPeripheralRecord` — locally persisted

```json
{
  "identifier": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
  "friendlyName": "ESP32 Bike Minimap",
  "pairedAt": "2026-04-28T12:34:56.789Z"
}
```

- `identifier`: iOS uses the `peripheral.identifier.uuidString` from CoreBluetooth (it's stable across reconnects of the same paired peer on iOS). Android uses BLE MAC. Use the platform-native form.
- `pairedAt`: ISO-8601 with milliseconds and Z suffix.

Golden: `parity-fixtures/data/paired_peripheral.json`. Your Codable decoder MUST round-trip it exactly.

### `PairingQrPayload` v1 — emitted by the firmware as JSON in the QR

```json
{
  "v": 1,
  "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
  "id_android": "AA:BB:CC:DD:EE:FF",
  "secret": "<base64 32 bytes>",
  "fw": "1.2.3"
}
```

- The firmware emits both `id_ios` and `id_android` so the same QR works on either companion. Your iOS decoder reads `id_ios`.
- `secret`: 32 bytes of device-generated random, base64-encoded. Used as the OOB confirmation when writing to `pairing_confirm`.
- `fw`: optional. Reject `v` other than `1` with a specific error.

Golden: `parity-fixtures/data/pairing_qr_v1.json`.

---

## Phase A — UX foundation (no crypto yet)

Each step: red tests first, implement, verify green via `xcodebuild test -scheme ESP32MapCompanion -destination 'platform=iOS Simulator,name=iPhone 16'`.

### A.1 Extract `RouteSyncBluetoothClient` protocol

- **File**: `companion-ios/CompanionApp/Integration/BLE/CoreBluetoothRouteSyncClient.swift` — declare `protocol RouteSyncBluetoothClient: AnyObject` covering the surface `BleRouteSyncService` actually uses today: `var onSyncMessage`, `var onConnectionStateChange`, `var isReady`, `func armDebugFault(_:)`, `func scanForRouteSyncPeripheral(timeout:) async throws -> String`, `func connectToScannedPeripheral() async throws -> String`, `func write(packet:) async throws`. Make `final class CoreBluetoothRouteSyncClient` conform.
- **File**: `companion-ios/CompanionApp/Integration/BLE/BleRouteSyncService.swift` — change the `bluetoothClient` parameter type from concrete to protocol; default `init` arg keeps `CoreBluetoothRouteSyncClient()`.
- **New file**: `companion-ios/ESP32MapCompanionTests/Fakes/FakeRouteSyncBluetoothClient.swift` — protocol-conforming fake with public counters: `scanCallCount`, `connectCallCount`, `connectToPairedCallCount`, `writeCallCount`, `lastConnectedIdentifier`, `lastWrittenPacket`, `lastWrittenPairingSecret`. Mirrors the existing `FakeLocationService.swift` pattern.
- **Red test**: `BleRouteSyncServiceProtocolWiringTests.test_serviceAcceptsFakeBluetoothClient` — instantiates `BleRouteSyncService(bluetoothClient: FakeRouteSyncBluetoothClient())`; asserts `sessionState.connectionState == .disconnected` and `routeSyncState == .idle`. Single test; pure plumbing.

### A.2 Persist `PairedPeripheralRecord`

- **File**: `companion-ios/CompanionApp/Domain/Models/CompanionModels.swift` — add `struct PairedPeripheralRecord: Codable, Equatable { let identifier: String; let friendlyName: String; let pairedAt: Date }`. Use `JSONEncoder.dateEncodingStrategy = .iso8601` with `.withFractionalSeconds` to match the parity fixture; same on the decoder.
- **File**: `companion-ios/CompanionApp/Integration/Persistence/CompanionPersistence.swift` — add to the `Key` block: `static let pairedPeripheral = "companion.pairedPeripheral"`. Add `func loadPairedPeripheral() -> PairedPeripheralRecord?`, `func savePairedPeripheral(_:)`, `func clearPairedPeripheral()`. Mirror existing helpers (use the generic `load<T>`/`save<T>` methods).
- **Red tests** — new `companion-ios/ESP32MapCompanionTests/Flows/CompanionPersistencePairedPeripheralTests.swift`:
  - `test_savePairedPeripheral_roundTrips` — saves a record with a known UUID + name + `pairedAt: Date(timeIntervalSince1970: 1714305296.789)`; reads back via a fresh `CompanionPersistence` instance pointed at the same suite-scoped `UserDefaults(suiteName: UUID().uuidString)`; asserts all three fields equal. Catches Codable schema drift.
  - `test_clearPairedPeripheral_removesRecord` — saves then clears; asserts `loadPairedPeripheral() == nil`. Catches missing remove-key path.
  - `test_loadPairedPeripheral_returnsNilWhenAbsent` — fresh suite; asserts `nil`. Locks "unpaired install doesn't blow up on first launch".
  - `test_savePairedPeripheral_overwritesPriorRecord` — save A, save B, load → matches B. Locks single-bond at the persistence layer.
  - `test_loadPairedPeripheral_decodesParityFixture` — reads the bytes of `parity-fixtures/data/paired_peripheral.json` directly through `JSONDecoder`; asserts every field matches the documented values.

### A.3 `AppModel` exposes `pairedPeripheral` + `pairingState`

- **New enum** in `Domain/Models/CompanionModels.swift`: `enum PairingFlowState: Equatable { case idle, instructions, scanning, connecting, confirming, succeeded, failed(String) }`.
- **File**: `companion-ios/CompanionApp/App/AppModel.swift`:
  - Add `@Published private(set) var pairedPeripheral: PairedPeripheralRecord?`.
  - Add `@Published var pairingState: PairingFlowState = .idle`.
  - Add a `persistence` constructor parameter (default `CompanionPersistence()`) so tests can inject; today the field is a `let` literal — convert it to an init parameter.
  - In `init`, load `pairedPeripheral` from `persistence.loadPairedPeripheral()`.
  - Add `func forgetPairedDevice()` — clears in-memory + persistence + cancels any in-flight transfer.
  - Add `func beginPairingFlow()` — `pairingState = .instructions`.
- **Red tests** — `AppModelPairingStateTests`:
  - `test_appModel_loadsPairedPeripheralOnInit_whenStored` — pre-seed a `CompanionPersistence` instance against a suite-scoped `UserDefaults`, instantiate `AppModel(persistence:)` with it, assert `appModel.pairedPeripheral` matches.
  - `test_appModel_pairedPeripheralIsNilWhenNothingStored` — fresh persistence; assert `nil`.
  - `test_forgetPairedDevice_clearsBothMemoryAndPersistence` — start seeded; call `forgetPairedDevice()`; assert both `appModel.pairedPeripheral == nil` and a freshly-loaded persistence sees `nil`.
  - `test_beginPairingFlow_setsPairingStateInstructions` — assert `pairingState == .instructions`.

### A.4 Fast-path reconnect via `retrievePeripherals`

- **File**: `CoreBluetoothRouteSyncClient.swift` — new `func connectToPairedPeripheral(identifier: String) async throws -> String`:
  1. `try await ensurePoweredOn()`.
  2. `guard let uuid = UUID(uuidString: identifier) else { throw .noDiscoveredPeripheral }`.
  3. `let peripherals = centralManager.retrievePeripherals(withIdentifiers: [uuid])`.
  4. If empty → `throw .noDiscoveredPeripheral` (caller will fall back to scan).
  5. Else, run the same `connect → discoverServices → discoverCharacteristics → setNotifyValue` chain as `connectToScannedPeripheral`. Refactor the post-`didConnect` continuation block into a single `private func awaitServicesAndSubscribe(...)` shared with the scan path so the two flows can't drift.
  - Add the same to the `RouteSyncBluetoothClient` protocol (A.1).
- **File**: `BleRouteSyncService.swift` — mirror `func connectToPairedPeripheral(identifier: String) async`.
- **File**: `AppModel.swift` — change `connectToDevice()`:
  ```swift
  if let paired = pairedPeripheral {
      await bleService.connectToPairedPeripheral(identifier: paired.identifier)
      if bleService.sessionState.connectionState != .connected {
          await bleService.scanForDevices()
          await bleService.connectToLastKnownDevice()
      }
  } else {
      await bleService.scanForDevices()
      await bleService.connectToLastKnownDevice()
  }
  ```
- **Red tests** — `BleRouteSyncServiceFastPathTests`:
  - `test_fastPath_skipsScan_whenPairedIdentifierKnown` — fake's `connectToPairedPeripheral` returns success; call `service.connectToPairedPeripheral(identifier: storedUUID)`; asserts `fake.scanCallCount == 0` AND `fake.connectToPairedCallCount == 1` AND `fake.lastConnectedIdentifier == storedUUID`.
  - `test_appModel_connectToDevice_usesFastPathWhenPaired` — `AppModel` with a seeded paired record; call `connectToDevice()`; asserts `fake.scanCallCount == 0`, `fake.connectToPairedCallCount == 1`. End-to-end coverage that `pairedPeripheral != nil → fast path` lives in `AppModel`, not just the service.
  - `test_appModel_fallsBackToScan_whenNoPairedIdentifier` — no record; expects `scanCallCount == 1`, `connectToPairedCallCount == 0`.
  - `test_appModel_fallsBackToScan_whenRetrieveFindsNoPeripheral` — fake's `connectToPairedPeripheral` throws `.noDiscoveredPeripheral`; expects `scanCallCount == 1` after fallback. This is the iOS-bond-store-empty case after a fresh install.

### A.5 `DeviceStatusChip` on Home

- **State derivation** (cross-platform contract):
  - `nil, _` → `.unpaired` (grey "+", taps `appModel.beginPairingFlow()`)
  - `record, .scanning|.connecting` → `.connecting(name)` (non-interactive, spinner)
  - `record, .connected` → `.connected(name)` (taps show popover with route info)
  - `record, .disconnected` → `.pairedDisconnected(name)` (taps `appModel.connectToDevice()`)
- **New file**: `companion-ios/CompanionApp/Features/Home/DeviceStatusChip.swift` — pure SwiftUI view parameterized over `enum DeviceChipState` so it's snapshot-friendly. Use SF Symbols: `plus.circle` (unpaired, secondary tint), `bolt.horizontal.circle` (paired/disconnected, outlined), the same with `ProgressView` overlay (connecting), `bolt.horizontal.circle.fill` (connected, accent tint).
- **File**: `companion-ios/CompanionApp/Features/Home/CompanionHomeView.swift` — place the chip next to the settings gear in `topBar`, in all three top-overlay variants (planning, phoneGuidance, deviceOverview). 50×50 frame to match `zoomButton`.
- **File**: `companion-ios/CompanionApp/Features/Home/HomeViewModel.swift` — derived computed var:
  ```swift
  var deviceChipState: DeviceChipState {
      switch (appModel.pairedPeripheral, appModel.bleService.sessionState.connectionState) {
      case (nil, _): return .unpaired
      case (let p?, .scanning), (let p?, .connecting): return .connecting(name: p.friendlyName)
      case (let p?, .connected): return .connected(name: p.friendlyName)
      case (let p?, .disconnected): return .pairedDisconnected(name: p.friendlyName)
      }
  }
  func handleDeviceChipTap() {
      switch deviceChipState {
      case .unpaired: appModel.beginPairingFlow()
      case .pairedDisconnected: Task { await appModel.connectToDevice() }
      case .connecting: break // disabled
      case .connected: showConnectionPopover = true
      }
  }
  ```
- **Red tests** — `DeviceStatusChipStateTests`:
  - `test_unpaired_whenNoRecord` — covers all `connectionState` values; expect `.unpaired`.
  - `test_pairedDisconnected_whenRecordButStateDisconnected` — record present; assert `.pairedDisconnected(name)` and `name == record.friendlyName`.
  - `test_connecting_whenScanning` and `test_connecting_whenConnecting` — both map to the same chip state. Two separate tests so a future enum split (e.g. adding `.discovering`) doesn't silently re-route one to the wrong chip.
  - `test_connected_whenConnectedAndRecordPresent` — `.connected(name)`.
  - `test_chipTapAction_dispatchesCorrectAction` — for each of the 4 states, calling `handleDeviceChipTap` invokes the right `AppModel` method (verify via the fake or via a mock `AppModel` subclass with override-and-record). Catches accidentally-swapped tap actions.
- **UX**: VoiceOver labels per state ("No device paired, tap to pair" / "Device <name> disconnected, tap to reconnect" / "Connecting to <name>" / "Connected to <name>"). Tap target ≥44 pt. Connecting state non-interactive (`.disabled(true)`).

### A.6 `DeviceSettingsView` rework

- **File**: `companion-ios/CompanionApp/Features/Device/DeviceSettingsView.swift` — new top section "Paired device":
  - If `pairedPeripheral == nil`: "Pair a new device" CTA → `appModel.beginPairingFlow()`.
  - Else: name + last-paired date; primary "Connect"/"Disconnect" button; destructive "Forget paired device" with confirmation alert.
- Keep Transfer + Diagnostics sections.
- **Red tests** — `DeviceSettingsForgetTests`:
  - `test_forgetButton_clearsPersistedPairing` — start with seeded record, drive `appModel.forgetPairedDevice()` (the action the alert's destructive button calls); assert both `appModel.pairedPeripheral == nil` AND `persistence.loadPairedPeripheral() == nil`.
  - `test_pairedDeviceSection_descriptorWhenUnpaired` — state-only test using a derived `DeviceSettingsSectionDescriptor` helper (avoids `ViewInspector` dep): descriptor is `.callToAction("Pair a new device")`.
  - `test_pairedDeviceSection_descriptorWhenPaired` — `.detail(name: "ESP32 Bike Minimap", lastPairedAt: ..., primaryAction: .connect | .disconnect)`.
  - `test_pairedDeviceSection_descriptorWhenPairedAndConnected` — primary action is `.disconnect`.
- **UX copy** (verbatim parity with Android): alert title `"Forget this device?"`, body `"You'll need to scan the pairing code again to use it. Your route history stays."`, action `"Forget"`, dismiss `"Cancel"`.

---

## Phase B — Pairing flow

### B.1 `Info.plist` — `NSCameraUsageDescription`

- **File**: `companion-ios/CompanionApp/Info.plist` — add key `NSCameraUsageDescription` with value `"Used to scan the pairing code shown by your ESP32 Bike Minimap device."`.
- **Red test**: `InfoPlistPermissionsTests.test_infoPlist_includesCameraUsageDescription` — `Bundle.main.object(forInfoDictionaryKey: "NSCameraUsageDescription")`; assert non-nil, non-empty, contains substring `"ESP32"` (catches the App-Review-fail "generic copy" case).

### B.2 `PairingQrPayload` decoder

- **New file**: `companion-ios/CompanionApp/Integration/BLE/PairingQrPayload.swift`:
  ```swift
  struct PairingQrPayload: Equatable {
      let peripheralIdentifier: String  // from id_ios
      let ephemeralSecret: Data
      let firmwareVersion: String?
      static func decode(_ string: String) throws -> PairingQrPayload
  }
  enum PairingQrError: LocalizedError {
      case unsupportedVersion(Int)
      case missingField(String)
      case malformedField(String)
  }
  ```
- Use a private internal Codable struct matching the wire format (`v`, `id_ios`, `id_android`, `secret`, `fw`); decode via `JSONDecoder` and map to `PairingQrPayload`.
- **Red tests** — `PairingQrPayloadTests`:
  - `test_decode_validV1Payload` — happy path; secret round-trips through base64.
  - `test_decode_rejectsMissingSecret` — JSON without `secret` → throws `.missingField("secret")`. Specific error so the UI can surface a useful message.
  - `test_decode_rejectsMalformedSecret` — `secret: "not-base64!@#"` → `.malformedField("secret")`.
  - `test_decode_rejectsMalformedUuid` — `id_ios: "not-a-uuid"` → `.malformedField("id_ios")`.
  - `test_decode_rejectsUnsupportedVersion` — `v: 99` → `.unsupportedVersion(99)`.
  - `test_decode_acceptsMissingFirmwareVersion` — optional field; decoder still succeeds.
  - `test_decode_decodesParityFixture` — reads `parity-fixtures/data/pairing_qr_v1.json` directly; asserts every field matches the documented values. Cross-platform contract test.

### B.3 `PairingFlowView` with camera capture

- **New file**: `companion-ios/CompanionApp/Features/Pairing/PairingFlowView.swift` — SwiftUI sheet driven by `appModel.pairingState`. Three steps:
  1. Instructions: "Hold your phone so the device's screen is in frame, then tap Start when you see the QR code."
  2. Camera capture with crosshair overlay framing the center 70% of preview.
  3. Connecting/confirming progress + result.
- **New file**: `companion-ios/CompanionApp/Features/Pairing/PairingCameraController.swift` — `UIViewRepresentable` wrapping a `UIView` that hosts an `AVCaptureVideoPreviewLayer`. Owns `AVCaptureSession` + `AVCaptureMetadataOutput` with `metadataObjectTypes = [.qr]`. Calls a closure when a metadata object decodes to a parseable `PairingQrPayload`.
- **New protocol**: `QrCaptureSession` test seam. Production: `AVFoundationQrCaptureSession` wrapping the AVFoundation pipeline. Test: `FakeQrCaptureSession` that exposes `simulateScan(_ rawString: String)` and a `tearDownCallCount`.
- **New file**: `companion-ios/CompanionApp/Features/Pairing/PairingFlowViewModel.swift` — `@MainActor ObservableObject` consumed by the view; takes the protocol, produces the state machine. Tests drive this directly without UIKit.
- **Permission flow**: on Step 2 enter, check `AVCaptureDevice.authorizationStatus(for: .video)`. If `.notDetermined`, request via `AVCaptureDevice.requestAccess(for:)`. If `.denied`, show "Open Settings" card → `UIApplication.shared.open(URL(string: UIApplication.openSettingsURLString)!)`.
- **Red tests** — `PairingFlowViewModelTests`:
  - `test_qrCallback_validPayload_advancesPairingStateToConnecting` — fake fires `simulateScan(validJson)`; assert `viewModel.pairingState == .connecting` and `viewModel.lastDecodedPayload?.peripheralIdentifier == validPayload.peripheralIdentifier`.
  - `test_qrCallback_invalidPayload_setsHumanReadableError` — fake fires `simulateScan("garbage")`; assert state stays `.scanning` and `viewModel.scanErrorMessage` is set to a localized string.
  - `test_threeConsecutiveInvalidScans_promptCenterOnQr` — fake fires 3 invalid scans; assert `viewModel.scanGuidance == .centerOnQr`.
  - `test_cancel_inAnyStep_returnsToIdleAndTearsDownSession` — for each of the 3 steps, dispatch cancel; assert `appModel.pairingState == .idle` and `fakeSession.tearDownCallCount == 1`.
  - `test_cameraPermissionDenied_showsOpenSettingsAction` — permission stub returns `.denied`; assert `viewModel.permissionDescriptor == .denied(.openSettings)`.

### B.4 Confirm-and-persist write flow

- **File**: `companion-ios/CompanionApp/Integration/BLE/CoreBluetoothRouteSyncClient.swift`:
  - Add `func connectToAdvertisedPeripheral(identifier: String) async throws -> String` — first tries `retrievePeripherals(withIdentifiers:)`, falls back to a *targeted* scan filtered by `peripheral.identifier.uuidString == identifier` (this is the iOS-fresh-install case where the OS bond store is empty and `retrievePeripherals` returns empty until after the first cached connect). Reuse the post-`didConnect` helper from A.4.
  - Add `func writePairingConfirm(secret: Data) async throws` — writes to the new `…-1004` UUID with `.withResponse`; error-maps as `.writeFailed(...)`.
  - Add both methods to `RouteSyncBluetoothClient`.
- **File**: `companion-ios/CompanionApp/Integration/BLE/BleRouteSyncPacket.swift` — extend `BleRouteSyncGattContract` with `static let pairingConfirmCharacteristicUUID = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1004"`.
- **File**: `companion-ios/CompanionApp/Integration/BLE/BleRouteSyncService.swift` — mirror `connectToAdvertisedPeripheral` and `writePairingConfirm`.
- **File**: `companion-ios/CompanionApp/App/AppModel.swift`:
  ```swift
  func completePairing(payload: PairingQrPayload) async {
      pairingState = .connecting
      do {
          _ = try await bleService.connectToAdvertisedPeripheral(identifier: payload.peripheralIdentifier)
          pairingState = .confirming
          try await bleService.writePairingConfirm(secret: payload.ephemeralSecret)
          let record = PairedPeripheralRecord(
              identifier: payload.peripheralIdentifier,
              friendlyName: bleService.sessionState.lastDeviceName ?? "ESP32 Bike Minimap",
              pairedAt: Date()
          )
          persistence.savePairedPeripheral(record)
          pairedPeripheral = record
          pairingState = .succeeded
          // Auto-dismiss after 1.5s
          try? await Task.sleep(nanoseconds: 1_500_000_000)
          pairingState = .idle
      } catch {
          pairingState = .failed(error.localizedDescription)
      }
  }
  ```
- **Red tests** — `PairingConfirmFlowTests`:
  - `test_completePairing_writesSecretAndPersistsRecord` — happy path; assert `fake.connectToAdvertisedCallCount == 1`, `fake.lastWrittenPairingSecret == payload.ephemeralSecret`, `persistence.loadPairedPeripheral()?.identifier == payload.peripheralIdentifier`, `pairingState == .succeeded` (then transitions to `.idle` after 1.5s — see next test).
  - `test_completePairing_doesNotPersistWhenConnectFails` — fake throws on connect; assert `persistence.loadPairedPeripheral() == nil` AND `pairingState == .failed(...)` (no half-state).
  - `test_completePairing_doesNotPersistWhenWriteFails` — connect succeeds, write throws; same no-half-state assertion.
  - `test_completePairing_overwritesPriorPairedRecord` — pre-seed record A; complete pairing for B; assert persistence holds B (single-bond at the persistence layer; firmware enforces it on the device side too).
  - `test_completePairing_clearsPairingStateAfterAutoDismissDelay` — happy path with `Task.sleep` mocked or scheduler advanced 1.5s; assert `pairingState == .idle`.

---

## Implementation order + dependencies

```
A.1 (protocol) ─┬─► A.4 (fast path) ─┐
                │                     │
A.2 (persist) ──► A.3 (AppModel) ────► A.5 (chip) ─► A.6 (settings) ─┐
                                                                     │
B.1 (Info.plist) ─► B.2 (codec) ─► B.3 (PairingFlowView) ────────────┴─► B.4 (confirm flow)
```

A.1 + A.2 can land in parallel; A.3 depends on both. A.4 and A.5+A.6 land independently after A.3. Phase B can start after A.3 (specifically, `pairingState` must exist before `PairingFlowView` can drive it).

## Test count summary

- A.1 plumbing: 1
- A.2 persistence: 5 (incl. parity-fixture decode)
- A.3 app model: 4
- A.4 fast path: 4
- A.5 chip: 5 (parameterized)
- A.6 settings: 4
- B.1 Info.plist: 1
- B.2 codec: 7 (incl. parity-fixture decode)
- B.3 pairing flow VM: 5
- B.4 confirm flow: 5

Total ~41 new tests. Each pinned to one observable behavior.

## Verification

After each step:
```bash
xcodegen generate --project companion-ios
xcodebuild test \
  -project companion-ios/ESP32MapCompanion.xcodeproj \
  -scheme ESP32MapCompanion \
  -destination 'platform=iOS Simulator,name=iPhone 16'
```
Confirm the new test you wrote fails first, then implement, then it passes alongside the existing suite.

After Phase A:
- All A.* tests green.
- Manual smoke on a real iPhone: install, see chip in unpaired state, tap → pairing screen opens (empty until B.3); tap Reconnect on a stored bond uses fast path (no scanning UI flicker).

After Phase B:
- All B.* tests green.
- The two parity-fixture decode tests (A.2 and B.2) pass — they're the cross-platform contract.

End-to-end on hardware (this requires the firmware-side pairing changes from the sibling plan to be flashed on the device — coordinate with the Linux agent before this step):
1. Flash device, power on. QR appears within 5s.
2. Open companion → tap unpaired chip → camera opens → scan QR. Pairing succeeds within ~5s; sheet auto-dismisses; chip flips to connected.
3. Push a route via Start. Device renders route over the now-encrypted BLE link.
4. Lock the phone, walk out of range, return. Companion auto-uses fast path on next connect; no scan dialog.
5. Tap "Forget paired device". Device drops bond, returns to QR. Re-pair flow works.
6. Pair to iPhone, "Forget" from iPhone, then pair to Android phone. Verify both companions parse the same QR correctly (the parity fixture decode tests catch this in CI; the hardware run is the integration check).

## Pushbacks worth considering

- **Reuse SMP OOB pairing instead of the custom `pairing_confirm` characteristic?** More correct, less iOS-friendly; CoreBluetooth's OOB pairing API is awkward and mostly works against AppleTV-class peripherals. Stick with Just Works pairing (link-layer encryption) + the OOB-confirm-over-an-encrypted-characteristic pattern. Document this trade-off in code comments.
- **Auto-reconnect timer on disconnect.** Out of scope for this plan; manual reconnect via the chip is the current path. If you find yourself wanting to add it for UX polish, defer to a follow-up.

## When you're done

1. Confirm `xcodebuild test` is green for the full ESP32MapCompanion test target.
2. Run `xcodebuild build` for the Release configuration to confirm no warnings/errors that would fail App Review (especially the camera permission strings).
3. **Delete `docs/_plan-ios-pairing.md` in the same PR** that lands the implementation — this file is transient.
4. Update `docs/companion-app-architecture.md` and `docs/ble-route-sync-contract.md` per the iOS-side bullets there (the Linux-side agent owns the firmware/Android sections of those docs).
