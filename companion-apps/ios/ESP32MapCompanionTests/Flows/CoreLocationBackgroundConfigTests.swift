import CoreLocation
import XCTest
@testable import ESP32MapCompanion

/// Pins the CoreLocation manager's background-readiness configuration so iOS
/// does not aggressively pause GPS when the rider locks the phone mid-ride.
///
/// Two flags matter beyond `allowsBackgroundLocationUpdates`:
///   - `pausesLocationUpdatesAutomatically` defaults to `true`. iOS uses
///     motion heuristics to decide the phone is "stopped" and silently
///     stops updates — fatal for cycling, where headwind / coasting
///     looks like "not moving" to the OS.
///   - `activityType` defaults to `.other`. Setting it to
///     `.otherNavigation` tells iOS this is a turn-by-turn use case
///     and exempts it from pause heuristics.
///
/// Dynamic accuracy: planning mode uses `kCLLocationAccuracyBest` (good
/// enough for the rider dot, lighter on battery). Navigation mode switches
/// to `kCLLocationAccuracyBestForNavigation` + `kCLDistanceFilterNone` to
/// keep the GPS radio fully awake in background — matching the pattern used
/// by OsmAnd and OwnTracks for reliable background navigation.
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

    func test_defaultAccuracy_isGpsGrade() {
        // Planning mode uses kCLLocationAccuracyBest so the rider dot is
        // accurate on the map. The old kCLLocationAccuracyHundredMeters
        // default caused iOS to skip the GPS radio entirely in background,
        // making location updates unpredictable mid-ride.
        let svc = makeService()
        XCTAssertEqual(
            svc.manager.desiredAccuracy,
            kCLLocationAccuracyBest,
            "planning-mode default must be kCLLocationAccuracyBest — 100 m accuracy skips the GPS radio in background"
        )
    }

    func test_setNavigationAccuracy_true_usesFullGps() {
        let svc = makeService()
        svc.setNavigationAccuracy(true)
        XCTAssertEqual(
            svc.manager.desiredAccuracy,
            kCLLocationAccuracyBestForNavigation,
            "navigation mode must use kCLLocationAccuracyBestForNavigation to keep the GPS radio awake in background"
        )
        XCTAssertEqual(
            svc.manager.distanceFilter,
            kCLDistanceFilterNone,
            "navigation mode must deliver every fix (kCLDistanceFilterNone) so reroute detection is not gated by distance"
        )
    }

    func test_setNavigationAccuracy_false_revertsToPlanning() {
        let svc = makeService()
        svc.setNavigationAccuracy(true)
        svc.setNavigationAccuracy(false)
        XCTAssertEqual(
            svc.manager.desiredAccuracy,
            kCLLocationAccuracyBest,
            "returning to planning mode must restore kCLLocationAccuracyBest"
        )
        XCTAssertEqual(
            svc.manager.distanceFilter, 10,
            "returning to planning mode must restore the 10 m distance filter"
        )
    }
}
