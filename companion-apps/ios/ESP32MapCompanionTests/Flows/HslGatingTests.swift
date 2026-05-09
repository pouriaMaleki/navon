import XCTest
@testable import ESP32MapCompanion

/// L1 tests — HSL applicability gate and source-mode option list.
///
/// Spec flows #38, #39, #41 (see `/work/docs/ux-specs.md`). These exercise the
/// pure logic on `AppModel.isInFinland(_:)` and the derived computed properties
/// that drive the source-mode picker. No CoreLocation, no network.
@MainActor
final class HslGatingTests: XCTestCase {

    func test_isInFinland_helsinkiReturnsTrue() {
        let helsinki = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
        XCTAssertTrue(AppModel.isInFinland(helsinki))
    }

    func test_isInFinland_tampereReturnsTrue() {
        let tampere = CoordinatePoint(latitude: 61.4978, longitude: 23.7610)
        XCTAssertTrue(AppModel.isInFinland(tampere))
    }

    func test_isInFinland_rovaniemiReturnsTrue() {
        let rovaniemi = CoordinatePoint(latitude: 66.5039, longitude: 25.7294)
        XCTAssertTrue(AppModel.isInFinland(rovaniemi))
    }

    func test_isInFinland_rejectsStockholm() {
        let stockholm = CoordinatePoint(latitude: 59.3293, longitude: 18.0686)
        XCTAssertFalse(AppModel.isInFinland(stockholm))
    }

    func test_isInFinland_rejectsTallinn() {
        let tallinn = CoordinatePoint(latitude: 59.4370, longitude: 24.7536)
        XCTAssertFalse(AppModel.isInFinland(tallinn))
    }
}
