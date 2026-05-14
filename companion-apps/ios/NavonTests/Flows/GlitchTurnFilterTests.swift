import XCTest
@testable import Navon

final class GlitchTurnFilterTests: XCTestCase {

    private let metersPerDegLat = 111_320.0

    private func straightGeometry(lengthM: Double, stepM: Double = 5) -> [CoordinatePoint] {
        var points: [CoordinatePoint] = []
        var d = 0.0
        while d <= lengthM {
            points.append(CoordinatePoint(latitude: 60.17 + d / metersPerDegLat, longitude: 24.94))
            d += stepM
        }
        return points
    }

    /// Geometry with a localized bend at `bendAtM` by `totalBendDeg` degrees.
    /// Before the bend: heading 0 (north). After: heading = totalBendDeg (clockwise).
    private func bentGeometry(lengthM: Double, bendAtM: Double, totalBendDeg: Double, stepM: Double = 5) -> [CoordinatePoint] {
        let bendRad = totalBendDeg * .pi / 180
        var points: [CoordinatePoint] = []
        var lat = 60.17
        var lon = 24.94
        points.append(CoordinatePoint(latitude: lat, longitude: lon))
        var d = stepM
        while d <= lengthM {
            let t = max(0, min(1, (d - (bendAtM - 2.5)) / 5))
            let heading = bendRad * t
            let cosLat = cos(lat * .pi / 180)
            lat += cos(heading) * stepM / metersPerDegLat
            lon += sin(heading) * stepM / (metersPerDegLat * cosLat)
            points.append(CoordinatePoint(latitude: lat, longitude: lon))
            d += stepM
        }
        return points
    }

    private func pointAtDistance(_ geometry: [CoordinatePoint], _ targetDistM: Double) -> CoordinatePoint {
        let cum = cumulativeDistances(geometry)
        for (i, c) in cum.enumerated() {
            if c >= targetDistM - 1e-3 { return geometry[i] }
        }
        return geometry.last!
    }

    private func m(_ id: String, _ type: RouteManeuverType, _ dist: Double, location: CoordinatePoint? = nil) -> RouteManeuver {
        let loc = location ?? CoordinatePoint(latitude: 60.17 + dist / metersPerDegLat, longitude: 24.94)
        return RouteManeuver(
            id: id,
            maneuverType: type,
            location: loc,
            distanceFromStartMeters: dist
        )
    }

    private func ids(_ maneuvers: [RouteManeuver]) -> [String] {
        maneuvers.map(\.id)
    }

    // MARK: - Tests

    func test_empty_returnsUnchanged() {
        XCTAssertEqual(filterGlitchClusters([], geometry: []).count, 0)
    }

    func test_single_returnsUnchanged() {
        let maneuvers = [m("m1", .left, 100)]
        let geom = straightGeometry(lengthM: 200)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(ids(result), ["m1"])
    }

    func test_insufficientGeometry_returnsUnchanged() {
        let maneuvers = [m("m1", .left, 100), m("m2", .left, 107)]
        let geom = [CoordinatePoint(latitude: 60.17, longitude: 24.94)]
        XCTAssertEqual(ids(filterGlitchClusters(maneuvers, geometry: geom)), ["m1", "m2"])
    }

    func test_removesTwoCloseManeuversOnStraightPath() {
        let maneuvers = [m("m1", .left, 100), m("m2", .left, 107)]
        let geom = straightGeometry(lengthM: 200)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 0)
    }

    func test_preservesTwoCloseManeuversWhenPathBends() {
        let geom = bentGeometry(lengthM: 200, bendAtM: 103.5, totalBendDeg: 12)
        let maneuvers = [
            m("m1", .left, 100, location: pointAtDistance(geom, 100)),
            m("m2", .left, 107, location: pointAtDistance(geom, 107)),
        ]
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(ids(result), ["m1", "m2"])
    }

    func test_removesThreeCloseManeuversOnStraightPath() {
        let maneuvers = [
            m("m1", .left, 100),
            m("m2", .right, 105),
            m("m3", .left, 112),
        ]
        let geom = straightGeometry(lengthM: 200)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 0)
    }

    func test_preservesThreeCloseManeuversOnCurvedPath() {
        let geom = bentGeometry(lengthM: 200, bendAtM: 108.5, totalBendDeg: 15)
        let maneuvers = [
            m("m1", .left, 100, location: pointAtDistance(geom, 100)),
            m("m2", .right, 105, location: pointAtDistance(geom, 105)),
            m("m3", .left, 112, location: pointAtDistance(geom, 112)),
        ]
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(ids(result), ["m1", "m2", "m3"])
    }

    func test_doesNotGroupManeuversMoreThan10mApart() {
        let maneuvers = [
            m("m1", .left, 100),
            m("m2", .left, 115),
        ]
        let geom = straightGeometry(lengthM: 200)
        XCTAssertEqual(ids(filterGlitchClusters(maneuvers, geometry: geom)), ["m1", "m2"])
    }

    func test_twoSeparateClusters_handledIndependently() {
        let maneuvers = [
            m("m1", .left, 100),
            m("m2", .right, 107),
            m("m3", .left, 300),
            m("m4", .right, 306),
        ]
        let geom = straightGeometry(lengthM: 500)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 0)
    }

    func test_keepsNonClusteredManeuverAfterRemovedCluster() {
        let maneuvers = [
            m("m1", .left, 100),
            m("m2", .right, 107),
            m("m3", .left, 300),
        ]
        let geom = straightGeometry(lengthM: 500)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(ids(result), ["m3"])
    }

    func test_preservesGlitchClusterThatTurnsSharplyEnough() {
        let geom = bentGeometry(lengthM: 200, bendAtM: 103.5, totalBendDeg: 10.5)
        let maneuvers = [
            m("m1", .left, 100, location: pointAtDistance(geom, 100)),
            m("m2", .left, 107, location: pointAtDistance(geom, 107)),
        ]
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(ids(result), ["m1", "m2"])
    }

    func test_clusterAtRouteStart() {
        let maneuvers = [
            m("depart", .depart, 0),
            m("m1", .left, 5),
        ]
        let geom = straightGeometry(lengthM: 200)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 0)
    }

    func test_clusterAtRouteEnd() {
        let maneuvers = [
            m("m1", .left, 180),
            m("m2", .right, 187),
            m("arrive", .arrive, 200),
        ]
        let geom = straightGeometry(lengthM: 200)
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        // m1+m2 form a glitch cluster (7m gap, straight path) - removed
        // arrive is 13m from m2 (>10m) so it stays
        XCTAssertEqual(ids(result), ["arrive"])
    }

    func test_bendJustAboveThreshold_isPreserved() {
        let geom = bentGeometry(lengthM: 200, bendAtM: 103.5, totalBendDeg: 10.2)
        let maneuvers = [
            m("m1", .left, 100, location: pointAtDistance(geom, 100)),
            m("m2", .left, 107, location: pointAtDistance(geom, 107)),
        ]
        let result = filterGlitchClusters(maneuvers, geometry: geom)
        XCTAssertEqual(ids(result), ["m1", "m2"])
    }
}
