package app.navon.bike.app

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import app.navon.bike.domain.PairedPeripheralRecord
import app.navon.bike.domain.PairingFlowState
import app.navon.bike.integration.ble.BleRouteSyncService
import app.navon.bike.integration.ble.FakeRouteSyncBluetoothClient
import app.navon.bike.integration.persistence.CompanionPersistence
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Pairing-related state plumbing on `CompanionAppState`. Locks in:
 *
 * - The bonded record loads from persistence on init (so the chip's
 *   initial state is correct on app launch).
 * - `forgetPairedDevice` clears both the in-memory `mutableStateOf`
 *   and the SharedPreferences key.
 * - `beginPairingFlow` transitions the flow's state machine.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class CompanionAppStatePairingStateTest {

    private fun freshApp(): Application = ApplicationProvider.getApplicationContext()

    private fun sampleRecord() = PairedPeripheralRecord(
        identifier = "AA:BB:CC:DD:EE:FF",
        friendlyName = "Navon",
        pairedAt = "2026-04-28T12:34:56.789Z",
    )

    @Test
    fun loadsPairedPeripheralOnInit_whenStored() = runTest {
        val app = freshApp()
        val persistence = CompanionPersistence(app)
        persistence.savePairedPeripheral(sampleRecord())

        val state = CompanionAppState(
            app,
            persistenceOverride = persistence,
            bleServiceOverride = BleRouteSyncService(app, FakeRouteSyncBluetoothClient()),
        )

        assertEquals(
            "the chip's initial state on launch comes from persistence — must reflect the stored record",
            sampleRecord(),
            state.pairedPeripheral,
        )
    }

    @Test
    fun pairedPeripheralIsNullWhenNothingStored() = runTest {
        val app = freshApp()
        val state = CompanionAppState(
            app,
            persistenceOverride = CompanionPersistence(app),
            bleServiceOverride = BleRouteSyncService(app, FakeRouteSyncBluetoothClient()),
        )
        assertNull(state.pairedPeripheral)
    }

    @Test
    fun forgetPairedDevice_clearsInMemoryAndPersistence() = runTest {
        val app = freshApp()
        val persistence = CompanionPersistence(app)
        persistence.savePairedPeripheral(sampleRecord())
        val state = CompanionAppState(
            app,
            persistenceOverride = persistence,
            bleServiceOverride = BleRouteSyncService(app, FakeRouteSyncBluetoothClient()),
        )

        state.forgetPairedDevice()

        assertNull(
            "in-memory mirror must clear so SwiftUI/Compose re-renders the chip back to Unpaired",
            state.pairedPeripheral,
        )
        assertNull(
            "persistence key must clear so a process restart doesn't resurrect the bond",
            CompanionPersistence(app).loadPairedPeripheral(),
        )
    }

    @Test
    fun beginPairingFlow_transitionsStateToInstructions() = runTest {
        val app = freshApp()
        val state = CompanionAppState(
            app,
            persistenceOverride = CompanionPersistence(app),
            bleServiceOverride = BleRouteSyncService(app, FakeRouteSyncBluetoothClient()),
        )
        assertEquals(PairingFlowState.Idle, state.pairingState)

        state.beginPairingFlow()

        assertEquals(PairingFlowState.Instructions, state.pairingState)
    }

    @Test
    fun forgetPairedDevice_alsoResetsInFlightPairingState() = runTest {
        // Edge case: user starts the pairing flow, then taps Forget on a
        // pre-existing bond before completing. The pairing state must
        // reset so the UI doesn't get stuck on a half-baked sheet.
        val app = freshApp()
        val persistence = CompanionPersistence(app)
        persistence.savePairedPeripheral(sampleRecord())
        val state = CompanionAppState(
            app,
            persistenceOverride = persistence,
            bleServiceOverride = BleRouteSyncService(app, FakeRouteSyncBluetoothClient()),
        )
        state.beginPairingFlow()
        assertNotEquals(PairingFlowState.Idle, state.pairingState)

        state.forgetPairedDevice()

        assertEquals(PairingFlowState.Idle, state.pairingState)
    }
}
