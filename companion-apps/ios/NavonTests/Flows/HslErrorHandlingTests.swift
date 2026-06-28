import XCTest
@testable import Navon

/// Verifies that the app surfaces errors when the HSL upstream is unreachable.
@MainActor
final class HslErrorHandlingTests: XCTestCase {

    private let helsinki = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
    private let destination = CoordinatePoint(latitude: 60.1921, longitude: 24.9458)

    /// Point to a port that nothing listens on → connection refused fast.
    private func deadEndpointApp() -> AppModel {
        let app = AppModel()
        app.settings.hslEndpointURL = "http://127.0.0.1:1/api/hsl/routing"
        return app
    }

    func test_pureHslShowsErrorWhenServerUnreachable() async {
        let app = deadEndpointApp()
        app.routeRequest = RoutePlanRequest(
            origin: helsinki,
            destination: destination,
            providerID: .hsl
        )
        await app.planRoute(using: .hsl)

        XCTAssertTrue(
            app.preview.alternatives.isEmpty,
            "alternatives must be empty when HSL server is unreachable"
        )
        XCTAssertNotNil(app.preview.planningNotice, "error notice must be present")
        XCTAssertTrue(
            app.preview.planningNotice?.contains("Planning failed") == true,
            "notice must contain 'Planning failed'"
        )
        // No start button when empty
        XCTAssertNil(app.preview.selectedAlternativeID)
    }

    func test_cardShouldBeVisibleWhenErrorPresent() async {
        let app = deadEndpointApp()
        app.routeRequest = RoutePlanRequest(
            origin: helsinki,
            destination: destination,
            providerID: .hsl
        )
        await app.planRoute(using: .hsl)

        // The BottomOverlay card visibility condition (mirrored from web):
        // alternatives.isEmpty && planningNotice == nil → hidden
        // alternatives.isEmpty && planningNotice != nil → visible ← this case
        let cardVisible = !app.preview.alternatives.isEmpty || app.preview.planningNotice != nil
        XCTAssertTrue(cardVisible, "card must be visible when there is an error notice")
    }
}
