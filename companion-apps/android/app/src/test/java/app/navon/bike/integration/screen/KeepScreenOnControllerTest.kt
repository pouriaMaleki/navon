package app.navon.bike.integration.screen

import android.app.Activity
import android.app.Application
import android.view.WindowManager
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class KeepScreenOnControllerTest {

    private fun newActivity(): Activity =
        Robolectric.buildActivity(Activity::class.java).create().get()

    @Test
    fun setsKeepScreenOnFlagWhenRequested() {
        val activity = newActivity()
        val controller = KeepScreenOnController(activity)
        controller.update(true)
        val flags = activity.window.attributes.flags
        assertTrue(
            "FLAG_KEEP_SCREEN_ON should be set",
            (flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) != 0,
        )
    }

    @Test
    fun clearsKeepScreenOnFlagWhenDisabled() {
        val activity = newActivity()
        val controller = KeepScreenOnController(activity)
        controller.update(true)
        controller.update(false)
        val flags = activity.window.attributes.flags
        assertEquals(
            "FLAG_KEEP_SCREEN_ON should be cleared",
            0,
            flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON,
        )
    }
}
