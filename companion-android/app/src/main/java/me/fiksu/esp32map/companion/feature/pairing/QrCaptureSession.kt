package me.fiksu.esp32map.companion.feature.pairing

/**
 * Test seam for the camera-side of the pairing flow. The Compose
 * screen drives a [CameraXQrSession] in production; unit tests inject
 * a fake that calls [onScan] with synthetic QR strings.
 *
 * Lifecycles: `start` is idempotent (recompositions can call it
 * repeatedly), `stop` releases the camera and ends the analyzer.
 */
interface QrCaptureSession {
    fun start(onScan: (String?) -> Unit)
    fun stop()
}

/**
 * Pairing-flow scan UX hint. After a few consecutive failed parses we
 * surface a "center the QR in the frame" guidance line so the user
 * knows the issue is alignment, not the camera.
 */
sealed class ScanGuidance {
    object None : ScanGuidance()
    object CenterOnQr : ScanGuidance()
}
