import XCTest
@testable import Navon

/// Spec lines 128-145 — four new settings (keepScreenOn, allowBackgroundGps,
/// audioCuesEnabled, liveActivityEnabled) added to `CompanionSettings`.
/// Verifies defaults, round-trip persistence, and legacy-blob migration
/// (older payloads that lack these keys must load with the spec defaults).
@MainActor
final class CompanionSettingsExpansionTests: XCTestCase {

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "settings-expansion-tests-\(UUID().uuidString)")!
    }

    func test_defaults_matchSpec() {
        let defaults = CompanionSettings.defaults
        XCTAssertFalse(defaults.keepScreenOn)
        XCTAssertFalse(defaults.allowBackgroundGps)
        XCTAssertTrue(defaults.audioCuesEnabled, "Audio cues are on by default per spec")
        XCTAssertFalse(defaults.liveActivityEnabled)
    }

    func test_roundTrip_allFourNewFields() {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        var settings = CompanionSettings.defaults
        settings.keepScreenOn = true
        settings.allowBackgroundGps = true
        settings.audioCuesEnabled = false
        settings.liveActivityEnabled = true
        persistence.saveSettings(settings)

        let reloaded = persistence.loadSettings()
        XCTAssertTrue(reloaded.keepScreenOn)
        XCTAssertTrue(reloaded.allowBackgroundGps)
        XCTAssertFalse(reloaded.audioCuesEnabled)
        XCTAssertTrue(reloaded.liveActivityEnabled)
    }

    func test_legacyBlob_missingNewKeys_loadsWithSpecDefaults() throws {
        let defaults = freshDefaults()
        // Hand-craft a legacy JSON blob that lacks the new keys.
        let legacy: [String: Any] = [
            "hslEndpointURL": "/api/hsl/routing",
            "cyclingSpeedKph": 18.0,
            "speedUnit": "kph"
        ]
        let data = try JSONSerialization.data(withJSONObject: legacy, options: [])
        defaults.set(data, forKey: "companion.settings")

        let persistence = CompanionPersistence(defaults: defaults)
        let loaded = persistence.loadSettings()
        XCTAssertFalse(loaded.keepScreenOn)
        XCTAssertFalse(loaded.allowBackgroundGps)
        XCTAssertTrue(loaded.audioCuesEnabled, "Legacy blob must default audio cues to true")
        XCTAssertFalse(loaded.liveActivityEnabled)
    }
}
