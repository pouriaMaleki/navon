package app.navon.bike.integration.persistence

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import app.navon.bike.domain.CompanionSettings
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Spec lines 128-145: four new settings (keepScreenOn, allowBackgroundGps,
 * audioCuesEnabled, liveActivityEnabled) added to [CompanionSettings].
 * Verifies defaults, round-trip persistence, and legacy-storage migration
 * (older blobs that lack these keys must load with the spec defaults).
 */
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class CompanionSettingsExpansionTest {

    @Test
    fun defaultsMatchSpec() {
        val defaults = CompanionSettings()
        assertFalse(defaults.keepScreenOn)
        assertFalse(defaults.allowBackgroundGps)
        assertTrue("Audio cues are on by default per spec", defaults.audioCuesEnabled)
        assertFalse(defaults.liveActivityEnabled)
    }

    @Test
    fun roundTripsAllFourNewFields() {
        val app: Application = ApplicationProvider.getApplicationContext()
        val persistence = CompanionPersistence(app)
        val saved = CompanionSettings(
            keepScreenOn = true,
            allowBackgroundGps = true,
            audioCuesEnabled = false,
            liveActivityEnabled = true,
        )
        persistence.saveSettings(saved)

        val reloaded = persistence.loadSettings()
        assertEquals(true, reloaded.keepScreenOn)
        assertEquals(true, reloaded.allowBackgroundGps)
        assertEquals(false, reloaded.audioCuesEnabled)
        assertEquals(true, reloaded.liveActivityEnabled)
    }

    @Test
    fun legacyBlobMissingNewKeysLoadsWithSpecDefaults() {
        val app: Application = ApplicationProvider.getApplicationContext()
        val prefs = app.getSharedPreferences("companion.persistence", android.content.Context.MODE_PRIVATE)
        // Hand-craft a legacy JSON blob — no boolean fields.
        prefs.edit()
            .putString(
                "settings",
                """{"preferLiveHslRouting":false,"hslSubscriptionKey":"","hslEndpointUrl":"https://example.com","cyclingSpeedKph":18.0,"speedUnit":"KPH"}""",
            )
            .apply()

        val persistence = CompanionPersistence(app)
        val loaded = persistence.loadSettings()
        assertFalse(loaded.keepScreenOn)
        assertFalse(loaded.allowBackgroundGps)
        assertTrue("Legacy blob must default audioCuesEnabled to spec value (true)", loaded.audioCuesEnabled)
        assertFalse(loaded.liveActivityEnabled)
    }
}
