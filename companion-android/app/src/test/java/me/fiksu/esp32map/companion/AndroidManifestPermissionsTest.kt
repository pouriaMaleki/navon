package me.fiksu.esp32map.companion

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
}
