package me.fiksu.esp32map.companion.app

import android.content.Intent
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

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
