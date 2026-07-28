package app.navon.bike.integration.ble.navdevice

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withTimeoutOrNull
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.DeviceConnectionState
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.integration.ble.navdevice.beeline.BeelineDevice
import app.navon.bike.integration.ble.navdevice.beeline.BeelineRouteController
import app.navon.bike.integration.ble.navdevice.beeline.BeelineScanner

/**
 * Observable snapshot of the Beeline session, mirroring (a subset of) the
 * shape of the ESP32 route-sync `SyncSessionState` so the home UI can render
 * either device through one device-chip vocabulary.
 */
data class BeelineSessionState(
    val connectionState: DeviceConnectionState = DeviceConnectionState.DISCONNECTED,
    val deviceName: String? = null,
    val deviceAddress: String? = null,
    val batteryLevel: Int? = null,
    val isCharging: Boolean = false,
    val firmwareVersion: String? = null,
    val activeRouteIdentifier: String? = null,
    val lastError: String? = null,
)

/**
 * Owns the Beeline [NavDevice] + its [BeelineScanner] and drives a
 * [BeelineRouteController], exposing a small lifecycle the app layer can call
 * without touching the BLE protocol directly.
 *
 * This is the Beeline counterpart to `BleRouteSyncService` (the ESP32 path).
 * Only one device is active at a time — see [app.navon.bike.domain.PairedDeviceType] —
 * so the app constructs and feeds whichever manager matches the paired
 * record's device type.
 *
 * Pairing model: unlike the ESP32 QR-OOB flow, a Beeline is discovered by a
 * BLE name scan ("Beeline"/"Velo"/"Moto"); [pair] scans, connects (the device
 * bonds on first connect), and resolves once the connection callback fires.
 */
class BeelineDeviceManager(
    context: Context,
    planningSpeedKph: Double = 18.0,
) {
    private val appContext = context.applicationContext
    private val bluetoothManager =
        appContext.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
    private val adapter get() = bluetoothManager.adapter

    private val device = BeelineDevice(appContext)
    private val scanner = BeelineScanner(appContext)
    private val controller = BeelineRouteController(device, planningSpeedKph)

    private val _state = MutableStateFlow(BeelineSessionState())
    val state: StateFlow<BeelineSessionState> = _state.asStateFlow()

    /** Completes the next connect attempt's await; set in [connectInternal]. */
    private var connectGate: CompletableDeferred<Boolean>? = null

    init {
        device.onConnectionStateChanged = { connected ->
            connectGate?.takeIf { !it.isCompleted }?.complete(connected)
            _state.value = _state.value.copy(
                connectionState = if (connected) DeviceConnectionState.CONNECTED
                else DeviceConnectionState.DISCONNECTED,
                deviceName = device.lastConnectedDevice?.let { safeName(it) } ?: _state.value.deviceName,
                deviceAddress = device.lastConnectedDevice?.address ?: _state.value.deviceAddress,
                activeRouteIdentifier = if (connected) _state.value.activeRouteIdentifier else null,
            )
        }
        device.onPowerStatus = { status ->
            _state.value = _state.value.copy(
                batteryLevel = status.batteryLevel,
                isCharging = status.isCharging,
            )
        }
        device.onFirmwareVersion = { fw ->
            _state.value = _state.value.copy(firmwareVersion = fw)
        }
    }

    val isConnected: Boolean
        get() = _state.value.connectionState == DeviceConnectionState.CONNECTED

    /**
     * Scan for a nearby Beeline, connect, and await the connection. On success
     * returns the discovered [ScannedDevice] (address + name) so the caller can
     * persist a paired record.
     *
     * If the device is not yet bonded, the OS shows its pairing dialog; the
     * first attempt may time out while bonding completes — call again to finish.
     */
    suspend fun pair(timeoutMs: Long = 12_000L): Result<ScannedDevice> {
        if (!scanner.isBluetoothEnabled()) {
            return fail("Bluetooth is off")
        }
        _state.value = _state.value.copy(connectionState = DeviceConnectionState.SCANNING, lastError = null)
        val found = scanForFirstDevice(timeoutMs) ?: run {
            _state.value = _state.value.copy(connectionState = DeviceConnectionState.DISCONNECTED)
            return fail("No Beeline found nearby")
        }
        val connected = connectInternal(found.address, timeoutMs)
        return if (connected) {
            _state.value = _state.value.copy(deviceName = found.name, deviceAddress = found.address)
            Result.success(found)
        } else {
            fail("Couldn't connect to ${found.name ?: found.address}")
        }
    }

    /** Fast-path reconnect to a previously bonded Beeline by MAC address. */
    suspend fun connect(address: String, timeoutMs: Long = 12_000L): Boolean =
        connectInternal(address, timeoutMs)

    private suspend fun connectInternal(address: String, timeoutMs: Long): Boolean {
        val remote = runCatching { adapter?.getRemoteDevice(address) }.getOrNull() ?: run {
            _state.value = _state.value.copy(lastError = "Invalid device address")
            return false
        }
        _state.value = _state.value.copy(
            connectionState = DeviceConnectionState.CONNECTING,
            deviceAddress = address,
            lastError = null,
        )

        // A Beeline bonds on first connect: the underlying connect path calls
        // createBond() and returns without a GATT link when unbonded, so the
        // OS pairing dialog must be accepted before we can actually connect.
        // Wait for the system bond to land, then connect for real.
        if (remote.bondState != BluetoothDevice.BOND_BONDED) {
            if (!awaitBond(remote, BOND_TIMEOUT_MS)) {
                _state.value = _state.value.copy(
                    connectionState = DeviceConnectionState.DISCONNECTED,
                    lastError = "Pairing not confirmed — accept the prompt and try again",
                )
                return false
            }
        }

        val gate = CompletableDeferred<Boolean>()
        connectGate = gate
        device.connectToDevice(remote)
        val result = withTimeoutOrNull(timeoutMs) { gate.await() } ?: false
        connectGate = null
        if (!result) {
            _state.value = _state.value.copy(connectionState = DeviceConnectionState.DISCONNECTED)
        }
        return result
    }

    /**
     * Trigger bonding with [remote] (via the device's connect path, which calls
     * `createBond()` when unbonded) and suspend until the system reports the
     * bond completed or failed. Returns true once bonded.
     */
    @Suppress("DEPRECATION") // EXTRA_DEVICE typed getter needs API 33; minSdk is 29.
    private suspend fun awaitBond(remote: BluetoothDevice, timeoutMs: Long): Boolean {
        val bondGate = CompletableDeferred<Boolean>()
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action != BluetoothDevice.ACTION_BOND_STATE_CHANGED) return
                val dev = intent.getParcelableExtra<BluetoothDevice>(BluetoothDevice.EXTRA_DEVICE)
                if (dev?.address != remote.address) return
                when (intent.getIntExtra(BluetoothDevice.EXTRA_BOND_STATE, BluetoothDevice.BOND_NONE)) {
                    BluetoothDevice.BOND_BONDED -> if (!bondGate.isCompleted) bondGate.complete(true)
                    BluetoothDevice.BOND_NONE -> if (!bondGate.isCompleted) bondGate.complete(false)
                }
            }
        }
        ContextCompat.registerReceiver(
            appContext,
            receiver,
            IntentFilter(BluetoothDevice.ACTION_BOND_STATE_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        return try {
            device.connectToDevice(remote) // kicks off createBond() on the unbonded device
            withTimeoutOrNull(timeoutMs) { bondGate.await() }
                ?: (remote.bondState == BluetoothDevice.BOND_BONDED)
        } finally {
            runCatching { appContext.unregisterReceiver(receiver) }
        }
    }

    fun disconnect() {
        controller.stop()
        device.disconnectDevice()
        _state.value = _state.value.copy(
            connectionState = DeviceConnectionState.DISCONNECTED,
            activeRouteIdentifier = null,
        )
    }

    // ── navigation ──────────────────────────────────────────────────────

    /** Begin map navigation along [route]. No-op if not connected. */
    fun startNavigation(route: NormalizedRoutePackage, rider: CoordinatePoint, speedMps: Double?) {
        if (!isConnected) return
        controller.start(route, rider, speedMps)
        _state.value = _state.value.copy(activeRouteIdentifier = route.routeIdentifier)
    }

    /** Push the latest rider fix into the active navigation frame. */
    fun onLocation(rider: CoordinatePoint, speedMps: Double?) {
        if (!isConnected) return
        controller.onLocation(rider, speedMps)
    }

    /** Clear the active route and return the device to its idle screen. */
    fun stopNavigation() {
        controller.stop()
        _state.value = _state.value.copy(activeRouteIdentifier = null)
    }

    // ── scanning ────────────────────────────────────────────────────────

    private suspend fun scanForFirstDevice(timeoutMs: Long): ScannedDevice? {
        val gate = CompletableDeferred<ScannedDevice>()
        scanner.startScan { found ->
            if (!gate.isCompleted) gate.complete(found)
        }
        val result = withTimeoutOrNull(timeoutMs) { gate.await() }
        scanner.stopScan()
        return result
    }

    private fun safeName(device: android.bluetooth.BluetoothDevice): String? =
        runCatching { device.name }.getOrNull()

    private fun <T> fail(message: String): Result<T> {
        _state.value = _state.value.copy(
            connectionState = DeviceConnectionState.DISCONNECTED,
            lastError = message,
        )
        return Result.failure(IllegalStateException(message))
    }

    private companion object {
        /** Generous window for the user to accept the system pairing dialog. */
        const val BOND_TIMEOUT_MS = 60_000L
    }
}
