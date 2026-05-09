import XCTest
@testable import ESP32MapCompanion

@MainActor
final class BleRouteSyncServiceProtocolWiringTests: XCTestCase {
    func test_serviceAcceptsFakeBluetoothClient() {
        let fake = FakeRouteSyncBluetoothClient()
        let service = BleRouteSyncService(bluetoothClient: fake)
        XCTAssertEqual(service.sessionState.connectionState, .disconnected)
        XCTAssertEqual(service.sessionState.routeSyncState, .idle)
    }
}
