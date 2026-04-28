import XCTest
@testable import ESP32MapCompanion

@MainActor
final class PairingConfirmFlowTests: XCTestCase {

    /// CoreBluetooth identifier captured at connect time (not from the QR —
    /// see `PairingQrPayload`). Tests pin this to a known UUID so we can
    /// assert the persisted record matches what the connect path returned.
    private let connectedIdentifier = "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A"

    private func samplePayload() -> PairingQrPayload {
        // 32 bytes of 0x42 — matches the canonical parity fixture.
        PairingQrPayload(
            ephemeralSecret: Data(repeating: 0x42, count: 32),
            firmwareVersion: "0.1.0",
            androidIdentifier: "AA:BB:CC:DD:EE:FF"
        )
    }

    private func samplePairedRecord(identifier: String) -> PairedPeripheralRecord {
        PairedPeripheralRecord(
            identifier: identifier,
            friendlyName: "Existing",
            pairedAt: CompanionPersistence.iso8601MillisFormatter.date(from: "2026-04-28T12:34:56.789Z")!
        )
    }

    private func makeHarness(
        connectResult: Result<ConnectedPeripheralInfo, Error> = .success(
            ConnectedPeripheralInfo(name: "ESP32 Bike Minimap", identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A")
        ),
        writeResult: Result<Void, Error> = .success(())
    ) -> (AppModel, FakeRouteSyncBluetoothClient, CompanionPersistence) {
        let defaults = UserDefaults(suiteName: "pairing-confirm-tests-\(UUID().uuidString)")!
        let persistence = CompanionPersistence(defaults: defaults)
        let fake = FakeRouteSyncBluetoothClient()
        fake.connectToAdvertisedResult = connectResult
        fake.writePairingConfirmResult = writeResult
        let service = BleRouteSyncService(bluetoothClient: fake)
        let appModel = AppModel(persistence: persistence, bleService: service)
        return (appModel, fake, persistence)
    }

    func test_completePairing_writesSecretAndPersistsRecord() async {
        let (appModel, fake, persistence) = makeHarness()

        await appModel.completePairing(payload: samplePayload())

        XCTAssertEqual(fake.connectToAdvertisedCallCount, 1)
        XCTAssertEqual(fake.writePairingConfirmCallCount, 1)
        XCTAssertEqual(fake.lastWrittenPairingSecret, samplePayload().ephemeralSecret)
        // Persisted identifier comes from CoreBluetooth at connect time, not
        // from the QR — that is the cross-platform contract for iOS.
        XCTAssertEqual(persistence.loadPairedPeripheral()?.identifier, connectedIdentifier)
        XCTAssertEqual(appModel.pairedPeripheral?.identifier, connectedIdentifier)
        XCTAssertEqual(appModel.pairingState, .idle)
    }

    func test_completePairing_doesNotPersistWhenConnectFails() async {
        let (appModel, _, persistence) = makeHarness(
            connectResult: .failure(CoreBluetoothRouteSyncError.scanTimedOut)
        )

        await appModel.completePairing(payload: samplePayload())

        XCTAssertNil(persistence.loadPairedPeripheral())
        XCTAssertNil(appModel.pairedPeripheral)
        if case .failed = appModel.pairingState {
            // expected
        } else {
            XCTFail("expected pairingState to be .failed, got \(appModel.pairingState)")
        }
    }

    func test_completePairing_doesNotPersistWhenWriteFails() async {
        let (appModel, _, persistence) = makeHarness(
            writeResult: .failure(CoreBluetoothRouteSyncError.writeFailed("nope"))
        )

        await appModel.completePairing(payload: samplePayload())

        XCTAssertNil(persistence.loadPairedPeripheral())
        XCTAssertNil(appModel.pairedPeripheral)
        if case .failed = appModel.pairingState {
            // expected
        } else {
            XCTFail("expected pairingState to be .failed")
        }
    }

    func test_completePairing_overwritesPriorPairedRecord() async {
        let defaults = UserDefaults(suiteName: "pairing-confirm-overwrite-\(UUID().uuidString)")!
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(samplePairedRecord(identifier: "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA"))
        let fake = FakeRouteSyncBluetoothClient()
        let service = BleRouteSyncService(bluetoothClient: fake)
        let appModel = AppModel(persistence: persistence, bleService: service)
        XCTAssertEqual(appModel.pairedPeripheral?.identifier, "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA")

        await appModel.completePairing(payload: samplePayload())

        XCTAssertEqual(persistence.loadPairedPeripheral()?.identifier, connectedIdentifier)
        XCTAssertEqual(appModel.pairedPeripheral?.identifier, connectedIdentifier)
    }

    func test_completePairing_clearsPairingStateAfterAutoDismissDelay() async {
        let (appModel, _, _) = makeHarness()
        await appModel.completePairing(payload: samplePayload())
        XCTAssertEqual(appModel.pairingState, .idle)
    }
}
