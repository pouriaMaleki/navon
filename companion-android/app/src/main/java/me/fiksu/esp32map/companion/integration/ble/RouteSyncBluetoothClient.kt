package me.fiksu.esp32map.companion.integration.ble

import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode
import me.fiksu.esp32map.companion.domain.RouteSyncMessage

/**
 * Mockable surface of the BLE route-sync client. The concrete
 * implementation [AndroidBleRouteSyncClient] talks to CoreBluetooth's
 * Android equivalent (BluetoothManager + BluetoothLeScanner +
 * BluetoothGatt); test code substitutes a fake without bringing up a
 * Bluetooth stack.
 *
 * Mirrors `companion-ios/.../RouteSyncBluetoothClient.swift` in shape so
 * cross-platform pairing-flow tests stay in lockstep.
 */
interface RouteSyncBluetoothClient {
    var onSyncMessage: ((RouteSyncMessage) -> Unit)?
    var onConnectionStateChange: ((DeviceConnectionState, String?) -> Unit)?
    val isReady: Boolean

    fun armDebugFault(mode: RouteSyncFaultInjectionMode)

    suspend fun scanForRouteSyncPeripheral(timeoutMs: Long = 6_000): String

    suspend fun connectToScannedPeripheral(): String

    /**
     * Reconnect directly to a previously-bonded peripheral by its
     * platform-native identifier (BLE MAC address on Android). Skips
     * scanning when the peer is already cached/in-range. Fall back to
     * [scanForRouteSyncPeripheral] + [connectToScannedPeripheral] when
     * this throws.
     */
    suspend fun connectToPairedPeripheral(identifier: String): String

    suspend fun write(packet: BleRouteSyncPacket)
}
