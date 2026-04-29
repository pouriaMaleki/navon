import XCTest
import AVFoundation
@testable import ESP32MapCompanion

/// User-reported bug: the first tap on the "Prevent screen from turning off"
/// toggle was noticeably slow. Root cause: `SpeechService.init` eagerly
/// instantiated `AVSpeechSynthesizer` and called
/// `AVAudioSession.setCategory(.playback, …)`, both of which can block the
/// main thread on first invocation. Because `SpeechService` is owned by
/// `AppModel`, the cost rolls into the first-render path on iOS and shows
/// up as a delay on the first user interaction.
///
/// This guard lock asserts that `SpeechService.init` does not eagerly
/// activate the audio session — it must be deferred until the first
/// `speak()` call.
@MainActor
final class SpeechServiceLatencyTests: XCTestCase {

    func test_init_doesNotActivateAudioSession() {
        let service = SpeechService()
        XCTAssertFalse(
            service.hasActivatedAudioSession,
            "SpeechService.init must NOT touch AVAudioSession — defer that to the first speak() call so the first user interaction is not delayed by audio routing setup."
        )
    }

    func test_firstSpeak_activatesAudioSession() {
        let service = SpeechService()
        XCTAssertFalse(service.hasActivatedAudioSession)
        service.speak("Route started")
        XCTAssertTrue(
            service.hasActivatedAudioSession,
            "Audio session should be configured on the first speak() so the cue can actually play."
        )
        service.shutdown()
    }

    /// User-reported bug: even with all settings on, no audio came out
    /// during routing. Root cause: `configureAudioSession` only called
    /// `setCategory(...)` and never `setActive(true)`, so AVSpeechSynthesizer
    /// had a category-but-inactive audio session and produced no sound.
    /// `setActive` must run on the first `speak()`.
    func test_firstSpeak_activatesAudioSessionRouting() {
        let service = SpeechService()
        XCTAssertFalse(service.hasActivatedAudioSessionRouting)
        service.speak("Route started")
        XCTAssertTrue(
            service.hasActivatedAudioSessionRouting,
            "AVAudioSession.setActive(true) must run on the first speak() — without it the synthesizer is silent on real hardware."
        )
        service.shutdown()
    }
}
