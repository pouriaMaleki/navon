package app.navon.bike.feature.pairing

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.navon.bike.domain.PairingFlowState
import app.navon.bike.integration.ble.PairingQrError
import app.navon.bike.integration.ble.PairingQrPayload

/**
 * Backs the pairing screen's QR-scan step. Decoupled from
 * [app.navon.bike.app.CompanionAppState] so the unit
 * tests can drive the state machine without booting the entire app.
 *
 * The screen wires:
 *   - `qrCallback` into a [QrCaptureSession] (CameraX + ML Kit in prod,
 *     a fake in tests).
 *   - `onPayloadDecoded` to hand the decoded payload to the AppState's
 *     `completePairing` step.
 */
class PairingFlowViewModel(
    private val onPayloadDecoded: (PairingQrPayload) -> Unit = {},
) {
    var pairingState: PairingFlowState by mutableStateOf(PairingFlowState.Scanning)
        private set

    var lastDecodedPayload: PairingQrPayload? by mutableStateOf(null)
        private set

    var scanErrorMessage: String? by mutableStateOf(null)
        private set

    var scanGuidance: ScanGuidance by mutableStateOf(ScanGuidance.None)
        private set

    private var consecutiveFailures: Int = 0

    /** Wire as `qrCaptureSession.start(::handleScannedRawValue)`. */
    fun handleScannedRawValue(rawValue: String?) {
        if (rawValue == null) {
            // ML Kit can hand back a Barcode whose `rawValue` is null
            // even when the format is detected — silently keep waiting
            // for the next frame.
            return
        }
        val payload = try {
            PairingQrPayload.decode(rawValue)
        } catch (e: PairingQrError) {
            consecutiveFailures += 1
            scanErrorMessage = humanReadable(e)
            if (consecutiveFailures >= CENTER_ON_QR_PROMPT_THRESHOLD) {
                scanGuidance = ScanGuidance.CenterOnQr
            }
            return
        }

        consecutiveFailures = 0
        scanErrorMessage = null
        scanGuidance = ScanGuidance.None
        lastDecodedPayload = payload
        pairingState = PairingFlowState.Connecting
        onPayloadDecoded(payload)
    }

    /** Cancel the flow from any step; the screen calls `qrCaptureSession.stop()` separately. */
    fun cancel() {
        pairingState = PairingFlowState.Idle
        scanErrorMessage = null
        scanGuidance = ScanGuidance.None
        consecutiveFailures = 0
        lastDecodedPayload = null
    }

    /** Surface a transition when external state (BLE / persistence) reports a result. */
    fun reportPairingResult(state: PairingFlowState) {
        pairingState = state
    }

    private fun humanReadable(error: PairingQrError): String = when (error) {
        is PairingQrError.MissingField -> "Pairing code is missing the ${error.field} field"
        is PairingQrError.MalformedField -> "Pairing code's ${error.field} field is malformed"
        is PairingQrError.UnsupportedVersion ->
            "Pairing code uses an unsupported schema version (v${error.version})"
        is PairingQrError.MalformedJson -> "Pairing code couldn't be read"
    }

    private companion object {
        private const val CENTER_ON_QR_PROMPT_THRESHOLD = 3
    }
}
