package me.fiksu.esp32map.companion.integration.ble

import android.annotation.SuppressLint
import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.le.BluetoothLeScanner
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.ParcelUuid
import androidx.core.content.ContextCompat
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode
import me.fiksu.esp32map.companion.domain.RouteSyncMessage

class AndroidBleRouteSyncClient(
    private val context: Context,
) : RouteSyncBluetoothClient {
    override var onSyncMessage: ((RouteSyncMessage) -> Unit)? = null
    override var onConnectionStateChange: ((DeviceConnectionState, String?) -> Unit)? = null

    private val bluetoothManager: BluetoothManager? = context.getSystemService(BluetoothManager::class.java)
    private val bluetoothAdapter: BluetoothAdapter? get() = bluetoothManager?.adapter
    private val scanner: BluetoothLeScanner? get() = bluetoothAdapter?.bluetoothLeScanner

    private var scannedDevice: android.bluetooth.BluetoothDevice? = null
    private var bluetoothGatt: BluetoothGatt? = null
    private var chunkWriteCharacteristic: BluetoothGattCharacteristic? = null
    private var eventNotifyCharacteristic: BluetoothGattCharacteristic? = null

    private var pendingScanContinuation: kotlin.coroutines.Continuation<String>? = null
    private var pendingConnectContinuation: kotlin.coroutines.Continuation<String>? = null
    private var pendingWriteContinuation: kotlin.coroutines.Continuation<Unit>? = null
    private var armedDebugFaultMode: RouteSyncFaultInjectionMode? = null

    override val isReady: Boolean
        get() = bluetoothGatt != null && chunkWriteCharacteristic != null && eventNotifyCharacteristic != null

    override fun armDebugFault(mode: RouteSyncFaultInjectionMode) {
        armedDebugFaultMode = mode
    }

    @SuppressLint("MissingPermission")
    override suspend fun scanForRouteSyncPeripheral(timeoutMs: Long): String {
        ensurePermissions()
        ensureBluetoothEnabled()
        val scanner = scanner ?: error("Bluetooth LE scanner is unavailable")
        onConnectionStateChange?.invoke(DeviceConnectionState.SCANNING, null)
        return suspendCancellableCoroutine { continuation ->
            pendingScanContinuation = continuation
            val callback = object : ScanCallback() {
                override fun onScanResult(callbackType: Int, result: ScanResult) {
                    scannedDevice = result.device
                    scanner.stopScan(this)
                    if (pendingScanContinuation === continuation) {
                        pendingScanContinuation = null
                        continuation.resume(result.device.name ?: "ESP32 Bike Minimap")
                    }
                }

                override fun onScanFailed(errorCode: Int) {
                    scanner.stopScan(this)
                    if (pendingScanContinuation === continuation) {
                        pendingScanContinuation = null
                        continuation.resumeWithException(IllegalStateException("BLE scan failed with code $errorCode"))
                    }
                }
            }
            val filters = listOf(
                ScanFilter.Builder()
                    .setServiceUuid(ParcelUuid.fromString(BleRouteSyncGattContract.SERVICE_UUID))
                    .build(),
            )
            scanner.startScan(filters, ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).build(), callback)
            continuation.invokeOnCancellation { scanner.stopScan(callback) }
            CoroutineScope(Dispatchers.Main).launch {
                delay(timeoutMs)
                if (pendingScanContinuation === continuation) {
                    pendingScanContinuation = null
                    scanner.stopScan(callback)
                    continuation.resumeWithException(IllegalStateException("BLE scan timed out before finding the ESP32 route-sync service"))
                    onConnectionStateChange?.invoke(DeviceConnectionState.DISCONNECTED, null)
                }
            }
        }
    }

    @SuppressLint("MissingPermission")
    override suspend fun connectToScannedPeripheral(): String {
        ensurePermissions()
        ensureBluetoothEnabled()
        val device = scannedDevice ?: error("No scanned ESP32 device is available to connect")
        onConnectionStateChange?.invoke(DeviceConnectionState.CONNECTING, device.nameOrFallback("ESP32 Bike Minimap"))
        return suspendCancellableCoroutine { continuation ->
            pendingConnectContinuation = continuation
            val gatt = device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
            bluetoothGatt = gatt
        }
    }

    @SuppressLint("MissingPermission")
    override suspend fun connectToPairedPeripheral(identifier: String): String {
        ensurePermissions()
        ensureBluetoothEnabled()
        val adapter = bluetoothAdapter ?: error("Bluetooth is unavailable on this device")
        val device = runCatching { adapter.getRemoteDevice(identifier) }.getOrElse {
            throw IllegalArgumentException("Stored paired peripheral identifier is not a valid BLE address: $identifier", it)
        }
        scannedDevice = device
        onConnectionStateChange?.invoke(DeviceConnectionState.CONNECTING, device.nameOrFallback("ESP32 Bike Minimap"))
        return suspendCancellableCoroutine { continuation ->
            pendingConnectContinuation = continuation
            val gatt = device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
            bluetoothGatt = gatt
        }
    }

    /**
     * Connect to a peripheral whose BD_ADDR was just delivered through
     * the QR-OOB pairing scan. The address is fresh — no need to scan
     * the air for a service-UUID match — but the peer may not be
     * directly cacheable yet, in which case `getRemoteDevice` returns
     * a `BluetoothDevice` whose `connectGatt` attempt fails. The fast
     * path is the same as `connectToPairedPeripheral`; the difference
     * is purely semantic so the AppState wiring stays readable.
     */
    @SuppressLint("MissingPermission")
    override suspend fun connectToAdvertisedPeripheral(identifier: String): String {
        ensurePermissions()
        ensureBluetoothEnabled()
        val adapter = bluetoothAdapter ?: error("Bluetooth is unavailable on this device")
        val device = runCatching { adapter.getRemoteDevice(identifier) }.getOrElse {
            throw IllegalArgumentException(
                "Advertised peripheral identifier from QR is not a valid BLE address: $identifier",
                it,
            )
        }
        scannedDevice = device
        onConnectionStateChange?.invoke(
            DeviceConnectionState.CONNECTING,
            device.nameOrFallback("ESP32 Bike Minimap"),
        )
        return suspendCancellableCoroutine { continuation ->
            pendingConnectContinuation = continuation
            val gatt = device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
            bluetoothGatt = gatt
        }
    }

    /**
     * Write the QR's 32-byte ephemeral secret to the firmware's
     * pairing-confirm characteristic. The first encrypted write
     * triggers SMP Just Works pairing transparently; on success the
     * companion has bonded and the firmware persists `peer_identity`
     * to NVS so the next boot comes up Operational.
     */
    @SuppressLint("MissingPermission")
    override suspend fun writePairingConfirm(secret: ByteArray) {
        require(secret.size == PairingQrPayload.SECRET_LEN_BYTES) {
            "pairing-confirm secret must be ${PairingQrPayload.SECRET_LEN_BYTES} bytes"
        }
        ensurePermissions()
        val gatt = bluetoothGatt ?: error("BLE pairing GATT connection is not ready")
        val service: BluetoothGattService = gatt.getService(
            java.util.UUID.fromString(BleRouteSyncGattContract.SERVICE_UUID),
        ) ?: error("ESP32 route-sync service was not found on connected peripheral")
        val characteristic = service.getCharacteristic(
            java.util.UUID.fromString(BleRouteSyncGattContract.PAIRING_CONFIRM_CHARACTERISTIC_UUID),
        ) ?: error("ESP32 pairing-confirm characteristic was not found on connected peripheral")
        characteristic.value = secret
        characteristic.writeType = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
        suspendCancellableCoroutine { continuation ->
            pendingWriteContinuation = continuation
            if (!gatt.writeCharacteristic(characteristic)) {
                pendingWriteContinuation = null
                continuation.resumeWithException(
                    IllegalStateException("BluetoothGatt.writeCharacteristic returned false (pairing_confirm)"),
                )
            }
        }
    }

    @SuppressLint("MissingPermission")
    override suspend fun write(packet: BleRouteSyncPacket) {
        ensurePermissions()
        val gatt = bluetoothGatt ?: error("BLE route-sync GATT connection is not ready")
        val characteristic = chunkWriteCharacteristic ?: error("BLE route chunk characteristic is missing")

        if (armedDebugFaultMode == RouteSyncFaultInjectionMode.WRITE_FAILURE) {
            armedDebugFaultMode = null
            throw IllegalStateException("Injected BLE write failure before packet send")
        }

        val disconnectAfterWrite = armedDebugFaultMode == RouteSyncFaultInjectionMode.DISCONNECT_AFTER_CHUNK_WRITE
        if (disconnectAfterWrite) {
            armedDebugFaultMode = null
        }

        characteristic.value = BleRouteSyncCodec.encode(packet)
        characteristic.writeType = if ((characteristic.properties and BluetoothGattCharacteristic.PROPERTY_WRITE) != 0) {
            BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
        } else {
            BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE
        }
        if (characteristic.writeType == BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT) {
            suspendCancellableCoroutine { continuation ->
                pendingWriteContinuation = continuation
                if (!gatt.writeCharacteristic(characteristic)) {
                    pendingWriteContinuation = null
                    continuation.resumeWithException(IllegalStateException("BluetoothGatt.writeCharacteristic returned false"))
                }
            }
        } else {
            if (!gatt.writeCharacteristic(characteristic)) {
                error("BluetoothGatt.writeCharacteristic returned false")
            }
            delay(20)
        }

        if (disconnectAfterWrite) {
            gatt.disconnect()
            throw IllegalStateException("Injected BLE disconnect after chunk write")
        }
    }

    private fun ensurePermissions() {
        val missing = requiredPermissions().firstOrNull {
            ContextCompat.checkSelfPermission(context, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing != null) {
            throw SecurityException("Missing Bluetooth permission: $missing")
        }
    }

    private fun ensureBluetoothEnabled() {
        val adapter = bluetoothAdapter ?: error("Bluetooth is unavailable on this device")
        check(adapter.isEnabled) { "Bluetooth is turned off" }
    }

    private val callback = object : BluetoothGattCallback() {
        @SuppressLint("MissingPermission")
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            if (newState == android.bluetooth.BluetoothProfile.STATE_CONNECTED) {
                gatt.discoverServices()
            } else if (newState == android.bluetooth.BluetoothProfile.STATE_DISCONNECTED) {
                bluetoothGatt = null
                chunkWriteCharacteristic = null
                eventNotifyCharacteristic = null
                if (pendingConnectContinuation != null) {
                    pendingConnectContinuation?.resumeWithException(
                        IllegalStateException("Disconnected from ${gatt.device.nameOrFallback("ESP32 device")}")
                    )
                    pendingConnectContinuation = null
                }
                pendingWriteContinuation?.resumeWithException(IllegalStateException("Disconnected before BLE write completed"))
                pendingWriteContinuation = null
                onConnectionStateChange?.invoke(DeviceConnectionState.DISCONNECTED, gatt.device.nameOrFallback("ESP32 device"))
            }
        }

        @SuppressLint("MissingPermission")
        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            if (status != BluetoothGatt.GATT_SUCCESS) {
                pendingConnectContinuation?.resumeWithException(IllegalStateException("Service discovery failed with status $status"))
                pendingConnectContinuation = null
                return
            }
            val service: BluetoothGattService = gatt.getService(java.util.UUID.fromString(BleRouteSyncGattContract.SERVICE_UUID))
                ?: run {
                    pendingConnectContinuation?.resumeWithException(IllegalStateException("ESP32 route-sync service was not found"))
                    pendingConnectContinuation = null
                    return
                }
            chunkWriteCharacteristic = service.getCharacteristic(java.util.UUID.fromString(BleRouteSyncGattContract.CHUNK_WRITE_CHARACTERISTIC_UUID))
            eventNotifyCharacteristic = service.getCharacteristic(java.util.UUID.fromString(BleRouteSyncGattContract.EVENT_NOTIFY_CHARACTERISTIC_UUID))
            val notifyCharacteristic = eventNotifyCharacteristic
            if (chunkWriteCharacteristic == null || notifyCharacteristic == null) {
                pendingConnectContinuation?.resumeWithException(IllegalStateException("ESP32 route-sync characteristics were not discovered"))
                pendingConnectContinuation = null
                return
            }
            gatt.setCharacteristicNotification(notifyCharacteristic, true)
            val cccd = notifyCharacteristic.getDescriptor(java.util.UUID.fromString(CLIENT_CHARACTERISTIC_CONFIG_UUID))
            if (cccd != null) {
                @Suppress("DEPRECATION")
                cccd.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                gatt.writeDescriptor(cccd)
            } else {
                pendingConnectContinuation?.resume(gatt.device.nameOrFallback("ESP32 Bike Minimap"))
                pendingConnectContinuation = null
                onConnectionStateChange?.invoke(DeviceConnectionState.CONNECTED, gatt.device.nameOrFallback("ESP32 Bike Minimap"))
            }
        }

        @SuppressLint("MissingPermission")
        override fun onDescriptorWrite(gatt: BluetoothGatt, descriptor: BluetoothGattDescriptor, status: Int) {
            if (descriptor.characteristic.uuid == java.util.UUID.fromString(BleRouteSyncGattContract.EVENT_NOTIFY_CHARACTERISTIC_UUID)) {
                if (status == BluetoothGatt.GATT_SUCCESS) {
                    pendingConnectContinuation?.resume(gatt.device.nameOrFallback("ESP32 Bike Minimap"))
                    pendingConnectContinuation = null
                    onConnectionStateChange?.invoke(DeviceConnectionState.CONNECTED, gatt.device.nameOrFallback("ESP32 Bike Minimap"))
                } else {
                    pendingConnectContinuation?.resumeWithException(IllegalStateException("Enabling BLE notifications failed with status $status"))
                    pendingConnectContinuation = null
                }
            }
        }

        override fun onCharacteristicWrite(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, status: Int) {
            val continuation = pendingWriteContinuation ?: return
            pendingWriteContinuation = null
            if (status == BluetoothGatt.GATT_SUCCESS) {
                continuation.resume(Unit)
            } else {
                continuation.resumeWithException(IllegalStateException("Characteristic write failed with status $status"))
            }
        }

        override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, value: ByteArray) {
            if (characteristic.uuid != java.util.UUID.fromString(BleRouteSyncGattContract.EVENT_NOTIFY_CHARACTERISTIC_UUID)) {
                return
            }
            val packet = runCatching { BleRouteSyncCodec.decode(value) }.getOrNull() ?: return
            if (packet is BleRouteSyncPacket.SyncMessage) {
                if (armedDebugFaultMode == RouteSyncFaultInjectionMode.DROP_NEXT_INBOUND_STATUS && packet.message is RouteSyncMessage.Status) {
                    armedDebugFaultMode = null
                    return
                }
                onSyncMessage?.invoke(packet.message)
            }
        }

        @Suppress("DEPRECATION")
        override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic) {
            onCharacteristicChanged(gatt, characteristic, characteristic.value ?: byteArrayOf())
        }
    }

    companion object {
        private const val CLIENT_CHARACTERISTIC_CONFIG_UUID = "00002902-0000-1000-8000-00805f9b34fb"

        fun requiredPermissions(): Array<String> {
            return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                arrayOf(
                    Manifest.permission.BLUETOOTH_SCAN,
                    Manifest.permission.BLUETOOTH_CONNECT,
                )
            } else {
                arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
            }
        }
    }
}

@SuppressLint("MissingPermission")
private fun BluetoothDevice.nameOrFallback(fallback: String): String = name ?: fallback
