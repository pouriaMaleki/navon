import AVFoundation
import os.log

/// Audio cue port consumed by the cue engine wiring. The default
/// implementation uses `AVSpeechSynthesizer`; tests inject `SpeechSpy` (or
/// any other implementation) instead.
protocol SpeechPort: AnyObject {
    func speak(_ text: String)
    func shutdown()
}

/// Real `AVSpeechSynthesizer` wrapper. The synthesizer and the
/// `AVAudioSession` activation are both deferred to the first `speak()`
/// call — instantiating them eagerly was blocking the iOS main thread on
/// app launch (the first audio-session activation can take 100 ms+ while
/// CoreAudio sets up routing), which surfaced as a noticeably-slow first
/// tap on the settings toggles.
///
/// All steps emit `os_log` lines under the
/// `me.fiksu.esp32map.companion.ios` subsystem with category `audio` so a
/// failing real-device run can be diagnosed from Console.app
/// (`subsystem:me.fiksu.esp32map.companion.ios category:audio`).
final class SpeechService: SpeechPort {
    private static let log = Logger(
        subsystem: "me.fiksu.esp32map.companion.ios",
        category: "audio"
    )

    private var synthesizer: AVSpeechSynthesizer?
    /// True after the audio session category is configured.
    private(set) var hasActivatedAudioSession: Bool = false
    /// True after the audio session is also `setActive(true)`. Without
    /// this the synthesizer has a category but no active route, so it
    /// produces no sound on real hardware.
    private(set) var hasActivatedAudioSessionRouting: Bool = false

    init() {
        Self.log.info("SpeechService.init — synthesizer + audio session deferred until first speak()")
    }

    func speak(_ text: String) {
        Self.log.info("speak(\"\(text, privacy: .public)\")")
        let synth = ensureSynthesizer()
        if synth.isSpeaking {
            Self.log.debug("cancelling in-flight utterance before speaking new cue")
            synth.stopSpeaking(at: .immediate)
        }
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: AVSpeechSynthesisVoice.currentLanguageCode())
        synth.speak(utterance)
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        Self.log.info(
            "AVAudioSession after speak — category=\(session.category.rawValue, privacy: .public) mode=\(session.mode.rawValue, privacy: .public) outputVolume=\(session.outputVolume) otherAudioPlaying=\(session.isOtherAudioPlaying)"
        )
        #endif
    }

    func shutdown() {
        Self.log.info("shutdown")
        if let synth = synthesizer, synth.isSpeaking {
            synth.stopSpeaking(at: .immediate)
        }
    }

    private func ensureSynthesizer() -> AVSpeechSynthesizer {
        if let synth = synthesizer { return synth }
        Self.log.info("ensureSynthesizer — first speak(), constructing AVSpeechSynthesizer + activating audio session")
        configureAudioSession()
        let synth = AVSpeechSynthesizer()
        synthesizer = synth
        return synth
    }

    private func configureAudioSession() {
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(
                .playback,
                mode: .voicePrompt,
                options: [.duckOthers, .interruptSpokenAudioAndMixWithOthers]
            )
            Self.log.info("setCategory(.playback, .voicePrompt, [.duckOthers, .interruptSpokenAudioAndMixWithOthers]) — OK")
        } catch {
            Self.log.error("setCategory failed: \(error.localizedDescription, privacy: .public)")
        }
        do {
            try session.setActive(true, options: [])
            hasActivatedAudioSessionRouting = true
            Self.log.info("setActive(true) — OK")
        } catch {
            Self.log.error("setActive(true) failed: \(error.localizedDescription, privacy: .public)")
        }
        #else
        hasActivatedAudioSessionRouting = true
        #endif
        hasActivatedAudioSession = true
    }
}
