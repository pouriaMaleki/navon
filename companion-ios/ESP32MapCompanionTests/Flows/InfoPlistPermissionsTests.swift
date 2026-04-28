import XCTest
@testable import ESP32MapCompanion

final class InfoPlistPermissionsTests: XCTestCase {
    func test_infoPlist_includesCameraUsageDescription() throws {
        // The host app's Info.plist is loaded by the simulator; reach into the
        // host bundle (not the test bundle) so this catches a missing or
        // wrong-bundle copy.
        let hostBundle = Bundle(identifier: "me.fiksu.esp32map.companion.ios")
            ?? Bundle.main
        let value = hostBundle.object(forInfoDictionaryKey: "NSCameraUsageDescription") as? String
        XCTAssertNotNil(value, "NSCameraUsageDescription missing from app Info.plist")
        let unwrapped = value ?? ""
        XCTAssertFalse(unwrapped.isEmpty, "NSCameraUsageDescription must be non-empty")
        // Catches the App-Review-fail "generic copy" case.
        XCTAssertTrue(unwrapped.contains("ESP32"), "Camera permission copy must mention the device name (got: \(unwrapped))")
    }
}
