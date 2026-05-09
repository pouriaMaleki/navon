import XCTest
@testable import ESP32MapCompanion

/// User-reported issue: even when the app was idle (no route in progress)
/// and put in the background, it kept consuming GPS. We honour the user's
/// permission grant only WHILE there is something to do — i.e. either the
/// app is in foreground OR a route is actively routing. Otherwise we stop
/// listening to save battery.
///
/// We assert against `locationService.watching` (set/cleared by
/// `start()`/`stop()`) instead of `isLocating` because the simulator's
/// authorization status is non-deterministic between runs and `isLocating`
/// can flip false synchronously inside `start()` if the auth callback
/// returns `.denied`.
@MainActor
final class BackgroundLocationGatingTests: XCTestCase {

    private func tinyRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegLat,
            longitude: 24.94
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "bg-route",
            revision: 1,
            geometry: [start, mid],
            maneuvers: [
                RouteManeuver(id: "depart", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "arrive", maneuverType: .arrive, location: mid,
                              distanceFromStartMeters: 400, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 400, estimatedDurationSeconds: 120,
                                  startLabel: nil, destinationLabel: "Park"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    func test_locationStops_whenAppGoesToBackground_andNoRouteIsActive() {
        let app = AppModel()
        XCTAssertTrue(
            app.locationService.watching,
            "Sanity: AppModel.init starts the location service so the planner has a rider position to bias suggestions against."
        )
        app.handleApplicationLifecycleEnteredBackground()
        XCTAssertFalse(
            app.locationService.watching,
            "Going to background without an active route must stop GPS to preserve battery — even if the user granted Always permission."
        )
    }

    func test_locationKeepsListening_whenAppGoesToBackground_duringActiveRoute() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 400, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        XCTAssertTrue(app.isRoutingInProgress)

        app.handleApplicationLifecycleEnteredBackground()
        XCTAssertTrue(
            app.locationService.watching,
            "An active route in the background must keep GPS running — that is the whole point of the Allow GPS in background permission."
        )
    }

    func test_locationResumes_whenAppReturnsToForeground() {
        let app = AppModel()
        app.handleApplicationLifecycleEnteredBackground()
        XCTAssertFalse(app.locationService.watching)
        app.handleApplicationLifecycleEnteredForeground()
        XCTAssertTrue(
            app.locationService.watching,
            "Returning to foreground must resume GPS so the where-to bar and recenter button work again."
        )
    }
}
