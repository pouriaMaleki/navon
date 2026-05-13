import XCTest
@testable import Navon

@MainActor
final class PhoneGpsForwarderTests: XCTestCase {

    func test_forwarder_sendsCurrentSpeedWhenFreshFixExists() async {
        let ble = FakeRouteSyncBluetoothClient()
        let location = FakeLocationService()
        location.emitFix(latitude: 60.1700, longitude: 24.9400, speedMps: 3.6)
        let forwarder = PhoneGpsForwarder(bleClient: ble, locationService: location)

        forwarder.start(interval: 0.02, staleSpeedAfter: 0.20)
        try? await Task.sleep(nanoseconds: 90_000_000)
        forwarder.stop()

        XCTAssertFalse(ble.phoneGpsWrites.isEmpty, "expected at least one forwarded phone GPS sample")
        let lastSpeed = ble.phoneGpsWrites.last?.speed
        XCTAssertNotNil(lastSpeed)
        XCTAssertEqual(lastSpeed!, 3.6, accuracy: 0.0001)
    }

    func test_forwarder_zeroesSpeedWhenFixIsStale() async {
        let ble = FakeRouteSyncBluetoothClient()
        let location = FakeLocationService()
        location.emitFix(latitude: 60.1700, longitude: 24.9400, speedMps: 3.6)
        let forwarder = PhoneGpsForwarder(bleClient: ble, locationService: location)

        forwarder.start(interval: 0.02, staleSpeedAfter: 0.10)
        // No new fixes arrive. Forwarder should eventually stop replaying the
        // old moving speed and send zero instead.
        try? await Task.sleep(nanoseconds: 260_000_000)
        forwarder.stop()

        XCTAssertFalse(ble.phoneGpsWrites.isEmpty, "expected forwarded phone GPS samples")
        let staleSpeed = ble.phoneGpsWrites.last?.speed
        XCTAssertNotNil(staleSpeed)
        XCTAssertEqual(
            staleSpeed!,
            0.0,
            accuracy: 0.0001,
            "stale location fixes must not keep reporting stale moving speed"
        )
    }
}
