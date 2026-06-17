package app.navon.bike.integration.ble.navdevice

/**
 * Abstraction over BLE device scanning.
 * Concrete implementations (BeelineScanner, future scanners) handle
 * device-specific filtering and service UUID matching internally.
 */
interface DeviceScanner {
    fun startScan(onDeviceFound: (ScannedDevice) -> Unit)
    fun stopScan()
    fun isBluetoothEnabled(): Boolean
    fun isBleSupported(): Boolean
}
