import XCTest

/// Plan flow #32 — full-row tap target on the where-to dropdown. iOS L3.
final class DropdownRowHitAreaTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func test_tapNearRowEdgeSelectsTheItem() {
        let app = XCUIApplication()
        app.launch()
        let whereTo = app.textFields["whereToInput"]
        XCTAssertTrue(whereTo.waitForExistence(timeout: 5))
        whereTo.tap()
        whereTo.typeText("hel")

        let row = app.buttons["searchRow-0"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        // Tap close to the leading edge of the row — should still register.
        let edge = row.coordinate(withNormalizedOffset: CGVector(dx: 0.02, dy: 0.5))
        edge.tap()

        // After selection the input should be populated and the dropdown
        // dismissed. We assert the dropdown disappears.
        XCTAssertFalse(row.waitForExistence(timeout: 2))
    }
}
