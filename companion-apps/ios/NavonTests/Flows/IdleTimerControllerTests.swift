import XCTest
import UIKit
@testable import Navon

@MainActor
final class IdleTimerControllerTests: XCTestCase {
    func test_setEnabled_disablesIdleTimerOnApplication() {
        let app = UIApplication.shared
        let controller = IdleTimerController(application: app)
        controller.update(true)
        XCTAssertTrue(app.isIdleTimerDisabled)
        controller.update(false)
        XCTAssertFalse(app.isIdleTimerDisabled)
    }
}
