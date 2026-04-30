import XCTest
import UIKit
@testable import ESP32MapCompanion

@MainActor
final class RoutingActivityCoordinatorTests: XCTestCase {

    final class SpeechSpy: SpeechPort {
        private(set) var spoken: [String] = []
        private(set) var lang: String = "en"
        func speak(_ text: String) { spoken.append(text) }
        func setLanguage(_ bcp47: String) { lang = bcp47 }
        func shutdown() {}
    }

    private func makeCoordinator() -> (RoutingActivityCoordinator, SpeechSpy, SpyLiveActivityPort) {
        let speech = SpeechSpy()
        let liveActivity = SpyLiveActivityPort()
        let idleTimer = IdleTimerController(application: UIApplication.shared)
        let coordinator = RoutingActivityCoordinator(
            idleTimer: idleTimer,
            speech: speech,
            liveActivity: liveActivity
        )
        return (coordinator, speech, liveActivity)
    }

    private func defaultSettings(
        keepScreenOn: Bool = false,
        allowBackgroundGps: Bool = false,
        audioCuesEnabled: Bool = true,
        liveActivityEnabled: Bool = false
    ) -> CompanionSettings {
        var s = CompanionSettings.defaults
        s.keepScreenOn = keepScreenOn
        s.allowBackgroundGps = allowBackgroundGps
        s.audioCuesEnabled = audioCuesEnabled
        s.liveActivityEnabled = liveActivityEnabled
        return s
    }

    private func snapshot(pairedWithDevice: Bool = false) -> CueSnapshot {
        CueSnapshot(
            routeId: "r1",
            pairedWithDevice: pairedWithDevice,
            progressDistanceM: 0,
            maneuvers: [],
            offRoute: false,
            rerouting: false,
            arrived: false,
            distanceFromRouteM: 0,
            routeTotalDistanceM: 1000
        )
    }

    func test_cuesAreSilentUntilBackgroundGpsIsOn() {
        let (coordinator, speech, _) = makeCoordinator()
        let s = defaultSettings(allowBackgroundGps: false, audioCuesEnabled: true)
        coordinator.onGuidanceTick(snapshot: snapshot(), settings: s, isRouting: true)
        XCTAssertEqual(speech.spoken, [])
    }

    func test_cuesAreSilentWhenPairedWithDevice() {
        let (coordinator, speech, _) = makeCoordinator()
        let s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        coordinator.onGuidanceTick(snapshot: snapshot(pairedWithDevice: true), settings: s, isRouting: true)
        XCTAssertEqual(speech.spoken, [])
    }

    func test_cuesFireWhenAllGatesAreOpen() {
        let (coordinator, speech, _) = makeCoordinator()
        let s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        coordinator.onGuidanceTick(snapshot: snapshot(), settings: s, isRouting: true)
        XCTAssertTrue(speech.spoken.contains("Route started"))
    }

    func test_liveActivity_startsAndEndsOnRoutingTransitions() {
        let (coordinator, _, live) = makeCoordinator()
        let onSettings = defaultSettings(allowBackgroundGps: true, liveActivityEnabled: true)
        let content = RoutingLiveActivityContent(
            routeIdentifier: "r1",
            destinationLabel: "Park",
            nextInstruction: "Turn left in 200m",
            etaMinutes: 7
        )
        coordinator.onSettingsOrRoutingChange(
            settings: onSettings, isRouting: true, pairedWithDevice: false,
            liveActivityContent: content
        )
        XCTAssertEqual(live.startedWith, content)

        coordinator.onSettingsOrRoutingChange(
            settings: onSettings, isRouting: false, pairedWithDevice: false,
            liveActivityContent: nil
        )
        XCTAssertEqual(live.endedCount, 1)
    }

    func test_liveActivity_doesNotStartWhenBackgroundGpsIsOff() {
        let (coordinator, _, live) = makeCoordinator()
        let s = defaultSettings(allowBackgroundGps: false, liveActivityEnabled: true)
        let content = RoutingLiveActivityContent(
            routeIdentifier: "r1",
            destinationLabel: "Park",
            nextInstruction: "Turn left in 200m",
            etaMinutes: 7
        )
        coordinator.onSettingsOrRoutingChange(
            settings: s, isRouting: true, pairedWithDevice: false,
            liveActivityContent: content
        )
        XCTAssertNil(live.startedWith)
    }
}
