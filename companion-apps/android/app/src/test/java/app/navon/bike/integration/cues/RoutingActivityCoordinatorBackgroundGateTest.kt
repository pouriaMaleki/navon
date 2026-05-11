package app.navon.bike.integration.cues

import android.app.Activity
import android.app.Application
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import app.navon.bike.domain.CompanionSettings
import app.navon.bike.integration.audio.TtsPort
import app.navon.bike.integration.screen.KeepScreenOnController
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Spec line 144: "setting to enable audio cues only when app is in
 * background and enabled by default".
 *
 * Verifies the new `audioCuesOnlyInBackground` gate suppresses cues
 * while the app is foregrounded, and that the default is ON.
 */
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class RoutingActivityCoordinatorBackgroundGateTest {

    private class FakeTts : TtsPort {
        val spoken = mutableListOf<String>()
        var lang: String = "en"
        var voiceAvailable: Boolean = true
        override fun speak(text: String) { spoken += text }
        override fun setLanguage(bcp47: String) { lang = bcp47 }
        override fun hasVoice(forLocale: String): Boolean = voiceAvailable
        override fun shutdown() {}
    }

    private fun snapshot(): CueSnapshot = CueSnapshot(
        routeId = "r1",
        pairedWithDevice = false,
        progressDistanceM = 0.0,
        // A maneuver at 200 m so the first-tick orientation cue fires and
        // the gating tests have a real cue to assert on.
        maneuvers = listOf(CueManeuver("m1", ManeuverKind.RIGHT, 200.0)),
        offRoute = false,
        rerouting = false,
        arrived = false,
        distanceFromRouteM = 0.0,
        routeTotalDistanceM = 1000.0,
    )

    private fun makeCoordinator(): Pair<RoutingActivityCoordinator, FakeTts> {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        val activity = Robolectric.buildActivity(Activity::class.java).get()
        val tts = FakeTts()
        val coordinator = RoutingActivityCoordinator(KeepScreenOnController(activity), tts)
        // Bootstrap i18n so cue lookups inside onGuidanceTick produce
        // English strings to assert against.
        app.navon.bike.integration.i18n.Strings.bootstrap(ctx)
        return coordinator to tts
    }

    @Test
    fun audioCuesOnlyInBackgroundIsOnByDefault() {
        assertTrue(
            "spec line 144: enabled by default",
            CompanionSettings().audioCuesOnlyInBackground,
        )
    }

    @Test
    fun cuesAreSilentInForegroundWhenOnlyInBackgroundIsOn() {
        val (coordinator, tts) = makeCoordinator()
        val settings = CompanionSettings(
            allowBackgroundGps = true,
            audioCuesEnabled = true,
            audioCuesOnlyInBackground = true,
        )
        coordinator.onGuidanceTick(
            snapshot = snapshot(),
            settings = settings,
            isRouting = true,
            isAppInBackground = false,
        )
        assertEquals(emptyList<String>(), tts.spoken)
    }

    @Test
    fun cuesFireInBackgroundWhenOnlyInBackgroundIsOn() {
        val (coordinator, tts) = makeCoordinator()
        val settings = CompanionSettings(
            allowBackgroundGps = true,
            audioCuesEnabled = true,
            audioCuesOnlyInBackground = true,
        )
        coordinator.onGuidanceTick(
            snapshot = snapshot(),
            settings = settings,
            isRouting = true,
            isAppInBackground = true,
        )
        assertTrue("expected at least one cue to fire — got ${tts.spoken}", tts.spoken.isNotEmpty())
    }

    @Test
    fun cuesFireInForegroundWhenOnlyInBackgroundIsOff() {
        val (coordinator, tts) = makeCoordinator()
        val settings = CompanionSettings(
            allowBackgroundGps = true,
            audioCuesEnabled = true,
            audioCuesOnlyInBackground = false,
        )
        coordinator.onGuidanceTick(
            snapshot = snapshot(),
            settings = settings,
            isRouting = true,
            isAppInBackground = false,
        )
        assertTrue("expected at least one cue to fire — got ${tts.spoken}", tts.spoken.isNotEmpty())
    }

    // No-voice fallback. When the OS has no installed TTS voice for the
    // active locale, the cue dispatcher should:
    //   - configure the speech engine with `lang="en"` (so the EN voice
    //     is selected instead of the default voice spelling foreign
    //     glyphs phonetically), AND
    //   - render the cue text in English so it lines up with the voice.

    @Test
    fun speaksEnglishWhenActiveLocaleHasNoVoice() {
        val (coordinator, tts) = makeCoordinator()
        tts.voiceAvailable = false
        val settings = CompanionSettings(
            allowBackgroundGps = true,
            audioCuesEnabled = true,
            audioCuesOnlyInBackground = false,
            language = app.navon.bike.integration.i18n.AppLanguagePref.FA,
        )
        coordinator.onSettingsOrRoutingChange(
            context = ApplicationProvider.getApplicationContext<Context>(),
            settings = settings,
            isRouting = true,
            pairedWithDevice = false,
            title = "",
            body = "",
        )
        assertEquals(
            "TTS should fall back to English when no Persian voice is installed",
            "en",
            tts.lang,
        )
        coordinator.onGuidanceTick(
            snapshot = snapshot(),
            settings = settings,
            isRouting = true,
            isAppInBackground = false,
        )
        assertTrue(
            "cue text should be rendered in English to match the EN voice; got ${tts.spoken}",
            tts.spoken.any { it.contains("Next turn", ignoreCase = true) || it.contains("right", ignoreCase = true) },
        )
    }

    @Test
    fun speaksActiveLocaleWhenVoiceIsAvailable() {
        val (coordinator, tts) = makeCoordinator()
        tts.voiceAvailable = true
        val settings = CompanionSettings(
            allowBackgroundGps = true,
            audioCuesEnabled = true,
            audioCuesOnlyInBackground = false,
            language = app.navon.bike.integration.i18n.AppLanguagePref.FA,
        )
        coordinator.onSettingsOrRoutingChange(
            context = ApplicationProvider.getApplicationContext<Context>(),
            settings = settings,
            isRouting = true,
            pairedWithDevice = false,
            title = "",
            body = "",
        )
        assertEquals(
            "TTS should use the active locale when a voice is installed",
            "fa",
            tts.lang,
        )
    }
}
