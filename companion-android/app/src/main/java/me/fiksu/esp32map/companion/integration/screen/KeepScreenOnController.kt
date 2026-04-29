package me.fiksu.esp32map.companion.integration.screen

import android.app.Activity
import android.view.WindowManager

/**
 * Toggles `FLAG_KEEP_SCREEN_ON` on the given activity's window. The wiring
 * layer feeds it the boolean expression `keepScreenOn && isRouting` so the
 * flag is only held while the rider actually needs the display awake.
 */
class KeepScreenOnController(private val activity: Activity) {

    fun update(shouldKeepOn: Boolean) {
        if (shouldKeepOn) {
            activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }
}
