package me.fiksu.esp32map.companion.integration.notifications

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * Foreground service that keeps GPS sampling and the lock-screen notification
 * alive while a route is active. Started by the wiring layer when both
 * `allowBackgroundGps` and (`liveActivityEnabled` or audio cues that need
 * background GPS) are on.
 *
 * Spec lines 130, 144.
 */
class RoutingForegroundService : Service() {

    companion object {
        const val CHANNEL_ID = "esp32-routing"
        const val NOTIFICATION_ID = 4242
        const val NOTIFICATION_TAG = "esp32-routing"
        const val EXTRA_TITLE = "title"
        const val EXTRA_BODY = "body"
        private const val ACTION_UPDATE = "me.fiksu.esp32map.action.UPDATE_ROUTING_NOTIFICATION"

        fun start(context: Context, title: String, body: String) {
            val intent = Intent(context, RoutingForegroundService::class.java).apply {
                action = ACTION_UPDATE
                putExtra(EXTRA_TITLE, title)
                putExtra(EXTRA_BODY, body)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, RoutingForegroundService::class.java))
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val title = intent?.getStringExtra(EXTRA_TITLE) ?: "Riding"
        val body = intent?.getStringExtra(EXTRA_BODY) ?: "On route"
        startForeground(NOTIFICATION_ID, buildNotification(title, body))
        return START_STICKY
    }

    private fun buildNotification(title: String, body: String): Notification {
        return LiveUpdateNotificationBuilder.build(this, CHANNEL_ID, title, body)
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
        if (manager.getNotificationChannel(CHANNEL_ID) == null) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Route guidance",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Live route status while you ride."
                setShowBadge(false)
            }
            manager.createNotificationChannel(channel)
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } else {
            @Suppress("DEPRECATION")
            stopForeground(true)
        }
    }
}

/**
 * Builds the routing notification. On Android 14+ uses Notification Live
 * Updates style (PROGRESS / FOREGROUND_SERVICE category) so the lock screen
 * surfaces it like a media playback. On older versions falls back to a
 * standard ongoing notification.
 */
object LiveUpdateNotificationBuilder {
    fun build(context: Context, channelId: String, title: String, body: String): Notification {
        val builder = NotificationCompat.Builder(context, channelId)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(android.R.drawable.ic_menu_directions)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_NAVIGATION)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
        if (Build.VERSION.SDK_INT >= 34) {
            // Android 14 promoted "live updates" — categorising as PROGRESS
            // surfaces the notification more prominently on the lock screen.
            builder.setCategory(NotificationCompat.CATEGORY_PROGRESS)
        }
        return builder.build()
    }
}
