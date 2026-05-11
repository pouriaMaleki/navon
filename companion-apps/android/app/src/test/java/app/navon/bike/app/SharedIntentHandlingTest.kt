package app.navon.bike.app

import android.app.Application
import android.content.Intent
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class SharedIntentHandlingTest {
    @Test
    fun consumedShareIntentIsIgnoredOnSubsequentChecks() {
        val intent = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, "60.1699,24.9384")
        }

        assertTrue(shouldHandleSharedIntent(intent))

        markSharedIntentConsumed(intent)

        assertFalse(shouldHandleSharedIntent(intent))
    }

    @Test
    fun nonShareIntentIsIgnored() {
        val intent = Intent(Intent.ACTION_MAIN)

        assertFalse(shouldHandleSharedIntent(intent))
    }
}
