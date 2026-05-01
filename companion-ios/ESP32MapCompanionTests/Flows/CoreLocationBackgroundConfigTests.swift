import CoreLocation
import XCTest
@testable import ESP32MapCompanion

/// User-reported (critical): "Route while backgrounded, reroute, show real
/// lock-screen map — none of which currently works." This file pins the
/// CoreLocation manager's background-readiness configuration so iOS does
/// not aggressively pause GPS when the rider locks the phone mid-ride.
///
/// Two flags matter beyond `allowsBackgroundLocationUpdates`:
///   - `pausesLocationUpdatesAutomatically` defaults to `true`. iOS uses
///     motion heuristics to decide the phone is "stopped" and silently
///     stops updates — fatal for cycling, where headwind / coasting
///     looks like "not moving" to the OS.
///   - `activityType` defaults to `.other`. Setting it to
///     `.otherNavigation` (or `.fitness`) tells iOS this is a
///     turn-by-turn use case and should be exempt from those pause
///     heuristics.
///
/// We also tighten the desiredAccuracy band — `kCLLocationAccuracyHundredMeters`
/// from the planning-mode default is too coarse for guidance.
@MainActor
final class CoreLocationBackgroundConfigTests: XCTestCase {

    private func makeService() -> CoreLocationService {
        let suiteName = "core-location-config-test-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        let persistence = CompanionPersistence(defaults: defaults)
        return CoreLocationService(persistence: persistence)
    }

    func test_pausesLocationUpdatesAutomatically_isOff() {
        // iOS must not silently pause GPS when its motion heuristics
        // decide the rider has "stopped" — cyclists coasting look
        // identical to a stopped phone to the OS.
        let svc = makeService()
        XCTAssertFalse(
            svc.manager.pausesLocationUpdatesAutomatically,
            "background routing requires `pausesLocationUpdatesAutomatically = false` so iOS doesn't silently stop updates mid-ride"
        )
    }

    func test_activityType_isOtherNavigation() {
        let svc = makeService()
        XCTAssertEqual(
            svc.manager.activityType, .otherNavigation,
            "navigation use case must be advertised so iOS spends battery accordingly and skips its motion-pause heuristics"
        )
    }

    func test_allowsBackgroundLocationUpdates_isOnByDefault() {
        // `allowsBackgroundLocationUpdates` is a no-op without
        // UIBackgroundModes=location AND .authorizedAlways granted, but
        // setting it eagerly is harmless on missing-permission builds and
        // saves us the race where the auth callback fires before `start()`.
        let svc = makeService()
        XCTAssertTrue(
            svc.manager.allowsBackgroundLocationUpdates,
            "manager.allowsBackgroundLocationUpdates must be true at init time so background GPS works the moment Always auth lands, without waiting for the next auth callback"
        )
    }
}
