import XCTest
@testable import ESP32MapCompanion

/// L1 unit tests for the BRouter response parser on iOS.
///
/// Uses the canonical Phase-0 fixture in
/// `parity-fixtures/data/cycling/brouter-fastbike-helsinki-kallio.json`
/// shared with web + Android. Verifies geometry, distance, duration, and
/// voicehints-derived maneuvers parse correctly.
@MainActor
final class CyclingRoutingTests: XCTestCase {

    private func fixturesDir() -> URL {
        // Test bundle path → walk up to repo root → parity-fixtures/data/cycling.
        // Bundle.main may be the test runner; walk upwards from the source
        // file at compile time.
        let thisFile = URL(fileURLWithPath: #file)
        // ESP32MapCompanionTests/Flows/CyclingRoutingTests.swift → repo root.
        let repoRoot = thisFile
            .deletingLastPathComponent() // Flows
            .deletingLastPathComponent() // ESP32MapCompanionTests
            .deletingLastPathComponent() // companion-ios
            .deletingLastPathComponent() // repo root
        return repoRoot.appendingPathComponent("parity-fixtures/data/cycling")
    }

    private func loadFixture(_ name: String) throws -> [String: Any] {
        let url = fixturesDir().appendingPathComponent(name)
        let data = try Data(contentsOf: url)
        return try JSONSerialization.jsonObject(with: data) as! [String: Any]
    }

    func test_mapsBrouterFastbikeFixtureToAlternative() throws {
        let response = try loadFixture("brouter-fastbike-helsinki-kallio.json")
        let features = response["features"] as! [[String: Any]]
        let request = RoutePlanRequest(
            origin: CoordinatePoint(latitude: 60.1699, longitude: 24.9384),
            destination: CoordinatePoint(latitude: 60.1854, longitude: 24.9522),
            providerID: .osm
        )
        let alt = mapBrouterToAlternative(
            feature: features[0],
            request: request,
            revision: 1,
            profile: .fastbike,
            title: "Bike-paths first"
        )
        XCTAssertNotNil(alt, "parser must produce an alternative for the fixture")
        let pkg = alt!.normalizedPackage
        XCTAssertEqual(pkg.provenance.providerID, .osm)
        XCTAssertGreaterThan(pkg.geometry.count, 50, "BRouter fastbike fixture has many polyline points")
        XCTAssertGreaterThan(pkg.summary.totalDistanceMeters, 1000)
        XCTAssertLessThan(pkg.summary.totalDistanceMeters, 5000)
        XCTAssertGreaterThanOrEqual(pkg.summary.estimatedDurationSeconds, 60)
        XCTAssertEqual(pkg.maneuvers.first?.maneuverType, .depart)
        XCTAssertEqual(pkg.maneuvers.last?.maneuverType, .arrive)
        XCTAssertTrue(
            pkg.maneuvers.contains { $0.maneuverType != .depart && $0.maneuverType != .arrive && $0.maneuverType != .straight },
            "voicehints should produce at least one non-trivial turn"
        )
    }

    func test_mapsBrouterTrekkingFixtureToAlternative() throws {
        let response = try loadFixture("brouter-trekking-helsinki-kallio.json")
        let features = response["features"] as! [[String: Any]]
        let alt = mapBrouterToAlternative(
            feature: features[0],
            request: RoutePlanRequest(
                origin: CoordinatePoint(latitude: 60.1699, longitude: 24.9384),
                destination: CoordinatePoint(latitude: 60.1854, longitude: 24.9522),
                providerID: .osm
            ),
            revision: 1,
            profile: .trekking,
            title: "Balanced cycling"
        )
        XCTAssertNotNil(alt)
        XCTAssertEqual(alt?.title, "Balanced cycling")
        XCTAssertGreaterThan(alt?.normalizedPackage.geometry.count ?? 0, 50)
    }
}
