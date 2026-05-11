import XCTest
@testable import Navon

/// Mirrors the web `guidanceLabels.test.ts`, `arrivalDetection.test.ts`, and
/// `routeOverview.test.ts`: assertions on derived guidance state without a
/// SwiftUI view tree.
///
/// Why existing tests didn't cover this: HomeViewModel had no
/// `guidanceSubtitleLine`, no `arrivalNotice`, no `routeOverviewGeometry` —
/// all three were view-layer string literals before this change.
@MainActor
final class GuidanceLayoutTests: XCTestCase {

    private let origin = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
    private let quarter = CoordinatePoint(latitude: 60.17545, longitude: 24.93915)
    private let half = CoordinatePoint(latitude: 60.181, longitude: 24.9399)
    private let dest = CoordinatePoint(latitude: 60.1921, longitude: 24.9458)

    private func makeRoute(destinationLabel: String?) -> NormalizedRoutePackage {
        NormalizedRoutePackage(
            version: .current,
            routeIdentifier: "osm-test",
            revision: 1,
            geometry: [origin, quarter, half, dest],
            maneuvers: [
                RouteManeuver(
                    id: "depart", maneuverType: .depart,
                    location: origin, distanceFromStartMeters: 0,
                    distanceToNextMeters: nil, instructionText: "Start riding"
                ),
                RouteManeuver(
                    id: "left", maneuverType: .left,
                    location: quarter, distanceFromStartMeters: 60,
                    distanceToNextMeters: nil, instructionText: "Turn left onto Test St"
                ),
                RouteManeuver(
                    id: "arrive", maneuverType: .arrive,
                    location: dest, distanceFromStartMeters: 4000,
                    distanceToNextMeters: nil, instructionText: "Arrive"
                ),
            ],
            summary: RouteSummary(
                totalDistanceMeters: 4000,
                estimatedDurationSeconds: 800,
                startLabel: nil,
                destinationLabel: destinationLabel
            ),
            provenance: RouteProvenance(
                providerID: .osm,
                sourceReference: "test",
                generatedAtUnixMs: 0
            )
        )
    }

    private func vmInGuidance(destinationLabel: String?) async -> HomeViewModel {
        let appModel = AppModel()
        let alt = RouteAlternative(
            id: UUID(),
            title: "Route 1",
            subtitle: "",
            distanceMeters: 4000,
            durationSeconds: 800,
            normalizedPackage: makeRoute(destinationLabel: destinationLabel)
        )
        appModel.preview = RoutePreviewModel(
            alternatives: [alt],
            selectedAlternativeID: alt.id,
            routeIdentifier: alt.normalizedPackage.routeIdentifier,
            routeRevision: 1,
            planningNotice: nil
        )
        let vm = HomeViewModel(appModel: appModel)
        await vm.startSelectedRoute()
        return vm
    }

    func test_guidanceSubtitleLine_includesDestinationAndRemainingAndMin() async {
        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        let line = vm.guidanceSubtitleLine
        XCTAssertTrue(line.contains("Ensi linja 1"))
        XCTAssertTrue(line.contains("min"))
        XCTAssertTrue(line.contains("km") || line.contains("m"))
    }

    func test_guidanceSubtitleLine_dropsLeadingSeparatorWhenLabelMissing() async {
        let vm = await vmInGuidance(destinationLabel: "")
        XCTAssertFalse(vm.guidanceSubtitleLine.hasPrefix("•"))
        XCTAssertTrue(vm.guidanceSubtitleLine.contains("min"))
    }

    func test_routeOverviewGeometry_dropsCompletedPrefixAfterProgress() async {
        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        // Walk to ~half-way along the route.
        vm.ingestRiderLocationFix(half, timestampMs: 1000)
        let overview = vm.routeOverviewGeometry
        XCTAssertGreaterThanOrEqual(overview.count, 2)
        // The original origin must NOT be in the overview anymore.
        XCTAssertFalse(overview.contains { abs($0.latitude - origin.latitude) < 1e-6 && abs($0.longitude - origin.longitude) < 1e-6 })
        // The last point is still the destination.
        XCTAssertEqual(overview.last?.latitude ?? 0, dest.latitude, accuracy: 1e-4)
        XCTAssertEqual(overview.last?.longitude ?? 0, dest.longitude, accuracy: 1e-4)
    }

    func test_arrivalDetection_endsRoutingWhenWithinArrivalRadius() async {
        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        XCTAssertEqual(vm.homeMode, .phoneGuidance)
        // A coordinate ~5 m south of the destination — well inside the
        // 25-m arrival radius.
        let arrivalPoint = CoordinatePoint(
            latitude: dest.latitude - 0.00005,
            longitude: dest.longitude
        )
        vm.ingestRiderLocationFix(arrivalPoint, timestampMs: 1000)
        // `declareArrival()` calls `stopActiveNavigation()` which wraps its
        // body in `Task { ... }`. Yield the main actor a few times so that
        // task gets to run before we read `homeMode`.
        for _ in 0..<5 where vm.homeMode != .planning {
            await Task.yield()
        }
        XCTAssertEqual(vm.homeMode, .planning)
        XCTAssertEqual(vm.arrivalNotice, "Arrived at destination")
    }

    func test_arrival_clearsQueryAndPreview() async {
        // After arriving the "Where to?" field and all route alternatives
        // must be wiped so the map shows a blank planning state — no
        // phantom route polyline, no stale destination in the search bar.
        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        // Simulate having typed the destination in the search bar.
        vm.query = "Ensi linja 1"
        XCTAssertNotNil(vm.selectedPreview, "preview must have a selected alternative during guidance")

        let arrivalPoint = CoordinatePoint(latitude: dest.latitude - 0.00005, longitude: dest.longitude)
        vm.ingestRiderLocationFix(arrivalPoint, timestampMs: 1000)

        for _ in 0..<10 where vm.homeMode != .planning { await Task.yield() }
        for _ in 0..<10 where !vm.query.isEmpty { await Task.yield() }

        XCTAssertTrue(
            vm.query.isEmpty,
            "query must be cleared after arrival — got '\(vm.query)'"
        )
        XCTAssertNil(
            vm.selectedPreview,
            "preview must be empty after arrival (no phantom route polyline)"
        )
    }

    /// The arrival banner used to persist forever; the rider could not
    /// dismiss it even after picking a new destination, so the banner won
    /// the bottom-overlay z-order against the new route suggestions card.
    /// An explicit close affordance is required.
    func test_dismissArrivalNotice_clearsBanner() async {
        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        let arrivalPoint = CoordinatePoint(latitude: dest.latitude - 0.00005, longitude: dest.longitude)
        vm.ingestRiderLocationFix(arrivalPoint, timestampMs: 1000)
        for _ in 0..<10 where vm.arrivalNotice == nil { await Task.yield() }
        XCTAssertEqual(vm.arrivalNotice, "Arrived at destination")

        vm.dismissArrivalNotice()

        XCTAssertNil(vm.arrivalNotice)
    }

    /// 60s parity with web/Android: if the rider walks away without tapping
    /// close, the banner must clear itself so a follow-up trip's suggestions
    /// can appear.
    func test_arrivalNotice_autoDismissesAfterTimeout() async {
        // Shrink the auto-dismiss delay so the test runs in <1 s instead of
        // the production 60 s.
        HomeViewModel.arrivalNoticeAutoDismissDelayForTesting = 0.05
        defer { HomeViewModel.arrivalNoticeAutoDismissDelayForTesting = nil }

        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        let arrivalPoint = CoordinatePoint(latitude: dest.latitude - 0.00005, longitude: dest.longitude)
        vm.ingestRiderLocationFix(arrivalPoint, timestampMs: 1000)
        for _ in 0..<10 where vm.arrivalNotice == nil { await Task.yield() }
        XCTAssertEqual(vm.arrivalNotice, "Arrived at destination")

        // Wait long enough for the auto-dismiss timer to fire.
        try? await Task.sleep(nanoseconds: 300_000_000)
        XCTAssertNil(vm.arrivalNotice, "arrival banner must auto-dismiss after the configured delay")
    }

    func test_arrivalDetection_doesNotTriggerWhenFarFromDestination() async {
        let vm = await vmInGuidance(destinationLabel: "Ensi linja 1")
        // A coordinate roughly 60 m off-route from `quarter` — far from
        // `dest`, so arrival should NOT trigger.
        let mid = CoordinatePoint(
            latitude: quarter.latitude + 0.0005,
            longitude: quarter.longitude
        )
        vm.ingestRiderLocationFix(mid, timestampMs: 1000)
        XCTAssertEqual(vm.homeMode, .phoneGuidance)
        XCTAssertNil(vm.arrivalNotice)
    }
}
