package app.navon.bike

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidManifestPermissionsTest {
    @Test
    fun manifest_includesCameraPermission() {
        // Robolectric's `PackageManager.getPackageInfo(GET_PERMISSIONS)` returns
        // an empty `requestedPermissions` array under JUnit unit tests, so read
        // the manifest XML directly. Tests run with cwd = `companion-android/app/`.
        val manifest = File("src/main/AndroidManifest.xml").readText()
        assertTrue(
            "android.permission.CAMERA must be in the manifest so the pairing flow can " +
                "open the camera; manifest: $manifest",
            manifest.contains("android.permission.CAMERA"),
        )
    }

    @Test
    fun manifest_includesBackgroundLocationAndForegroundServicePermissions() {
        // Spec lines 130, 144: allow GPS in background + lock-screen live activity.
        val manifest = File("src/main/AndroidManifest.xml").readText()
        assertTrue(
            "ACCESS_BACKGROUND_LOCATION must be declared",
            manifest.contains("android.permission.ACCESS_BACKGROUND_LOCATION"),
        )
        assertTrue(
            "FOREGROUND_SERVICE_LOCATION must be declared (Android 14)",
            manifest.contains("android.permission.FOREGROUND_SERVICE_LOCATION"),
        )
        assertTrue(
            "POST_NOTIFICATIONS must be declared (Android 13)",
            manifest.contains("android.permission.POST_NOTIFICATIONS"),
        )
        assertTrue(
            "RoutingForegroundService must be declared with foregroundServiceType=location",
            manifest.contains("RoutingForegroundService") &&
                manifest.contains("android:foregroundServiceType=\"location\""),
        )
    }
}
