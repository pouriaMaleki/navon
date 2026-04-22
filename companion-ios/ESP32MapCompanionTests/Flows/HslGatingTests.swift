import XCTest
@testable import ESP32MapCompanion

/// L1 tests — HSL applicability gate and source-mode option list.
///
/// Spec flows #38, #39, #41 (see `/work/docs/ux-specs.md`). These exercise the
/// pure logic on `AppModel.isInUusimaa(_:)` and the derived computed properties
/// that drive the source-mode picker. No CoreLocation, no network.
@MainActor
final class HslGatingTests: XCTestCase {

    func test_isInUusimaa_insideBoundsReturnsTrue() {
        let helsinki = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
        XCTAssertTrue(AppModel.isInUusimaa(helsinki))
    }

    func test_isInUusimaa_outsideBoundsReturnsFalse() {
        let tampere = CoordinatePoint(latitude: 61.4978, longitude: 23.7610)
        XCTAssertFalse(AppModel.isInUusimaa(tampere))
    }

    func test_isInUusimaa_rejectsFarSouthOfBoundingBox() {
        let stockholm = CoordinatePoint(latitude: 59.3293, longitude: 18.0686)
        XCTAssertFalse(AppModel.isInUusimaa(stockholm))
    }

    func test_isInUusimaa_rejectsFarEastOfBoundingBox() {
        let stPetersburg = CoordinatePoint(latitude: 59.9311, longitude: 30.3609)
        XCTAssertFalse(AppModel.isInUusimaa(stPetersburg))
    }
}
