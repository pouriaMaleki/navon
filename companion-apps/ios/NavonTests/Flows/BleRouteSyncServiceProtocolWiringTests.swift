import XCTest
@testable import Navon

@MainActor
final class BleRouteSyncServiceProtocolWiringTests: XCTestCase {
    func test_serviceAcceptsFakeBluetoothClient() {
        let fake = FakeRouteSyncBluetoothClient()
        let service = BleRouteSyncService(bluetoothClient: fake)
        XCTAssertEqual(service.sessionState.connectionState, .disconnected)
        XCTAssertEqual(service.sessionState.routeSyncState, .idle)
    }
}
