package app.navon.bike.integration.notifications

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class LiveUpdateNotificationBuilderTest {

    @Test
    fun buildsOngoingNavigationNotification() {
        val app: Application = ApplicationProvider.getApplicationContext()
        // Pre-create the channel (the Service does this in production).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = app.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(
                NotificationChannel(
                    "esp32-routing",
                    "Route guidance",
                    NotificationManager.IMPORTANCE_LOW,
                ),
            )
        }
        val n = LiveUpdateNotificationBuilder.build(app, "esp32-routing", "Riding", "Turn left in 200m")
        assertEquals("Riding", n.extras.getString(android.app.Notification.EXTRA_TITLE))
        assertEquals("Turn left in 200m", n.extras.getString(android.app.Notification.EXTRA_TEXT))
        assertTrue(
            "Notification must be ongoing",
            (n.flags and android.app.Notification.FLAG_ONGOING_EVENT) != 0,
        )
        // The category is set on the underlying Notification object.
        if (Build.VERSION.SDK_INT >= 34) {
            assertEquals(NotificationCompat.CATEGORY_PROGRESS, n.category)
        } else {
            assertEquals(NotificationCompat.CATEGORY_NAVIGATION, n.category)
        }
    }
}
