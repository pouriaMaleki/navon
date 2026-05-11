import XCTest

/// Plan flow #33 — tapping outside the search panel dismisses the dropdown.
final class OutsideTapDismissTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func test_tapOnMapDismissesDropdown() {
        let app = XCUIApplication()
        app.launch()
        let whereTo = app.textFields["whereToInput"]
        XCTAssertTrue(whereTo.waitForExistence(timeout: 5))
        whereTo.tap()
        whereTo.typeText("hel")
        let panel = app.otherElements["searchPanel"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))

        // Tap somewhere on the map surface, well below the input.
        let mapTap = app.windows.firstMatch.coordinate(
            withNormalizedOffset: CGVector(dx: 0.5, dy: 0.85)
        )
        mapTap.tap()

        XCTAssertFalse(panel.waitForExistence(timeout: 2))
    }
}
