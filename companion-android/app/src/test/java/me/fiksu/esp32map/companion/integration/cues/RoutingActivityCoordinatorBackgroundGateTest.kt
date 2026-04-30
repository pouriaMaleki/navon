package me.fiksu.esp32map.companion.integration.cues

import android.app.Activity
import android.app.Application
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.integration.audio.TtsPort
import me.fiksu.esp32map.companion.integration.screen.KeepScreenOnController
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
        override fun speak(text: String) { spoken += text }
        override fun setLanguage(bcp47: String) { lang = bcp47 }
        override fun shutdown() {}
    }

    private fun snapshot(): CueSnapshot = CueSnapshot(
        routeId = "r1",
        pairedWithDevice = false,
        progressDistanceM = 0.0,
        maneuvers = emptyList(),
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
        me.fiksu.esp32map.companion.integration.i18n.Strings.bootstrap(ctx)
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
        assertTrue("expected at least 'Route started' cue", tts.spoken.contains("Route started"))
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
        assertTrue("expected at least 'Route started' cue", tts.spoken.contains("Route started"))
    }
}
