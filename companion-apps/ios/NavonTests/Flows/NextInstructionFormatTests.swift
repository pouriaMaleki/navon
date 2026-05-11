import XCTest
@testable import Navon

/// User-reported reformat: the next-turn headline used to read
/// "Turn right • 17 m". The user wanted the distance first so the eye
/// hits the metric — matching the rest of the routing top card
/// ("8.6 km to Alppila", "16 min remaining"). New format:
///
///   "17 m Turn right"
@MainActor
final class NextInstructionFormatTests: XCTestCase {

    private func offset(_ base: CoordinatePoint, eastM: Double, northM: Double) -> CoordinatePoint {
        let metersPerDegLat = 111_320.0
        let meanLat = base.latitude * .pi / 180.0
        return CoordinatePoint(
            latitude: base.latitude + northM / metersPerDegLat,
            longitude: base.longitude + eastM / (metersPerDegLat * cos(meanLat))
        )
    }

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
            routeIdentifier: "lshape-fmt",
            revision: 1,
            geometry: [start, mid, end],
            maneuvers: [
                RouteManeuver(id: "m1", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "m2", maneuverType: .right, location: mid,
                              distanceFromStartMeters: 400, distanceToNextMeters: 400,
                              instructionText: "Turn right"),
                RouteManeuver(id: "m3", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 800, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 800, estimatedDurationSeconds: 240,
                                  startLabel: nil, destinationLabel: "Park"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    func test_nextInstructionLine_putsDistanceBeforeInstruction() async {
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
        // Move the rider 383 m up — 17 m short of the right-turn at 400 m.
        vm.ingestRiderLocationFix(offset(pkg.geometry[0], eastM: 0, northM: 383), timestampMs: 1_000)

        let line = vm.nextInstructionLine ?? ""
        XCTAssertEqual(
            line, "17 m Turn right",
            "Next-turn headline must be '<distance> <instruction>' — got '\(line)'"
        )
    }
}
