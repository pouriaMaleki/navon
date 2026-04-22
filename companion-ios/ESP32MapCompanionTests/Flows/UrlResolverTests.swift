import XCTest
@testable import ESP32MapCompanion

/// L1 tests for URL paste behaviour (flows #29-31).
///
/// These exercise the pure coordinate-extraction path in
/// `AppModel.resolveDestinationFromUrl(_:using:)` via fake place search.
@MainActor
final class UrlResolverTests: XCTestCase {

    func test_inlineCoordUrl_resolvesWithoutNetwork() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        search.resolveResult = DestinationSearchResult(
            id: "helsinki",
            title: "Helsinki",
            subtitle: "",
            coordinate: CoordinatePoint(latitude: 60.16, longitude: 24.95)
        )
        let result = await app.resolveDestinationFromUrl(
            "https://www.google.com/maps/@60.16,24.95,15z",
            using: search
        )
        switch result {
        case .coordinate(let coordinate, _):
            XCTAssertEqual(coordinate.latitude, 60.16, accuracy: 0.001)
            XCTAssertEqual(coordinate.longitude, 24.95, accuracy: 0.001)
        default:
            XCTFail("expected inline-coord URL to resolve to a coordinate, got \(result)")
        }
    }

    func test_nonUrlInput_returnsNoDestinationFound() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        let result = await app.resolveDestinationFromUrl("not a url", using: search)
        switch result {
        case .noDestinationFound:
            break
        default:
            XCTFail("expected .noDestinationFound for non-URL input, got \(result)")
        }
    }
}
