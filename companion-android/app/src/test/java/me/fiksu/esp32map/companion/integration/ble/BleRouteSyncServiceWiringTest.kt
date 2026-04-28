package me.fiksu.esp32map.companion.integration.ble

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.RouteSyncState
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class BleRouteSyncServiceWiringTest {

    /**
     * Pure plumbing: confirms `BleRouteSyncService` accepts the
     * `RouteSyncBluetoothClient` interface (rather than the concrete
     * `AndroidBleRouteSyncClient` class). Lets pairing-flow tests
     * inject [FakeRouteSyncBluetoothClient] so they don't need a real
     * Bluetooth stack to run.
     */
    @Test
    fun serviceAcceptsFakeBluetoothClient() {
        val context = ApplicationProvider.getApplicationContext<Application>()
        val fake = FakeRouteSyncBluetoothClient()

        val service = BleRouteSyncService(context, bluetoothClient = fake)

        val sessionState = service.state.value
        assertEquals(DeviceConnectionState.DISCONNECTED, sessionState.connectionState)
        assertEquals(RouteSyncState.IDLE, sessionState.routeSyncState)
    }
}
