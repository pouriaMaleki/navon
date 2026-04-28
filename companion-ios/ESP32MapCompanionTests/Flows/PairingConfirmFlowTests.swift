import XCTest
@testable import ESP32MapCompanion

@MainActor
final class PairingConfirmFlowTests: XCTestCase {

    private let identifier = "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A"

    private func samplePayload() -> PairingQrPayload {
        PairingQrPayload(
            peripheralIdentifier: identifier,
            ephemeralSecret: Data((UInt8(0)...UInt8(31)).map { $0 }),
            firmwareVersion: "1.2.3"
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
        connectResult: Result<String, Error> = .success("ESP32 Bike Minimap"),
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
        XCTAssertEqual(persistence.loadPairedPeripheral()?.identifier, identifier)
        XCTAssertEqual(appModel.pairedPeripheral?.identifier, identifier)
        // After auto-dismiss, state collapses back to idle.
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

        XCTAssertEqual(persistence.loadPairedPeripheral()?.identifier, identifier)
        XCTAssertEqual(appModel.pairedPeripheral?.identifier, identifier)
    }

    func test_completePairing_clearsPairingStateAfterAutoDismissDelay() async {
        let (appModel, _, _) = makeHarness()
        await appModel.completePairing(payload: samplePayload())
        XCTAssertEqual(appModel.pairingState, .idle)
    }
}
