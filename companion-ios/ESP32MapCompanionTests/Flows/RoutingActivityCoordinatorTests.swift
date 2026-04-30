import XCTest
import UIKit
@testable import ESP32MapCompanion

@MainActor
final class RoutingActivityCoordinatorTests: XCTestCase {

    final class SpeechSpy: SpeechPort {
        private(set) var spoken: [String] = []
        private(set) var lang: String = "en"
        var voiceAvailable: Bool = true
        func speak(_ text: String) { spoken.append(text) }
        func setLanguage(_ bcp47: String) { lang = bcp47 }
        func hasVoice(forLocale locale: String) -> Bool { voiceAvailable }
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
            // A single upcoming maneuver gives the first-tick cue
            // (`nextTurnInAbout`) something concrete to announce — without
            // it, the engine would emit nothing on tick 1 and these "cues
            // fire" assertions would all read empty.
            maneuvers: [CueManeuver(id: "m1", kind: .left, distanceFromStartM: 300)],
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
        var s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        // The catch-all "all gates open" gate also requires the
        // audioCuesOnlyInBackground gate to be off, since this synchronous
        // test does not drive scenePhase.
        s.audioCuesOnlyInBackground = false
        coordinator.onGuidanceTick(snapshot: snapshot(), settings: s, isRouting: true)
        XCTAssertFalse(speech.spoken.isEmpty, "expected first-tick cue; got \(speech.spoken)")
    }

    // Spec line 144: "setting to enable audio cues only when app is in
    // background and enabled by default". The default behaviour is to
    // suppress cues while the rider has the app open (their phone screen
    // shows the map already), and only speak once the screen locks /
    // they switch apps.

    func test_cuesAreSilentInForegroundWhenOnlyInBackgroundIsOn() {
        let (coordinator, speech, _) = makeCoordinator()
        var s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        s.audioCuesOnlyInBackground = true
        coordinator.onGuidanceTick(
            snapshot: snapshot(), settings: s,
            isRouting: true, isAppInBackground: false
        )
        XCTAssertEqual(speech.spoken, [])
    }

    func test_cuesFireInBackgroundWhenOnlyInBackgroundIsOn() {
        let (coordinator, speech, _) = makeCoordinator()
        var s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        s.audioCuesOnlyInBackground = true
        coordinator.onGuidanceTick(
            snapshot: snapshot(), settings: s,
            isRouting: true, isAppInBackground: true
        )
        XCTAssertFalse(speech.spoken.isEmpty, "expected first-tick cue; got \(speech.spoken)")
    }

    func test_cuesFireInForegroundWhenOnlyInBackgroundIsOff() {
        let (coordinator, speech, _) = makeCoordinator()
        var s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        s.audioCuesOnlyInBackground = false
        coordinator.onGuidanceTick(
            snapshot: snapshot(), settings: s,
            isRouting: true, isAppInBackground: false
        )
        XCTAssertFalse(speech.spoken.isEmpty, "expected first-tick cue; got \(speech.spoken)")
    }

    func test_audioCuesOnlyInBackgroundIsOnByDefault() {
        XCTAssertTrue(
            CompanionSettings.defaults.audioCuesOnlyInBackground,
            "spec line 144: enabled by default"
        )
    }

    // No-voice fallback (spec: macOS Firefox + Persian → no installed
    // Persian voice → speak English audio so cues are intelligible
    // instead of being letter-by-letter spelled by the default voice).

    func test_speaksEnglishWhenActiveLocaleHasNoVoice() {
        let (coordinator, speech, _) = makeCoordinator()
        speech.voiceAvailable = false
        var s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        s.language = .fa
        s.audioCuesOnlyInBackground = false
        coordinator.onSettingsOrRoutingChange(
            settings: s, isRouting: true, pairedWithDevice: false,
            liveActivityContent: nil
        )
        XCTAssertEqual(speech.lang, "en", "TTS should fall back to English when no Persian voice is installed")
        coordinator.onGuidanceTick(snapshot: snapshot(), settings: s, isRouting: true)
        XCTAssertFalse(speech.spoken.isEmpty, "expected at least one cue to be spoken; got \(speech.spoken)")
        XCTAssertTrue(
            speech.spoken.allSatisfy { $0.lowercased().contains("turn") || $0.lowercased().contains("arriv") },
            "cue text should be rendered in English to match the EN voice; got \(speech.spoken)"
        )
    }

    func test_speaksActiveLocaleWhenVoiceIsAvailable() {
        let (coordinator, speech, _) = makeCoordinator()
        speech.voiceAvailable = true
        var s = defaultSettings(allowBackgroundGps: true, audioCuesEnabled: true)
        s.language = .fa
        s.audioCuesOnlyInBackground = false
        coordinator.onSettingsOrRoutingChange(
            settings: s, isRouting: true, pairedWithDevice: false,
            liveActivityContent: nil
        )
        XCTAssertEqual(speech.lang, "fa", "TTS should use the active locale when a voice is installed")
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
