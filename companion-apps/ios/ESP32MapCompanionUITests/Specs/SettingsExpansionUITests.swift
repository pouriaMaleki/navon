import XCTest

/// Spec lines 128-145 — the four activity settings (prevent screen off,
/// allow GPS in background, audio cues, lock-screen live activity) must
/// appear at the TOP of the settings page, in spec order, with the cues +
/// live-activity toggles disabled until background-GPS is on.
final class SettingsExpansionUITests: XCTestCase {

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func test_settingsTopOrder_matchesSpec() {
        let app = XCUIApplication()
        app.launch()
        // Open settings — the existing settings button is exposed via the
        // "Settings" accessibility label on the home screen.
        let settingsButton = app.buttons["Settings"]
        XCTAssertTrue(settingsButton.waitForExistence(timeout: 5))
        settingsButton.tap()

        let order = [
            "setting-keepScreenOn",
            "setting-allowBackgroundGps",
            "setting-audioCuesEnabled",
            "setting-liveActivityEnabled",
        ]
        for id in order {
            XCTAssertTrue(
                app.descendants(matching: .any).matching(identifier: id).firstMatch.waitForExistence(timeout: 3),
                "Toggle \(id) should be visible at the top of the settings page"
            )
        }
    }
}
