package me.fiksu.esp32map.companion.integration.cues

import android.content.Context
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.integration.audio.TtsPort
import me.fiksu.esp32map.companion.integration.i18n.Strings
import me.fiksu.esp32map.companion.integration.notifications.RoutingForegroundService
import me.fiksu.esp32map.companion.integration.screen.KeepScreenOnController

/**
 * Bridges app state to the four routing-time side-effect services. Holds the
 * single source of truth for the gating expressions:
 *
 *  - WakeLock active iff `keepScreenOn && isRouting`
 *  - Cues active iff `audioCuesEnabled && allowBackgroundGps && !pairedWithDevice && isRouting`
 *  - Foreground service active iff `(liveActivityEnabled || cuesActive) && allowBackgroundGps && isRouting`
 *
 * The caller assembles a [CueSnapshot] each guidance tick (the wire-up to
 * per-tick guidance state is a follow-up; for now only the screen-on toggle
 * and the foreground service get exercised on every settings change). When
 * cues become available, call [onGuidanceTick].
 */
/** Composite key for [CueSnapshot.routeId] so a revision bump on the same
 *  route identifier is treated as a genuine route change by [CueEngine]. */
fun buildRouteKey(routeIdentifier: String?, routeRevision: Int?): String? {
    if (routeIdentifier == null) return null
    return "$routeIdentifier-rev${routeRevision ?: 0}"
}

class RoutingActivityCoordinator(
    private val keepScreenOn: KeepScreenOnController,
    private val tts: TtsPort,
) {
    private var cueState: CueEngineState = CueEngineState()
    private var foregroundServiceRunning = false
    /** BCP-47 tag the speech engine is currently configured with. Equals
     *  the active locale's tag when Android has a voice for it, otherwise
     *  "en" — the rider gets intelligible English audio instead of the
     *  default voice spelling foreign glyphs letter-by-letter. */
    private var ttsBcp47: String = "en"

    fun onSettingsOrRoutingChange(
        context: Context,
        settings: CompanionSettings,
        isRouting: Boolean,
        pairedWithDevice: Boolean,
        title: String,
        body: String,
    ) {
        // Push the user's language preference to the i18n runtime + TTS so
        // subsequent `Strings.t(...)` calls and the next utterance render
        // in the chosen locale. If Android has no voice for that locale,
        // fall back to English audio so cues are intelligible — the UI
        // still renders in the chosen language.
        val locale = Strings.resolveLocale(settings.language)
        Strings.setActiveLocale(locale)
        ttsBcp47 = if (tts.hasVoice(locale.tag)) locale.tag else "en"
        tts.setLanguage(ttsBcp47)

        keepScreenOn.update(settings.keepScreenOn && isRouting)

        val cuesActive = isRouting &&
            settings.audioCuesEnabled &&
            settings.allowBackgroundGps &&
            !pairedWithDevice
        if (!isRouting) cueState = CueEngineState()

        val liveActivityActive = isRouting &&
            settings.allowBackgroundGps &&
            (settings.liveActivityEnabled || cuesActive)
        if (liveActivityActive) {
            RoutingForegroundService.start(context, title, body)
            foregroundServiceRunning = true
        } else if (foregroundServiceRunning) {
            RoutingForegroundService.stop(context)
            foregroundServiceRunning = false
        }
    }

    fun onGuidanceTick(
        snapshot: CueSnapshot,
        settings: CompanionSettings,
        isRouting: Boolean,
        isAppInBackground: Boolean = false,
        isExploringAlternativesFromGuidance: Boolean = false,
    ) {
        val cuesActive = isRouting &&
            settings.audioCuesEnabled &&
            settings.allowBackgroundGps &&
            !snapshot.pairedWithDevice &&
            (!settings.audioCuesOnlyInBackground || isAppInBackground) &&
            !isExploringAlternativesFromGuidance
        if (!cuesActive) return
        val result = CueEngine.tick(snapshot, cueState)
        cueState = result.nextState
        val distanceMode = Strings.resolveDistanceUnit(settings.distanceUnit)
        // Recompute every tick — `onSettingsOrRoutingChange` is the only
        // other place that touches `ttsBcp47`, but on a cold launch the
        // first guidance tick can arrive before any settings transition,
        // so the field would still be its "en" default and the rider
        // would hear English even with Suomi configured.
        val activeLocale = Strings.resolveLocale(settings.language)
        val resolvedTtsTag = if (tts.hasVoice(activeLocale.tag)) activeLocale.tag else "en"
        if (resolvedTtsTag != ttsBcp47) {
            ttsBcp47 = resolvedTtsTag
            tts.setLanguage(resolvedTtsTag)
        }
        val renderInFallback = ttsBcp47 != activeLocale.tag
        for (event in result.events) {
            val msg = CueEngine.cueMessage(event, distanceMode)
            val phrase = if (renderInFallback) {
                Strings.tIn(
                    me.fiksu.esp32map.companion.integration.i18n.SupportedLocale.EN,
                    msg.key,
                    msg.values,
                )
            } else {
                Strings.t(msg.key, msg.values)
            }
            tts.speak(phrase)
        }
    }
}
