import XCTest
@testable import ESP32MapCompanion

/// Regression tests for the "explore alternatives while in active guidance" flow.
///
/// Spec: pressing the split icon during `.phoneGuidance` must NOT drop
/// `homeMode` to `.planning`. Instead:
///  - `homeMode` stays `.phoneGuidance` so guidance keeps running.
///  - `isExploringAlternativesFromGuidance` is `true` while the panel is open.
///  - `cancelAlternativesExploration()` dismisses the panel without touching routing.
///  - `startSelectedRoute()` commits a new route and clears the flag.
@MainActor
final class ExploreAlternativesFromGuidanceTests: XCTestCase {

    // MARK: - Helpers

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

    /// Creates an `AppModel` + `HomeViewModel` pair that already has an active
    /// guidance session running (i.e. `homeMode == .phoneGuidance`).
    private func makeRoutingHarness() async -> (AppModel, HomeViewModel) {
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
        return (app, vm)
    }

    // MARK: - Tests

    /// 1. `isExploringAlternativesFromGuidance` defaults to `false` after guidance starts.
    func test_isExploringAlternativesFromGuidance_propertyExists() async {
        let (_, vm) = await makeRoutingHarness()
        XCTAssertFalse(
            vm.isExploringAlternativesFromGuidance,
            "should default to false"
        )
    }

    /// 2. Calling `exploreAlternateRoutes()` during `.phoneGuidance` must NOT
    ///    change `homeMode` — guidance keeps running.
    func test_exploreAlternateRoutes_keepsPhoneGuidanceMode() async {
        let (_, vm) = await makeRoutingHarness()
        XCTAssertEqual(vm.homeMode, .phoneGuidance, "precondition: must be in phoneGuidance")

        vm.exploreAlternateRoutes()

        XCTAssertEqual(
            vm.homeMode,
            .phoneGuidance,
            "homeMode must remain .phoneGuidance while alternatives are being explored"
        )
    }

    /// 3. Calling `exploreAlternateRoutes()` during `.phoneGuidance` sets
    ///    `isExploringAlternativesFromGuidance` to `true`.
    func test_exploreAlternateRoutes_setsExploringFlag() async {
        let (_, vm) = await makeRoutingHarness()
        XCTAssertEqual(vm.homeMode, .phoneGuidance, "precondition: must be in phoneGuidance")

        vm.exploreAlternateRoutes()

        XCTAssertTrue(
            vm.isExploringAlternativesFromGuidance,
            "isExploringAlternativesFromGuidance must be true after exploring alternatives from guidance"
        )
    }

    /// 4. `cancelAlternativesExploration()` clears the flag and keeps the
    ///    user in `.phoneGuidance`.
    func test_cancelAlternativesExploration_clearsFlag() async {
        let (_, vm) = await makeRoutingHarness()
        vm.exploreAlternateRoutes()
        // Preconditions established by the previous two tests (assuming they pass).

        vm.cancelAlternativesExploration()

        XCTAssertFalse(
            vm.isExploringAlternativesFromGuidance,
            "isExploringAlternativesFromGuidance must be false after cancellation"
        )
        XCTAssertEqual(
            vm.homeMode,
            .phoneGuidance,
            "homeMode must remain .phoneGuidance after cancelling alternatives exploration"
        )
    }

    /// 5. `startSelectedRoute()` clears `isExploringAlternativesFromGuidance`.
    func test_startSelectedRoute_clearsExploringFlag() async {
        let (app, vm) = await makeRoutingHarness()

        // Simulate the user opening the alternatives panel from active guidance.
        vm.isExploringAlternativesFromGuidance = true

        // Seed a fresh preview so `startSelectedRoute()` has something to pick.
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

        XCTAssertFalse(
            vm.isExploringAlternativesFromGuidance,
            "isExploringAlternativesFromGuidance must be cleared when a new route is started"
        )
    }
}
