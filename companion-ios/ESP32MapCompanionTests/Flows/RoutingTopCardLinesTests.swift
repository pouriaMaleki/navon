import XCTest
@testable import ESP32MapCompanion

/// User-reported reformat: the routing top card was a 2-line block whose
/// subtitle bundled "<destination> • X km remaining • Y min". The user
/// wants a 3-line block instead:
///
///   1. Headline — next-turn instruction (existing `nextInstructionLine`)
///   2. "8.6 km to Alppila"   (distance first, then destination short name)
///   3. "16 min remaining"
///
/// `routingTopLayout` exposes the lines as a typed array so the view
/// renders them via `ForEach` and the format is pinned by these tests.
@MainActor
final class RoutingTopCardLinesTests: XCTestCase {

    /// Fresh AppModel backed by an in-memory UserDefaults so the
    /// destination-label tests don't see leftover state from prior runs.
    private func freshApp() -> AppModel {
        let defaults = UserDefaults(suiteName: "routing-top-card-tests-\(UUID().uuidString)")!
        return AppModel(persistence: CompanionPersistence(defaults: defaults))
    }

    private func tinyRoute(destinationLabel: String) -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        // Place destination 8.6 km north so the subtitle reads "8.6 km".
        let mid = CoordinatePoint(
            latitude: 60.17 + 8_600.0 / metersPerDegLat,
            longitude: 24.94
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "lines-test",
            revision: 1,
            geometry: [start, mid],
            maneuvers: [
                RouteManeuver(id: "depart", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 8_600, instructionText: nil),
                RouteManeuver(id: "arrive", maneuverType: .arrive, location: mid,
                              distanceFromStartMeters: 8_600, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 8_600, estimatedDurationSeconds: 960,
                                  startLabel: nil, destinationLabel: destinationLabel),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    func test_routingTopLayout_secondLineIs_distanceToDestinationName() async {
        let app = freshApp()
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute(destinationLabel: "Alppila")
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 8_600, durationSeconds: 960, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let layout = vm.routingTopLayout
        XCTAssertEqual(
            layout?.distanceToDestinationLine, "8.6 km to Alppila",
            "Routing card line 2 must be 'X km to <destination>' — got '\(layout?.distanceToDestinationLine ?? "nil")'"
        )
    }

    func test_routingTopLayout_thirdLineIs_minutesRemaining() async {
        let app = freshApp()
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute(destinationLabel: "Alppila")
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 8_600, durationSeconds: 960, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let layout = vm.routingTopLayout
        XCTAssertEqual(
            layout?.minutesRemainingLine, "16 min remaining",
            "Routing card line 3 must be 'Y min remaining' — got '\(layout?.minutesRemainingLine ?? "nil")'"
        )
    }

    func test_routingTopLayout_prefersUserPickedDestination_overGenericRoutePackagePlaceholder() async {
        // User-reported regression: OSRM/HSL packages embed a generic
        // `summary.destinationLabel = "Selected destination"`, so the
        // distance line read just "3.4 km" — even though the rider had
        // typed a real destination ("Kallio") into the where-to bar and
        // that is what's stored on `activeSession.destinationLabel`.
        let app = freshApp()
        let vm = HomeViewModel(appModel: app)
        // 3,400 m route so the formatter shows "3.4 km".
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let end = CoordinatePoint(latitude: 60.17 + 3_400.0 / metersPerDegLat, longitude: 24.94)
        let pkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "user-typed",
            revision: 1,
            geometry: [start, end],
            maneuvers: [
                RouteManeuver(id: "depart", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 3_400, instructionText: nil),
                RouteManeuver(id: "arrive", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 3_400, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 3_400, estimatedDurationSeconds: 600,
                                  startLabel: "Current location",
                                  destinationLabel: "Selected destination"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: "OSRM bike", generatedAtUnixMs: 0)
        )
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 3_400, durationSeconds: 600, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        // Stamp the user-typed destination on the session, mirroring what
        // happens after `applySelectedAlternativeToSession`.
        app.activeSession.destinationLabel = "Kallio"
        await vm.startSelectedRoute()
        let layout = vm.routingTopLayout
        XCTAssertEqual(
            layout?.distanceToDestinationLine, "3.4 km to Kallio",
            "When the route package's destinationLabel is the generic 'Selected destination' placeholder, fall back to the user-picked destination on activeSession — got '\(layout?.distanceToDestinationLine ?? "nil")'"
        )
    }

    func test_routingTopLayout_dropsDistanceLine_whenDestinationLabelIsMissing() async {
        // Defensive: if the destination label is the placeholder we should
        // not produce a confusing "X km to No destination" line.
        let app = freshApp()
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute(destinationLabel: "No destination")
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 8_600, durationSeconds: 960, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let layout = vm.routingTopLayout
        XCTAssertEqual(
            layout?.distanceToDestinationLine, "8.6 km",
            "When there's no destination name, line 2 falls back to 'X km' alone — got '\(layout?.distanceToDestinationLine ?? "nil")'"
        )
    }
}
