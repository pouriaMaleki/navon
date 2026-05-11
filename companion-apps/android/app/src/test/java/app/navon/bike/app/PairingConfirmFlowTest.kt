package app.navon.bike.app

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import app.navon.bike.domain.PairingFlowState
import app.navon.bike.integration.ble.BleRouteSyncService
import app.navon.bike.integration.ble.FakeRouteSyncBluetoothClient
import app.navon.bike.integration.ble.PairingQrPayload
import app.navon.bike.integration.persistence.CompanionPersistence
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * End-to-end coverage for the pairing confirm-and-persist flow on the
 * companion side. Locks in the rule that no half-state is committed on
 * any failure path (single-bond + no-orphan-record).
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(AndroidJUnit4::class)
class PairingConfirmFlowTest {

    private val payload = PairingQrPayload(
        peripheralIdentifier = "AA:BB:CC:DD:EE:FF",
        ephemeralSecret = ByteArray(32) { 0x42 },
        firmwareVersion = "0.1.0",
    )

    private fun newAppState(
        fake: FakeRouteSyncBluetoothClient,
        persistence: CompanionPersistence,
    ): CompanionAppState {
        val app = ApplicationProvider.getApplicationContext<Application>()
        return CompanionAppState(
            application = app,
            persistenceOverride = persistence,
            bleServiceOverride = BleRouteSyncService(app, fake),
        )
    }

    @Test
    fun completePairing_writesSecretAndPersistsRecord() = runTest {
        val fake = FakeRouteSyncBluetoothClient()
        val persistence = CompanionPersistence(ApplicationProvider.getApplicationContext())
        persistence.clearPairedPeripheral()
        val state = newAppState(fake, persistence)

        state.completePairing(payload, autoDismissDelayMs = 0L)
        advanceUntilIdle()

        assertEquals(1, fake.connectToAdvertisedCallCount)
        assertEquals("AA:BB:CC:DD:EE:FF", fake.lastAdvertisedIdentifier)
        assertEquals(1, fake.writePairingConfirmCallCount)
        assertArrayEquals(payload.ephemeralSecret, fake.lastWrittenPairingSecret)

        val persisted = persistence.loadPairedPeripheral()
        assertEquals("AA:BB:CC:DD:EE:FF", persisted?.identifier)
        assertEquals(PairingFlowState.Idle, state.pairingState)
        assertEquals("AA:BB:CC:DD:EE:FF", state.pairedPeripheral?.identifier)
    }

    @Test
    fun completePairing_doesNotPersistWhenConnectFails() = runTest {
        val fake = FakeRouteSyncBluetoothClient().apply {
            connectAdvertisedResult = Result.failure(IllegalStateException("connect timed out"))
        }
        val persistence = CompanionPersistence(ApplicationProvider.getApplicationContext())
        persistence.clearPairedPeripheral()
        val state = newAppState(fake, persistence)

        state.completePairing(payload, autoDismissDelayMs = 0L)
        advanceUntilIdle()

        assertNull(
            "no record should be persisted when the connect step fails",
            persistence.loadPairedPeripheral(),
        )
        assertNull(state.pairedPeripheral)
        assertTrue(
            "pairingState must surface the failure: ${state.pairingState}",
            state.pairingState is PairingFlowState.Failed,
        )
    }

    @Test
    fun completePairing_doesNotPersistWhenWriteFails() = runTest {
        val fake = FakeRouteSyncBluetoothClient().apply {
            writePairingConfirmResult = Result.failure(IllegalStateException("write failed"))
        }
        val persistence = CompanionPersistence(ApplicationProvider.getApplicationContext())
        persistence.clearPairedPeripheral()
        val state = newAppState(fake, persistence)

        state.completePairing(payload, autoDismissDelayMs = 0L)
        advanceUntilIdle()

        assertNull(persistence.loadPairedPeripheral())
        assertNull(state.pairedPeripheral)
        assertTrue(state.pairingState is PairingFlowState.Failed)
        // Connect did happen but the half-state must not leak — no record.
        assertEquals(1, fake.connectToAdvertisedCallCount)
        assertEquals(1, fake.writePairingConfirmCallCount)
    }

    @Test
    fun completePairing_overwritesPriorPairedRecord() = runTest {
        val fake = FakeRouteSyncBluetoothClient()
        val persistence = CompanionPersistence(ApplicationProvider.getApplicationContext())
        persistence.savePairedPeripheral(
            app.navon.bike.domain.PairedPeripheralRecord(
                identifier = "11:22:33:44:55:66",
                friendlyName = "Old Bond",
                pairedAt = "2024-01-01T00:00:00Z",
            ),
        )
        val state = newAppState(fake, persistence)

        state.completePairing(payload, autoDismissDelayMs = 0L)
        advanceUntilIdle()

        assertEquals(
            "AA:BB:CC:DD:EE:FF",
            persistence.loadPairedPeripheral()?.identifier,
        )
    }
}
