package app.navon.bike.integration.ble.navdevice.beeline

import app.navon.bike.integration.ble.navdevice.*

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.content.pm.PackageManager
import android.os.ParcelUuid
import android.util.Log
import androidx.core.app.ActivityCompat
import java.util.*

/**
 * BLE Scanner for finding Beeline devices
 */
class BeelineScanner(private val context: Context) : DeviceScanner {

    companion object {
        private const val TAG = "BeelineScanner"
        private val SERVICE_UUID = UUID.fromString(BeelineProtocol.SERVICE_UUID)
    }

    private val bluetoothManager = context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
    private val bluetoothAdapter: BluetoothAdapter? = bluetoothManager.adapter
    private val scanner = bluetoothAdapter?.bluetoothLeScanner

    private var scanning = false
    private var scanCallback: ScanCallback? = null

    /**
     * Start scanning for Beeline devices
     *
     * @param onDeviceFound Callback when a Beeline device is found
     */
    override fun startScan(onDeviceFound: (ScannedDevice) -> Unit) {
        if (!hasPermissions()) {
            Log.e(TAG, "Missing Bluetooth permissions")
            return
        }

        if (bluetoothAdapter?.isEnabled != true) {
            Log.e(TAG, "Bluetooth is not enabled")
            return
        }

        if (scanning) {
            Log.w(TAG, "Already scanning")
            return
        }

        // Create scan filter - scan for any device with "Beeline" in the name
        // or no filter at all to find devices that might not advertise the service UUID
        val filters = emptyList<ScanFilter>()  // No filter - find all devices

        // Scan settings
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .build()

        // Scan callback
        scanCallback = object : ScanCallback() {
            override fun onScanResult(callbackType: Int, result: ScanResult) {
                val deviceName = result.device.name ?: ""

                // Filter for Beeline devices by name
                if (deviceName.contains("Beeline", ignoreCase = true) ||
                    deviceName.contains("Velo", ignoreCase = true) ||
                    deviceName.contains("Moto", ignoreCase = true)) {

                    val device = ScannedDevice(
                        address = result.device.address,
                        name = deviceName.ifEmpty { "Beeline" },
                        rssi = result.rssi,
                        deviceType = "beeline"
                    )

                    Log.i(TAG, "Found Beeline device: ${device.name} (${device.address}) RSSI: ${device.rssi}")
                    onDeviceFound(device)
                }
            }

            override fun onScanFailed(errorCode: Int) {
                Log.e(TAG, "Scan failed with error: $errorCode")
                scanning = false
            }
        }

        // Start scanning
        try {
            scanner?.startScan(filters, settings, scanCallback)
            scanning = true
            Log.i(TAG, "Scanning started")
        } catch (e: SecurityException) {
            Log.e(TAG, "Permission denied", e)
        }
    }

    /**
     * Stop scanning
     */
    override fun stopScan() {
        if (!scanning) return

        try {
            scanner?.stopScan(scanCallback)
            scanning = false
            scanCallback = null
            Log.i(TAG, "Scanning stopped")
        } catch (e: SecurityException) {
            Log.e(TAG, "Permission denied", e)
        }
    }

    /**
     * Check if all required Bluetooth permissions are granted
     */
    private fun hasPermissions(): Boolean {
        return if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
            ActivityCompat.checkSelfPermission(
                context,
                Manifest.permission.BLUETOOTH_SCAN
            ) == PackageManager.PERMISSION_GRANTED &&
            ActivityCompat.checkSelfPermission(
                context,
                Manifest.permission.BLUETOOTH_CONNECT
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            ActivityCompat.checkSelfPermission(
                context,
                Manifest.permission.BLUETOOTH
            ) == PackageManager.PERMISSION_GRANTED
        }
    }

    /**
     * Check if Bluetooth is enabled
     */
    override fun isBluetoothEnabled(): Boolean {
        return bluetoothAdapter?.isEnabled == true
    }

    /**
     * Check if device supports BLE
     */
    override fun isBleSupported(): Boolean {
        return context.packageManager.hasSystemFeature(PackageManager.FEATURE_BLUETOOTH_LE)
    }
}
