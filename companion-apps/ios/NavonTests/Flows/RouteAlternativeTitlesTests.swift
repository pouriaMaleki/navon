import XCTest
@testable import Navon

/// User-reported tightening of the suggested-routes card. Each row was
/// labelled "OSM Route N / via …" with a redundant subtitle. The new
/// scheme drops the per-provider counter and uses the underlying engine
/// name as the title:
///
///   - OSM via BRouter fastbike → "BRouter fastbike"
///   - OSM via BRouter trekking → "BRouter trekking"
///   - OSM via OSRM bike        → "OSM Route"
///   - HSL Digitransit live / fastest → "HSL Fastest" (no subtitle)
///   - HSL Digitransit live / alternative → "HSL Route"
@MainActor
final class RouteAlternativeTitlesTests: XCTestCase {

    private func alternative(provider: RouteProviderID, sourceReference: String?) -> RouteAlternative {
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let end = CoordinatePoint(latitude: 60.18, longitude: 24.95)
        let pkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "id-\(sourceReference ?? "anon")",
            revision: 1,
            geometry: [start, end],
            maneuvers: [],
            summary: RouteSummary(totalDistanceMeters: 1000, estimatedDurationSeconds: 300,
                                  startLabel: nil, destinationLabel: "Park"),
            provenance: RouteProvenance(providerID: provider, sourceReference: sourceReference, generatedAtUnixMs: 0)
        )
        return RouteAlternative(
            id: UUID(), title: "x", subtitle: "y",
            distanceMeters: 1000, durationSeconds: 300, normalizedPackage: pkg
        )
    }

    func test_brouterFastbike_titleIsBRouterFastbike_withoutSubtitle() {
        let renamed = RoutePlanningEngine.friendlyAlternativeLabel(for: alternative(provider: .osm, sourceReference: "BRouter fastbike"))
        XCTAssertEqual(renamed.title, "BRouter fastbike")
        XCTAssertEqual(renamed.subtitle, "")
    }

    func test_brouterTrekking_titleIsBRouterTrekking() {
        let renamed = RoutePlanningEngine.friendlyAlternativeLabel(for: alternative(provider: .osm, sourceReference: "BRouter trekking"))
        XCTAssertEqual(renamed.title, "BRouter trekking")
        XCTAssertEqual(renamed.subtitle, "")
    }

    func test_osrmBike_titleIsOsmRoute() {
        let renamed = RoutePlanningEngine.friendlyAlternativeLabel(for: alternative(provider: .osm, sourceReference: "OSRM bike"))
        XCTAssertEqual(renamed.title, "OSM Route")
        XCTAssertEqual(renamed.subtitle, "")
    }

    func test_hslFastest_titleIsHslFastest_withoutSubtitle() {
        let renamed = RoutePlanningEngine.friendlyAlternativeLabel(for: alternative(provider: .hsl, sourceReference: "HSL Digitransit live / fastest"))
        XCTAssertEqual(renamed.title, "HSL Fastest")
        XCTAssertEqual(renamed.subtitle, "",
                       "Spec: HSL Fastest must NOT carry the redundant 'HSL Digitransit live / fastest' subtitle")
    }

    func test_hslAlternative_titleIsHslRoute() {
        let renamed = RoutePlanningEngine.friendlyAlternativeLabel(for: alternative(provider: .hsl, sourceReference: "HSL Digitransit live / alternative"))
        XCTAssertEqual(renamed.title, "HSL Route")
    }
}
