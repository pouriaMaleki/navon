import XCTest
@testable import Navon

/// L2 start/stop guidance tests (plan flows #43, #44).
@MainActor
final class PlanningSessionTests: XCTestCase {

    private func straightLinePackage() -> NormalizedRoutePackage {
        let origin = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
        let destination = CoordinatePoint(latitude: 60.1921, longitude: 24.9458)
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "osm-straight",
            revision: 1,
            geometry: [origin, destination],
            maneuvers: [
                RouteManeuver(
                    id: "m1",
                    maneuverType: .depart,
                    location: origin,
                    distanceFromStartMeters: 0,
                    distanceToNextMeters: 2500,
                    instructionText: "Depart"
                ),
                RouteManeuver(
                    id: "m2",
                    maneuverType: .arrive,
                    location: destination,
                    distanceFromStartMeters: 2500,
                    distanceToNextMeters: nil,
                    instructionText: "Arrive"
                )
            ],
            summary: RouteSummary(
                totalDistanceMeters: 2500,
                estimatedDurationSeconds: 600,
                startLabel: nil,
                destinationLabel: nil
            ),
            provenance: RouteProvenance(
                providerID: .osm,
                sourceReference: nil,
                generatedAtUnixMs: 0
            )
        )
    }

    func test_startSelectedRoute_enterPhoneGuidanceMode() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = straightLinePackage()
        app.preview = RoutePreviewModel(
            alternatives: [
                RouteAlternative(
                    id: UUID(),
                    title: "Route 1",
                    subtitle: "",
                    distanceMeters: 2500,
                    durationSeconds: 600,
                    normalizedPackage: pkg
                )
            ],
            selectedAlternativeID: nil,
            routeIdentifier: nil,
            routeRevision: nil,
            planningNotice: nil
        )
        await vm.startSelectedRoute()
        XCTAssertEqual(vm.homeMode, .phoneGuidance)
        XCTAssertEqual(app.sessionManager.session.routeIdentifier, "osm-straight")
    }

    func test_stopGuidance_returnsToPlanning() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = straightLinePackage()
        app.preview = RoutePreviewModel(
            alternatives: [
                RouteAlternative(
                    id: UUID(),
                    title: "Route 1",
                    subtitle: "",
                    distanceMeters: 2500,
                    durationSeconds: 600,
                    normalizedPackage: pkg
                )
            ],
            selectedAlternativeID: nil,
            routeIdentifier: nil,
            routeRevision: nil,
            planningNotice: nil
        )
        await vm.startSelectedRoute()
        vm.stopActiveNavigation()
        // stopActiveNavigation schedules the state change in a detached Task.
        try? await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertEqual(vm.homeMode, .planning)
    }
}
