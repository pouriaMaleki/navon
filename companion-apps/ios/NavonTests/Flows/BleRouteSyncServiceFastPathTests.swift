import XCTest
@testable import Navon

@MainActor
final class BleRouteSyncServiceFastPathTests: XCTestCase {

    private let storedUUID = "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A"

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "fast-path-tests-\(UUID().uuidString)")!
    }

    private func sampleRecord(identifier: String) -> PairedPeripheralRecord {
        PairedPeripheralRecord(
            identifier: identifier,
            friendlyName: "Navon",
            pairedAt: CompanionPersistence.iso8601MillisFormatter.date(from: "2026-04-28T12:34:56.789Z")!
        )
    }

    func test_fastPath_skipsScan_whenPairedIdentifierKnown() async {
        let fake = FakeRouteSyncBluetoothClient()
        fake.connectToPairedResult = .success("Navon")
        let service = BleRouteSyncService(bluetoothClient: fake)

        await service.connectToPairedPeripheral(identifier: storedUUID)

        XCTAssertEqual(fake.scanCallCount, 0)
        XCTAssertEqual(fake.connectToPairedCallCount, 1)
        XCTAssertEqual(fake.lastConnectedIdentifier, storedUUID)
        XCTAssertEqual(service.sessionState.connectionState, .connected)
    }

    func test_appModel_connectToDevice_usesFastPathWhenPaired() async {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(sampleRecord(identifier: storedUUID))
        let fake = FakeRouteSyncBluetoothClient()
        fake.connectToPairedResult = .success("Navon")
        let service = BleRouteSyncService(bluetoothClient: fake)
        let appModel = AppModel(persistence: persistence, bleService: service)

        // AppModel.init kicks off a background auto-reconnect Task when a
        // paired record is loaded. Drain it before measuring so the test
        // only asserts on the explicit `connectToDevice()` call below.
        try? await Task.sleep(nanoseconds: 50_000_000)
        let baselinePairedCalls = fake.connectToPairedCallCount
        let baselineScanCalls = fake.scanCallCount

        await appModel.connectToDevice()

        XCTAssertEqual(fake.scanCallCount - baselineScanCalls, 0)
        XCTAssertEqual(fake.connectToPairedCallCount - baselinePairedCalls, 1)
        XCTAssertEqual(fake.lastConnectedIdentifier, storedUUID)
    }

    func test_appModel_fallsBackToScan_whenNoPairedIdentifier() async {
        let persistence = CompanionPersistence(defaults: freshDefaults())
        let fake = FakeRouteSyncBluetoothClient()
        let service = BleRouteSyncService(bluetoothClient: fake)
        let appModel = AppModel(persistence: persistence, bleService: service)

        await appModel.connectToDevice()

        XCTAssertEqual(fake.scanCallCount, 1)
        XCTAssertEqual(fake.connectToPairedCallCount, 0)
    }

    func test_appModel_fallsBackToScan_whenRetrieveFindsNoPeripheral() async {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(sampleRecord(identifier: storedUUID))
        let fake = FakeRouteSyncBluetoothClient()
        fake.connectToPairedResult = .failure(CoreBluetoothRouteSyncError.noDiscoveredPeripheral)
        let service = BleRouteSyncService(bluetoothClient: fake)
        let appModel = AppModel(persistence: persistence, bleService: service)

        // The init-time auto-reconnect also fails (same fake) but does not
        // fall back to scan — that path is gated behind an explicit user
        // action. So drain the init Task, then assert only on the explicit
        // call.
        try? await Task.sleep(nanoseconds: 50_000_000)
        let baselinePairedCalls = fake.connectToPairedCallCount
        let baselineScanCalls = fake.scanCallCount

        await appModel.connectToDevice()

        XCTAssertEqual(fake.connectToPairedCallCount - baselinePairedCalls, 1)
        XCTAssertEqual(fake.scanCallCount - baselineScanCalls, 1)
    }
}
