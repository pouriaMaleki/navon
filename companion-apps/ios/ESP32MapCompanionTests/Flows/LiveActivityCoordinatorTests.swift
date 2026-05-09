import XCTest
@testable import ESP32MapCompanion

@MainActor
final class LiveActivityCoordinatorTests: XCTestCase {

    private func settings(liveActivity: Bool = true, bgGps: Bool = true) -> CompanionSettings {
        var s = CompanionSettings.defaults
        s.liveActivityEnabled = liveActivity
        s.allowBackgroundGps = bgGps
        return s
    }

    private func route(id: String = "r1", totalM: Double = 1000) -> NormalizedRoutePackage {
        NormalizedRoutePackage(
            version: .current,
            routeIdentifier: id,
            revision: 1,
            geometry: [.init(latitude: 0, longitude: 0), .init(latitude: 0.001, longitude: 0)],
            maneuvers: [
                RouteManeuver(id: "m0", maneuverType: .depart,
                              location: .init(latitude: 0, longitude: 0),
                              distanceFromStartMeters: 0,
                              distanceToNextMeters: nil, instructionText: nil),
                RouteManeuver(id: "m1", maneuverType: .left,
                              location: .init(latitude: 0, longitude: 0),
                              distanceFromStartMeters: 200,
                              distanceToNextMeters: nil, instructionText: nil),
                RouteManeuver(id: "m2", maneuverType: .arrive,
                              location: .init(latitude: 0, longitude: 0),
                              distanceFromStartMeters: totalM,
                              distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: totalM, estimatedDurationSeconds: 600,
                                  startLabel: nil, destinationLabel: nil),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    private func tick(
        _ c: LiveActivityCoordinator,
        settings: CompanionSettings,
        isRouting: Bool = true,
        route r: NormalizedRoutePackage? = nil,
        progress: Double = 50,
        offRoute: Bool = false,
        rerouting: Bool = false,
        arrived: Bool = false
    ) {
        c.onGuidanceTick(
            settings: settings,
            isRouting: isRouting,
            route: r,
            progressDistanceM: progress,
            offRoute: offRoute,
            rerouting: rerouting,
            arrived: arrived,
            isImperial: false
        )
    }

    func test_startsActivityWhenRoutingBeginsAndSettingsAllow() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        let r = route()
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: r)
        XCTAssertEqual(driver.startedRouteIds, ["r1"])
        XCTAssertNil(driver.endedRouteIds.first)
    }

    func test_doesNotStartWhenLiveActivitySettingOff() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        coord.onSettingsOrRoutingChange(
            settings: settings(liveActivity: false), isRouting: true, route: route()
        )
        XCTAssertTrue(driver.startedRouteIds.isEmpty)
    }

    func test_doesNotStartWhenAllowBackgroundGpsOff() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        coord.onSettingsOrRoutingChange(
            settings: settings(bgGps: false), isRouting: true, route: route()
        )
        XCTAssertTrue(driver.startedRouteIds.isEmpty)
    }

    func test_doesNotStartWhenNotRouting() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: false, route: route())
        XCTAssertTrue(driver.startedRouteIds.isEmpty)
    }

    func test_doesNotDoubleStartWhenAlreadyActive() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        let r = route()
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: r)
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: r)
        XCTAssertEqual(driver.startedRouteIds.count, 1)
    }

    func test_updatesActivityOnGuidanceTickWithDerivedContentState() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        let r = route()
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: r)
        tick(coord, settings: settings(), route: r, progress: 50)
        XCTAssertEqual(driver.updates.count, 1)
        XCTAssertEqual(driver.updates.last?.glyph, .left)
        XCTAssertEqual(driver.updates.last?.distanceToNextM ?? 0, 150, accuracy: 0.01) // 200-50
    }

    func test_endsWhenRoutingStops() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        let r = route()
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: r)
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: false, route: nil)
        XCTAssertEqual(driver.endedRouteIds, ["r1"])
    }

    func test_endsWhenLiveActivityToggledOffMidRoute() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        let r = route()
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: r)
        coord.onSettingsOrRoutingChange(
            settings: settings(liveActivity: false), isRouting: true, route: r
        )
        XCTAssertEqual(driver.endedRouteIds, ["r1"])
    }

    func test_endsAndRestartsWhenRouteIdChanges() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: route(id: "r1"))
        coord.onSettingsOrRoutingChange(settings: settings(), isRouting: true, route: route(id: "r2"))
        XCTAssertEqual(driver.endedRouteIds, ["r1"])
        XCTAssertEqual(driver.startedRouteIds, ["r1", "r2"])
    }

    func test_doesNotUpdateWhenGateClosed() {
        let driver = FakeLiveActivityDriver()
        let coord = LiveActivityCoordinator(driver: driver)
        tick(coord, settings: settings(liveActivity: false), route: route())
        XCTAssertTrue(driver.updates.isEmpty)
    }
}
