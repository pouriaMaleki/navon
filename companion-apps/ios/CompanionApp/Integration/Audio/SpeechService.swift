import AVFoundation
import os.log

/// Audio cue port consumed by the cue engine wiring. The default
/// implementation uses `AVSpeechSynthesizer`; tests inject `SpeechSpy` (or
/// any other implementation) instead.
protocol SpeechPort: AnyObject {
    func speak(_ text: String)
    func setLanguage(_ bcp47: String)
    /// True when the OS has at least one TTS voice installed for `locale`'s
    /// primary language tag (e.g. `fa` matches "fa-IR"). Used by the cue
    /// dispatcher to fall back to English audio when the rider's language
    /// isn't installed — otherwise the OS spells Persian/Arabic glyphs
    /// letter-by-letter via the default English voice.
    func hasVoice(forLocale locale: String) -> Bool
    func shutdown()
}

/// AVSpeechSynthesizer wrapper. Deferred init avoids 100ms+ main-thread block on app launch.
/// Uses `.duckOthers` with debounced deactivation after `didFinish` — back-to-back cues keep
/// the session hot; a 1.5s quiet window restores music volume so it isn't permanently dimmed.
final class SpeechService: NSObject, SpeechPort, AVSpeechSynthesizerDelegate {
    private static let log = Logger(
        subsystem: "app.navon.bike",
        category: "audio"
    )

    /// 1.5 s — long enough for a "50m, turn left" → "turn left" pair to stay under one duck envelope.
    private static let deactivationDelaySeconds: UInt64 = 1_500_000_000

    private var synthesizer: AVSpeechSynthesizer?
    /// True after the audio session category has been configured at
    /// least once. The category set is permanent for this process; only
    /// the active flag flips on/off across cues.
    private(set) var hasActivatedAudioSession: Bool = false
    /// True between `setActive(true)` and a successful deactivation.
    /// Drives the "do we need to re-activate before this speak()?" check.
    private(set) var hasActivatedAudioSessionRouting: Bool = false
    /// BCP-47 tag of the active locale. The wiring layer calls
    /// `setLanguage(_:)` whenever the user's language preference changes.
    /// Defaults to the device's TTS language so first-launch behaviour
    /// matches the prior implementation.
    private var activeLanguage: String = AVSpeechSynthesisVoice.currentLanguageCode()
    /// Pending deactivation kicked off by `didFinish`. New `speak()`
    /// calls cancel this so the session stays hot for back-to-back cues.
    private var pendingDeactivation: Task<Void, Never>?

    override init() {
        super.init()
        Self.log.info("SpeechService.init — synthesizer + audio session deferred until first speak()")
    }

    func setLanguage(_ bcp47: String) {
        Self.log.info("setLanguage(\(bcp47, privacy: .public))")
        activeLanguage = bcp47
    }

    func hasVoice(forLocale locale: String) -> Bool {
        Self.hasVoice(forLocale: locale)
    }

    /// Static seam used by the Settings picker — we don't want the SwiftUI
    /// view to reach into the routing coordinator just to query voice
    /// availability.
    static func hasVoice(forLocale locale: String) -> Bool {
        let primary = locale
            .split(separator: "-").first
            .map(String.init)?.lowercased() ?? locale.lowercased()
        return AVSpeechSynthesisVoice.speechVoices().contains { voice in
            let voicePrimary = voice.language
                .split(separator: "-").first
                .map(String.init)?.lowercased() ?? voice.language.lowercased()
            return voicePrimary == primary
        }
    }

    func speak(_ text: String) {
        Self.log.info("speak(\"\(text, privacy: .public)\")")
        // Cancel any pending session-deactivation: a fresh utterance
        // means we want the duck to keep holding music down rather than
        // ramp music up just before the next cue speaks.
        cancelPendingDeactivation()
        let synth = ensureSynthesizer()
        if !hasActivatedAudioSessionRouting {
            activateAudioSession()
        }
        if synth.isSpeaking {
            Self.log.debug("cancelling in-flight utterance before speaking new cue")
            synth.stopSpeaking(at: .immediate)
        }
        let utterance = AVSpeechUtterance(string: text)
        // Picks the OS's preferred voice for `activeLanguage`. If no voice
        // is installed for that language, AVFoundation silently falls back
        // to the default voice — the UI surfaces a hint when no matching
        // voice exists (see Settings → voiceNotInstalled).
        utterance.voice = AVSpeechSynthesisVoice(language: activeLanguage)
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
        cancelPendingDeactivation()
        if let synth = synthesizer, synth.isSpeaking {
            synth.stopSpeaking(at: .immediate)
        }
        deactivateAudioSession(reason: "shutdown")
    }

    private func ensureSynthesizer() -> AVSpeechSynthesizer {
        if let synth = synthesizer { return synth }
        Self.log.info("ensureSynthesizer — first speak(), constructing AVSpeechSynthesizer + configuring audio session")
        configureAudioSession()
        let synth = AVSpeechSynthesizer()
        // Becoming the delegate is what wires `didFinish` (and friends)
        // back into our debounce — without this, we'd never know when
        // to deactivate the session and music would stay ducked.
        synth.delegate = self
        synthesizer = synth
        return synth
    }

    /// Set the audio session category. Idempotent; the category persists
    /// across `setActive(true/false)` cycles, so this only needs to run
    /// once per process.
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
        #endif
        hasActivatedAudioSession = true
    }

    /// Activate the audio session so the synthesizer has a route AND so
    /// `.duckOthers` actually dims the music. Called from `speak()` on
    /// the first utterance and again whenever a previous deactivation
    /// has fired since.
    private func activateAudioSession() {
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
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
    }

    /// Deactivate with `notifyOthersOnDeactivation` so other audio apps
    /// (Spotify, Apple Music, Podcasts) ramp their volume back up. No-op
    /// when we never activated.
    private func deactivateAudioSession(reason: String) {
        guard hasActivatedAudioSessionRouting else { return }
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setActive(false, options: [.notifyOthersOnDeactivation])
            Self.log.info("setActive(false, .notifyOthersOnDeactivation) — OK (\(reason, privacy: .public))")
        } catch {
            // A common-and-harmless failure: setActive(false) raises
            // `kAudioSessionIncompatibleCategory` when iOS thinks
            // someone else still has the session interrupted. Logged
            // and ignored — the next speak() will re-activate cleanly.
            Self.log.error("setActive(false) failed (\(reason, privacy: .public)): \(error.localizedDescription, privacy: .public)")
        }
        #endif
        hasActivatedAudioSessionRouting = false
    }

    private func cancelPendingDeactivation() {
        if let task = pendingDeactivation {
            task.cancel()
            pendingDeactivation = nil
        }
    }

    private func scheduleDeactivation(reason: String) {
        cancelPendingDeactivation()
        pendingDeactivation = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.deactivationDelaySeconds)
            guard !Task.isCancelled, let self else { return }
            await MainActor.run {
                self.pendingDeactivation = nil
                // A new utterance may have started between sleep
                // beginning and now; the cancellation path normally
                // catches that, but check `isSpeaking` as a belt-and-
                // braces guard against a rapid speak() that didn't
                // race the cancel.
                if let synth = self.synthesizer, synth.isSpeaking {
                    Self.log.debug("deactivation skipped — synthesizer is mid-utterance again")
                    return
                }
                self.deactivateAudioSession(reason: reason)
            }
        }
    }

    func speechSynthesizer(
        _ synthesizer: AVSpeechSynthesizer,
        didFinish utterance: AVSpeechUtterance
    ) {
        Self.log.debug("didFinish — scheduling debounced deactivation")
        scheduleDeactivation(reason: "didFinish")
    }

    func speechSynthesizer(
        _ synthesizer: AVSpeechSynthesizer,
        didCancel utterance: AVSpeechUtterance
    ) {
        // didCancel fires when stopSpeaking(at: .immediate) interrupts
        // an in-flight utterance — typically because a new cue has just
        // arrived. Don't schedule deactivation here: the `speak()` that
        // triggered the cancel will start a new utterance and we want
        // the session to stay hot for it.
        Self.log.debug("didCancel — leaving session active for the cue that just queued")
    }
}
