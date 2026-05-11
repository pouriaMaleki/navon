package app.navon.bike.feature.home

import app.navon.bike.domain.DeviceConnectionState
import app.navon.bike.domain.PairedPeripheralRecord
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Pure-derivation tests for the home-screen device chip. No Compose
 * rendering — exercises [deviceChipStateOf] and [deviceChipTapActionFor]
 * directly so the cross-platform contract (same shape on iOS) is
 * locked at the data layer.
 */
class DeviceStatusChipStateTest {

    private fun record() = PairedPeripheralRecord(
        identifier = "AA:BB:CC:DD:EE:FF",
        friendlyName = "Navon",
        pairedAt = "2026-04-28T12:34:56.789Z",
    )

    @Test
    fun unpaired_whenNoRecord() {
        for (connection in DeviceConnectionState.entries) {
            assertEquals(
                "any connection state collapses to Unpaired when no record is stored — connection=$connection",
                DeviceChipState.Unpaired,
                deviceChipStateOf(paired = null, connection = connection),
            )
        }
    }

    @Test
    fun pairedDisconnected_whenRecordButStateDisconnected() {
        val state = deviceChipStateOf(record(), DeviceConnectionState.DISCONNECTED)
        assertEquals(
            DeviceChipState.PairedDisconnected("Navon"),
            state,
        )
    }

    @Test
    fun connecting_whenScanning() {
        val state = deviceChipStateOf(record(), DeviceConnectionState.SCANNING)
        assertEquals(DeviceChipState.Connecting("Navon"), state)
    }

    @Test
    fun connecting_whenConnecting() {
        // Two separate cases so a future enum split (e.g. adding
        // DISCOVERING) doesn't silently re-route one of these to the
        // wrong chip state.
        val state = deviceChipStateOf(record(), DeviceConnectionState.CONNECTING)
        assertEquals(DeviceChipState.Connecting("Navon"), state)
    }

    @Test
    fun connected_whenConnectedAndRecordPresent() {
        val state = deviceChipStateOf(record(), DeviceConnectionState.CONNECTED)
        assertEquals(DeviceChipState.Connected("Navon"), state)
    }

    @Test
    fun chipTapActions_dispatchTheRightActionPerState() {
        // Locks tap-target intent so an accidental swap (e.g. routing
        // Unpaired's tap to ConnectToDevice instead of BeginPairingFlow)
        // fails loudly.
        assertEquals(
            DeviceChipTapAction.BeginPairingFlow,
            deviceChipTapActionFor(DeviceChipState.Unpaired),
        )
        assertEquals(
            DeviceChipTapAction.ConnectToDevice,
            deviceChipTapActionFor(DeviceChipState.PairedDisconnected("X")),
        )
        assertEquals(
            DeviceChipTapAction.ShowConnectedPopover,
            deviceChipTapActionFor(DeviceChipState.Connected("X")),
        )
        assertEquals(
            DeviceChipTapAction.Noop,
            deviceChipTapActionFor(DeviceChipState.Connecting("X")),
        )
    }
}
