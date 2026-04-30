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
class RoutingActivityCoordinator(
    private val keepScreenOn: KeepScreenOnController,
    private val tts: TtsPort,
) {
    private var cueState: CueEngineState = CueEngineState()
    private var foregroundServiceRunning = false

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
        // in the chosen locale.
        val locale = Strings.resolveLocale(settings.language)
        Strings.setActiveLocale(locale)
        tts.setLanguage(locale.tag)

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
    ) {
        val cuesActive = isRouting &&
            settings.audioCuesEnabled &&
            settings.allowBackgroundGps &&
            !snapshot.pairedWithDevice &&
            (!settings.audioCuesOnlyInBackground || isAppInBackground)
        if (!cuesActive) return
        val result = CueEngine.tick(snapshot, cueState)
        cueState = result.nextState
        val distanceMode = Strings.resolveDistanceUnit(settings.distanceUnit)
        for (event in result.events) {
            val msg = CueEngine.cueMessage(event, distanceMode)
            tts.speak(Strings.t(msg.key, msg.values))
        }
    }
}
