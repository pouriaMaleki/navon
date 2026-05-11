import XCTest
@testable import Navon

/// L2 tests for routing progress tracking on iOS (spec line 102).
/// User-reported bug: after the rider passes the first turn, the
/// "next turn" line on screen stays stuck on the first maneuver. Root
/// cause: HomeViewModel.nextInstructionLine returned the first non-depart
/// maneuver with no progress tracking.
@MainActor
final class GuidanceProgressTests: XCTestCase {

    private func offset(_ base: CoordinatePoint, eastM: Double, northM: Double) -> CoordinatePoint {
        let metersPerDegLat = 111_320.0
        let meanLat = base.latitude * .pi / 180.0
        return CoordinatePoint(
            latitude: base.latitude + northM / metersPerDegLat,
            longitude: base.longitude + eastM / (metersPerDegLat * cos(meanLat))
        )
    }

    /// L-shape route: start → 400 m N → 400 m E.
    /// m1 depart 0m, m2 right 400m, m3 arrive 800m.
    private func lShapeRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegLat,
            longitude: 24.94
        )
        let cosLat = cos(60.17 * .pi / 180.0)
        let end = CoordinatePoint(
            latitude: mid.latitude,
            longitude: mid.longitude + 400.0 / (metersPerDegLat * cosLat)
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "lshape",
            revision: 1,
            geometry: [start, mid, end],
            maneuvers: [
                RouteManeuver(id: "m1", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "m2", maneuverType: .right, location: mid,
                              distanceFromStartMeters: 400, distanceToNextMeters: 400,
                              instructionText: "Turn right onto 2nd street"),
                RouteManeuver(id: "m3", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 800, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 800, estimatedDurationSeconds: 240,
                                  startLabel: nil, destinationLabel: nil),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    func test_nextInstructionLine_atStartPointsAtRightTurn() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let line = vm.nextInstructionLine ?? ""
        XCTAssertTrue(
            line.lowercased().contains("right"),
            "before progress, next instruction should be the right turn at the corner — got '\(line)'"
        )
    }

    func test_nextInstructionLine_transitionsAfterPassingCorner() async {
        // Spec line 102 — the user-reported regression: after the rider
        // passes the first turn, the line must transition off "right" to
        // the next maneuver (arrive). Today HomeViewModel always returns
        // the first non-depart maneuver, so this is RED.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let cosLat = cos(60.17 * .pi / 180.0)
        let mid = pkg.geometry[1]
        let pastCorner = CoordinatePoint(
            latitude: mid.latitude,
            longitude: mid.longitude + 50.0 / (111_320.0 * cosLat)
        )
        // Two ticks past the corner so progressDistanceM monotonically advances.
        vm.ingestRiderLocationFix(pastCorner, timestampMs: 0)
        vm.ingestRiderLocationFix(pastCorner, timestampMs: 500)
        let line = vm.nextInstructionLine ?? ""
        XCTAssertFalse(
            line.lowercased().contains("right"),
            "after passing the corner, next instruction must move past the right turn — got '\(line)'"
        )
    }
}
