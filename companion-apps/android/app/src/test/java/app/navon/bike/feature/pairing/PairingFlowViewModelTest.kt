package app.navon.bike.feature.pairing

import app.navon.bike.domain.PairingFlowState
import app.navon.bike.integration.ble.PairingQrPayload
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PairingFlowViewModelTest {

    private val validJson = """{"v":1,"id_android":"AA:BB:CC:DD:EE:FF","secret":"$VALID_SECRET","fw":"0.1.0"}"""

    @Test
    fun qrCallback_validPayload_advancesPairingStateToConnecting() {
        var dispatched: PairingQrPayload? = null
        val vm = PairingFlowViewModel(onPayloadDecoded = { dispatched = it })
        vm.handleScannedRawValue(validJson)
        assertEquals(PairingFlowState.Connecting, vm.pairingState)
        assertNotNull(vm.lastDecodedPayload)
        assertEquals("AA:BB:CC:DD:EE:FF", vm.lastDecodedPayload?.peripheralIdentifier)
        assertEquals(
            "AA:BB:CC:DD:EE:FF",
            dispatched?.peripheralIdentifier,
        )
    }

    @Test
    fun qrCallback_invalidPayload_setsHumanReadableError() {
        val vm = PairingFlowViewModel()
        vm.handleScannedRawValue("garbage")
        assertEquals(PairingFlowState.Scanning, vm.pairingState)
        val message = vm.scanErrorMessage
        assertTrue("expected non-empty error: $message", !message.isNullOrEmpty())
    }

    @Test
    fun qrCallback_nullBarcodeRawValue_isHandledSafely() {
        val vm = PairingFlowViewModel()
        vm.handleScannedRawValue(null)
        assertEquals(PairingFlowState.Scanning, vm.pairingState)
        assertNull(vm.lastDecodedPayload)
    }

    @Test
    fun threeConsecutiveInvalidScans_promptCenterOnQr() {
        val vm = PairingFlowViewModel()
        vm.handleScannedRawValue("garbage1")
        vm.handleScannedRawValue("garbage2")
        vm.handleScannedRawValue("garbage3")
        assertEquals(ScanGuidance.CenterOnQr, vm.scanGuidance)
    }

    @Test
    fun cancel_resetsAllStateBackToIdle() {
        val vm = PairingFlowViewModel()
        vm.handleScannedRawValue("garbage")
        vm.handleScannedRawValue("garbage")
        vm.handleScannedRawValue("garbage")
        vm.cancel()
        assertEquals(PairingFlowState.Idle, vm.pairingState)
        assertNull(vm.scanErrorMessage)
        assertEquals(ScanGuidance.None, vm.scanGuidance)
        assertNull(vm.lastDecodedPayload)
    }

    @Test
    fun reportPairingResult_surfacesFailedTransition() {
        val vm = PairingFlowViewModel()
        vm.handleScannedRawValue(validJson)
        vm.reportPairingResult(PairingFlowState.Failed("connection refused"))
        assertTrue(vm.pairingState is PairingFlowState.Failed)
    }

    @Test
    fun successfulScanAfterFailures_clearsErrorAndGuidance() {
        val vm = PairingFlowViewModel()
        vm.handleScannedRawValue("bad1")
        vm.handleScannedRawValue("bad2")
        vm.handleScannedRawValue(validJson)
        assertNull(
            "a successful scan should clear the residual error so the UI doesn't keep showing a stale 'malformed' toast",
            vm.scanErrorMessage,
        )
        assertEquals(ScanGuidance.None, vm.scanGuidance)
    }

    private companion object {
        private val VALID_SECRET: String =
            "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI="
    }
}
