import Foundation
import os.log
#if canImport(UIKit)
import UIKit
#endif

/// Bridges app state to the four routing-time side-effect services. The
/// caller assembles a `CueSnapshot` each guidance tick and calls
/// `onGuidanceTick`; settings/routing transitions go through
/// `onSettingsOrRoutingChange`. Single source of truth for the gating
/// expressions:
///
///  - Idle timer disabled iff `keepScreenOn && isRouting`
///  - Cues active iff `audioCuesEnabled && allowBackgroundGps && !pairedWithDevice && isRouting`
///  - Live activity active iff `liveActivityEnabled && allowBackgroundGps && isRouting`
///
/// `onGuidanceTick` ALSO refreshes the Live Activity content so iOS does
/// not dismiss the activity for staleness mid-ride. A previous design
/// only refreshed the activity from `onSettingsOrRoutingChange` (which
/// fires only on settings toggles), which is why the lock-screen activity
/// silently disappeared during long rides.
@MainActor
final class RoutingActivityCoordinator {
    private static let log = Logger(
        subsystem: "me.fiksu.esp32map.companion.ios",
        category: "audio"
    )
    private let idleTimer: IdleTimerController
    private let speech: SpeechPort
    private let liveActivity: LiveActivityPort
    private var cueState = CueEngineState()
    private var liveActivityRunning = false

    init(
        idleTimer: IdleTimerController,
        speech: SpeechPort,
        liveActivity: LiveActivityPort
    ) {
        self.idleTimer = idleTimer
        self.speech = speech
        self.liveActivity = liveActivity
    }

    /// BCP-47 tag the speech engine is currently configured to speak. Equal
    /// to `T.activeBcp47` when the OS has a voice for that locale; falls
    /// back to "en" otherwise so cues are intelligible instead of being
    /// spelled letter-by-letter.
    private var ttsBcp47: String = "en"

    func onSettingsOrRoutingChange(
        settings: CompanionSettings,
        isRouting: Bool,
        pairedWithDevice: Bool,
        liveActivityContent: RoutingLiveActivityContent?
    ) {
        // Push the user's language preference to the i18n runtime + TTS so
        // the next `T.string(...)` call and the next `speech.speak(...)`
        // both render in the chosen locale. If the OS has no voice for the
        // active locale, fall back to English audio (`speech.speak(..)`
        // uses the EN voice + cues are rendered via `T.stringIn(.en, ...)`)
        // — otherwise iOS spells Persian/Arabic glyphs phonetically via
        // the default English voice.
        let locale = T.resolveLocale(settings.language)
        T.setActiveLocale(locale)
        ttsBcp47 = speech.hasVoice(forLocale: locale.rawValue) ? locale.rawValue : "en"
        speech.setLanguage(ttsBcp47)

        idleTimer.update(settings.keepScreenOn && isRouting)
        if !isRouting { cueState = CueEngineState() }
        applyLiveActivity(
            settings: settings,
            isRouting: isRouting,
            content: liveActivityContent
        )
    }

    func onGuidanceTick(
        snapshot: CueSnapshot,
        settings: CompanionSettings,
        isRouting: Bool,
        isAppInBackground: Bool = false,
        liveActivityContent: RoutingLiveActivityContent? = nil
    ) {
        let cuesActive = isRouting &&
            settings.audioCuesEnabled &&
            settings.allowBackgroundGps &&
            !snapshot.pairedWithDevice &&
            (!settings.audioCuesOnlyInBackground || isAppInBackground)
        Self.log.debug(
            "onGuidanceTick — isRouting=\(isRouting) audioCues=\(settings.audioCuesEnabled) bgGps=\(settings.allowBackgroundGps) paired=\(snapshot.pairedWithDevice) onlyBg=\(settings.audioCuesOnlyInBackground) bg=\(isAppInBackground) → cuesActive=\(cuesActive) progressM=\(snapshot.progressDistanceM, privacy: .public) routeId=\(snapshot.routeId ?? "nil", privacy: .public)"
        )
        if cuesActive {
            let result = CueEngine.tick(snapshot: snapshot, state: cueState)
            cueState = result.nextState
            if !result.events.isEmpty {
                Self.log.info("CueEngine emitted \(result.events.count) event(s) on this tick")
            }
            let distanceMode = T.resolveDistanceUnit(settings.distanceUnit)
            // Recompute every tick. `onSettingsOrRoutingChange` only fires
            // on settings or routing transitions, so on a cold launch the
            // first guidance tick can arrive before the coordinator has
            // ever been told about the user's locale — `ttsBcp47` would
            // still be its `"en"` default and the rider would hear English
            // even with Suomi (or whatever) configured. Computing fresh
            // here also handles the rare case of a voice being installed
            // mid-session via the OS settings.
            let activeLocale = T.resolveLocale(settings.language)
            let activeTag = activeLocale.rawValue
            let resolvedTtsTag = speech.hasVoice(forLocale: activeTag) ? activeTag : "en"
            if resolvedTtsTag != ttsBcp47 {
                ttsBcp47 = resolvedTtsTag
                speech.setLanguage(resolvedTtsTag)
            }
            let renderInFallback = ttsBcp47 != activeTag
            for event in result.events {
                let msg = CueEngine.cueMessage(event, distanceMode: distanceMode)
                let phrase = renderInFallback
                    ? T.stringIn(.en, msg.key, msg.values)
                    : T.string(msg.key, msg.values)
                Self.log.info("→ speak \"\(phrase, privacy: .public)\"")
                speech.speak(phrase)
            }
        }
        // Even on ticks where cues don't fire, refresh the Live Activity so
        // iOS keeps showing the lock-screen card. Without these updates the
        // activity gets dismissed for staleness within a few minutes.
        applyLiveActivity(
            settings: settings,
            isRouting: isRouting,
            content: liveActivityContent
        )
    }

    private func applyLiveActivity(
        settings: CompanionSettings,
        isRouting: Bool,
        content: RoutingLiveActivityContent?
    ) {
        let liveOn = isRouting && settings.liveActivityEnabled && settings.allowBackgroundGps
        if liveOn, let content = content {
            if liveActivityRunning {
                liveActivity.update(content)
            } else {
                liveActivity.start(content)
                liveActivityRunning = true
            }
        } else if liveActivityRunning {
            liveActivity.end()
            liveActivityRunning = false
        }
    }
}
