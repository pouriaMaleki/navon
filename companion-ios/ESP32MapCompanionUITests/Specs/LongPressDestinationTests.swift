import XCTest

/// Plan flow #46 — long-press on the map drops a destination pin.
/// Expected RED on iOS until the long-press handler is wired.
final class LongPressDestinationTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func test_longPressOnMapPopulatesWhereTo() {
        let app = XCUIApplication()
        app.launch()
        let whereTo = app.textFields["whereToInput"]
        XCTAssertTrue(whereTo.waitForExistence(timeout: 5))

        // Long-press near the centre of the screen for 1 second.
        let center = app.windows.firstMatch.coordinate(
            withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6)
        )
        center.press(forDuration: 1.0)

        // Expect the input to receive a non-empty value (resolved address or
        // coordinate). RED until the long-press handler exists.
        let predicate = NSPredicate(format: "value != %@", "")
        let exp = expectation(for: predicate, evaluatedWith: whereTo, handler: nil)
        XCTAssertEqual(XCTWaiter().wait(for: [exp], timeout: 5), .completed)
    }
}
