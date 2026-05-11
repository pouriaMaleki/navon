package app.navon.bike.integration.ble

import app.navon.bike.domain.DeviceConnectionState
import app.navon.bike.domain.RouteSyncFaultInjectionMode
import app.navon.bike.domain.RouteSyncMessage

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

    /**
     * Connect to an as-yet-unbonded peripheral discovered through the
     * pairing flow's QR scan. The QR provides the BD_ADDR up front so
     * we don't need a service-UUID scan; if the peer isn't directly
     * connectable yet (e.g. it just rebooted into pairing mode), the
     * implementation falls back to a targeted scan filtered by
     * `device.address == identifier`.
     */
    suspend fun connectToAdvertisedPeripheral(identifier: String): String

    /**
     * Write the 32-byte ephemeral secret pulled from the QR to the
     * firmware's pairing-confirm characteristic. The first encrypted
     * write triggers SMP Just Works pairing transparently; on success
     * the device transitions to `Operational` and the bond is persisted
     * to NVS.
     */
    suspend fun writePairingConfirm(secret: ByteArray)

    /**
     * Write a single byte to the firmware's pairing-request characteristic
     * (unencrypted) to signal "user tapped Pair new device". The firmware
     * responds by switching its panel from the map to the QR overlay so the
     * user can scan the secret. Must be called *before* the camera opens —
     * otherwise the device is still showing the map and there's nothing to
     * scan.
     */
    suspend fun writePairingRequest()

    suspend fun write(packet: BleRouteSyncPacket)

    /** Write a phone GPS sample as CSV: lat,lon,speed,course,accuracy */
    suspend fun writePhoneGpsSample(lat: Double, lon: Double, speed: Double, course: Double?, accuracy: Double?)
}
