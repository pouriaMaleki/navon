package app.navon.bike.integration.ble.navdevice.beeline

import app.navon.bike.integration.ble.navdevice.*

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCharacteristic
import android.content.Context
import android.util.Log
import no.nordicsemi.android.ble.BleManager
import no.nordicsemi.android.ble.data.Data
import java.util.*

/**
 * Beeline Device BLE Manager
 * Handles connection and communication with Beeline Velo2/Moto2 devices
 */
class BeelineDevice(context: Context) : BleManager(context), NavDevice {

    companion object {
        private const val TAG = "BeelineDevice"

        // Service and Characteristic UUIDs
        private val SERVICE_UUID = UUID.fromString(BeelineProtocol.SERVICE_UUID)
        private val TX_CHAR_UUID = UUID.fromString(BeelineProtocol.TX_CHAR_UUID)
        private val RX_CHAR_UUID = UUID.fromString(BeelineProtocol.RX_CHAR_UUID)

        // Standard Battery Service
        private val BATTERY_SERVICE_UUID = UUID.fromString("0000180f-0000-1000-8000-00805f9b34fb")
        private val BATTERY_LEVEL_CHAR_UUID = UUID.fromString("00002a19-0000-1000-8000-00805f9b34fb")
    }

    // Characteristics
    private var txCharacteristic: BluetoothGattCharacteristic? = null
    private var rxCharacteristic: BluetoothGattCharacteristic? = null
    private var batteryLevelCharacteristic: BluetoothGattCharacteristic? = null

    // Polyline sequence counter (incrementing, included in CLEAR command from 2nd update onward)
    private var polylineSequenceNum = 0

    // Heartbeat timer (0x28): every 15s, always 0x00 (PING)
    private var heartbeatHandler: android.os.Handler? = null
    private var heartbeatRunnable: Runnable? = null
    private val HEARTBEAT_INTERVAL_MS = 15_000L

    // Geo magnetics timer (0x06): every 60s
    private var geoMagHandler: android.os.Handler? = null
    private var geoMagRunnable: Runnable? = null
    private var cachedLat = Double.NaN
    private var cachedLng = Double.NaN
    private var cachedAlt = 0f
    private val GEO_MAG_INTERVAL_MS = 60_000L

    // Auto-reconnect state
    private var reconnectHandler: android.os.Handler? = null
    private var reconnectRunnable: Runnable? = null
    private var reconnectAttempt = 0
    var lastConnectedDevice: BluetoothDevice? = null
        private set
    private var userRequestedDisconnect = false
    private val MAX_RECONNECT_ATTEMPTS = 10
    private val RECONNECT_BASE_DELAY_MS = 2_000L  // 2s, 4s, 8s, ... up to 60s

    // Callbacks
    override var onUserEvent: ((UserEvent) -> Unit)? = null
    override var onPowerStatus: ((PowerStatus) -> Unit)? = null
    override var onFirmwareVersion: ((String) -> Unit)? = null
    override var onHardwareVersion: ((String) -> Unit)? = null
    var onDeviceUid: ((String) -> Unit)? = null
    override var onConnectionStateChanged: ((Boolean) -> Unit)? = null
    override var onRawNotification: ((ByteArray) -> Unit)? = null

    // Device info cache
    override var firmwareVersion: String? = null
        private set
    override var hardwareVersion: String? = null
        private set
    var deviceUid: String? = null
        private set
    override var batteryLevel: Int = -1
        private set
    override var isCharging: Boolean = false
        private set

    override fun log(priority: Int, message: String) {
        Log.println(priority, TAG, message)
    }

    override fun getMinLogPriority(): Int = Log.VERBOSE

    override fun getGattCallback(): BleManagerGattCallback {
        return BeelineGattCallback()
    }

    private inner class BeelineGattCallback : BleManagerGattCallback() {

        override fun isRequiredServiceSupported(gatt: BluetoothGatt): Boolean {
            // Log all discovered services
            log(Log.INFO, "Discovered ${gatt.services.size} services:")
            gatt.services.forEach { service ->
                log(Log.INFO, "  Service: ${service.uuid}")
                service.characteristics.forEach { char ->
                    log(Log.INFO, "    Char: ${char.uuid} (props: ${char.properties})")
                }
            }

            // Get service
            val service = gatt.getService(SERVICE_UUID)
            if (service == null) {
                log(Log.ERROR, "Beeline service not found (looking for $SERVICE_UUID)")
                return false
            }

            // Get characteristics
            txCharacteristic = service.getCharacteristic(TX_CHAR_UUID)
            rxCharacteristic = service.getCharacteristic(RX_CHAR_UUID)

            if (txCharacteristic == null || rxCharacteristic == null) {
                log(Log.ERROR, "Required characteristics not found")
                return false
            }

            // Verify TX is writable
            val txProperties = txCharacteristic!!.properties
            if (txProperties and BluetoothGattCharacteristic.PROPERTY_WRITE == 0 &&
                txProperties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE == 0
            ) {
                log(Log.ERROR, "TX characteristic is not writable")
                return false
            }

            // Verify RX is notifiable
            val rxProperties = rxCharacteristic!!.properties
            if (rxProperties and BluetoothGattCharacteristic.PROPERTY_NOTIFY == 0) {
                log(Log.ERROR, "RX characteristic is not notifiable")
                return false
            }

            // Get battery service (optional)
            val batteryService = gatt.getService(BATTERY_SERVICE_UUID)
            if (batteryService != null) {
                batteryLevelCharacteristic = batteryService.getCharacteristic(BATTERY_LEVEL_CHAR_UUID)
                if (batteryLevelCharacteristic != null) {
                    log(Log.INFO, "Battery service found")
                }
            }

            log(Log.INFO, "Beeline service discovered successfully")
            return true
        }

        override fun initialize() {
            super.initialize()

            // Enable notifications on RX characteristic
            setNotificationCallback(rxCharacteristic).with { _, data ->
                handleNotification(data.value ?: byteArrayOf())
            }

            enableNotifications(rxCharacteristic)
                .fail { _, status -> log(Log.ERROR, "Enable RX notifications failed: $status") }
                .enqueue()

            // Enable battery notifications if available
            batteryLevelCharacteristic?.let { batteryChar ->
                setNotificationCallback(batteryChar).with { _, data ->
                    data.value?.let { bytes ->
                        if (bytes.isNotEmpty()) {
                            batteryLevel = bytes[0].toInt() and 0xFF
                            log(Log.INFO, "Battery level: $batteryLevel%")
                            onPowerStatus?.invoke(PowerStatus(batteryLevel, isCharging))
                        }
                    }
                }
                enableNotifications(batteryChar)
                    .fail { _, status -> log(Log.ERROR, "Enable battery notifications failed: $status") }
                    .enqueue()

                // Also read battery level immediately
                readCharacteristic(batteryChar)
                    .with { _, data ->
                        data.value?.let { bytes ->
                            if (bytes.isNotEmpty()) {
                                batteryLevel = bytes[0].toInt() and 0xFF
                                log(Log.INFO, "Battery level (read): $batteryLevel%")
                                onPowerStatus?.invoke(PowerStatus(batteryLevel, isCharging))
                            }
                        }
                    }
                    .fail { _, status -> log(Log.ERROR, "Read battery level failed: $status") }
                    .enqueue()
            }

            // Request device info
            requestFirmwareVersion()
            requestHardwareVersion()
            requestChargeStatus()
            requestDeviceUid()

            // Start heartbeat timer (0x28 every 15s)
            startHeartbeat()
        }

        override fun onServicesInvalidated() {
            txCharacteristic = null
            rxCharacteristic = null
            batteryLevelCharacteristic = null
        }

        override fun onDeviceDisconnected() {
            super.onDeviceDisconnected()
            stopHeartbeat()
            stopGeoMagTimer()
            onConnectionStateChanged?.invoke(false)
            log(Log.INFO, "Device disconnected")

            // Auto-reconnect if this wasn't a user-initiated disconnect
            if (!userRequestedDisconnect && lastConnectedDevice != null) {
                log(Log.INFO, "Unexpected disconnect — scheduling auto-reconnect")
                scheduleReconnect()
            }
        }
    }

    // ========================================================================
    // CONNECTION
    // ========================================================================

    fun connectToDeviceInternal(device: BluetoothDevice, isReconnectAttempt: Boolean = false): Unit {
        // implementation below

        log(Log.INFO, "Attempting to connect to ${device.name} (${device.address})")

        userRequestedDisconnect = false
        cancelReconnect()

        // Check if device is already bonded, if not create bond first
        if (device.bondState != BluetoothDevice.BOND_BONDED) {
            log(Log.INFO, "Device not bonded, attempting to create bond first...")
            lastConnectedDevice = device
            val bondStarted = device.createBond()
            if (bondStarted) {
                return
            }
            // Some stacks may reject explicit bond requests; fall back to direct connect.
            log(Log.WARN, "createBond() returned false, falling back to direct connect")
        }

        lastConnectedDevice = device

        connect(device)
            .retry(3, 100)
            .useAutoConnect(false)
            .timeout(15000)
            .before {
                log(Log.INFO, "Pre-connection setup for ${device.address}")
            }
            .done {
                log(Log.INFO, "Connected to ${device.name}")
                reconnectAttempt = 0
                requestConnectionPriority(1) // CONNECTION_PRIORITY_HIGH
                    .fail { _, status -> log(Log.WARN, "Connection priority request failed: $status") }
                    .enqueue()
                requestMtu(517)
                    .fail { _, status -> log(Log.WARN, "MTU request failed: $status") }
                    .enqueue()
                onConnectionStateChanged?.invoke(true)
            }
            .fail { _, status ->
                log(Log.ERROR, "Connection failed: $status (0x${status.toString(16).uppercase()})")
                // Continue retries for reconnect flows (including first immediate reconnect).
                if (!userRequestedDisconnect && (isReconnectAttempt || reconnectAttempt > 0)) {
                    scheduleReconnect()
                } else {
                    onConnectionStateChanged?.invoke(false)
                }
            }
            .enqueue()
    }

    /**
     * Disconnect from device. Use this for user-initiated disconnects
     * to prevent auto-reconnection.
     */
    override fun disconnectDevice() {
        userRequestedDisconnect = true
        cancelReconnect()
        lastConnectedDevice = null
        disconnect()
            .fail { _, status -> log(Log.WARN, "Disconnect failed: $status") }
            .enqueue()
    }

    /**
     * Try to reconnect to the last known device.
     * Called from Activity on resume or when Bluetooth is re-enabled.
     */
    fun reconnectLastDevice(bluetoothAdapter: android.bluetooth.BluetoothAdapter) {
        val device = lastConnectedDevice ?: return
        if (isConnected) return
        if (device.bondState != BluetoothDevice.BOND_BONDED) return

        log(Log.INFO, "Attempting to reconnect to last device: ${device.address}")
        userRequestedDisconnect = false
        reconnectAttempt = 0
        connectToDeviceInternal(device, isReconnectAttempt = true)
    }

    /**
     * Set the last connected device from a saved address (for reconnect on app restart).
     */
    fun setLastDevice(device: BluetoothDevice) {
        lastConnectedDevice = device
    }

    /** Whether auto-reconnection is currently in progress */
    val isReconnecting: Boolean
        get() = reconnectRunnable != null

    private fun scheduleReconnect() {
        if (userRequestedDisconnect) return
        val device = lastConnectedDevice ?: return

        reconnectAttempt++
        if (reconnectAttempt > MAX_RECONNECT_ATTEMPTS) {
            log(Log.WARN, "Max reconnect attempts ($MAX_RECONNECT_ATTEMPTS) reached, giving up")
            reconnectAttempt = 0
            cancelReconnect()
            onConnectionStateChanged?.invoke(false)
            return
        }

        // Exponential backoff: 2s, 4s, 8s, 16s, 32s, capped at 60s
        val delay = minOf(
            RECONNECT_BASE_DELAY_MS * (1L shl (reconnectAttempt - 1)),
            60_000L
        )
        log(Log.INFO, "Reconnect attempt $reconnectAttempt/$MAX_RECONNECT_ATTEMPTS in ${delay}ms")

        cancelReconnect()
        reconnectHandler = android.os.Handler(android.os.Looper.getMainLooper())
        reconnectRunnable = Runnable {
            if (!userRequestedDisconnect && !isConnected) {
                log(Log.INFO, "Executing reconnect attempt $reconnectAttempt")
                connect(device)
                    .retry(2, 200)
                    .useAutoConnect(true) // Let OS manage reconnect timing
                    .timeout(30000)
                    .done {
                        log(Log.INFO, "Reconnected successfully on attempt $reconnectAttempt")
                        cancelReconnect()
                        reconnectAttempt = 0
                        requestConnectionPriority(1)
                            .fail { _, status -> log(Log.WARN, "Connection priority request failed: $status") }
                            .enqueue()
                        requestMtu(517)
                            .fail { _, status -> log(Log.WARN, "MTU request failed: $status") }
                            .enqueue()
                        onConnectionStateChanged?.invoke(true)
                    }
                    .fail { _, status ->
                        log(Log.WARN, "Reconnect attempt $reconnectAttempt failed: $status")
                        if (!userRequestedDisconnect) {
                            scheduleReconnect()
                        }
                    }
                    .enqueue()
            }
        }
        reconnectRunnable?.let { reconnectHandler?.postDelayed(it, delay) }
    }

    private fun cancelReconnect() {
        reconnectRunnable?.let { reconnectHandler?.removeCallbacks(it) }
        reconnectHandler = null
        reconnectRunnable = null
    }

    // ========================================================================
    // HEARTBEAT (0x28)
    // ========================================================================

    private fun startHeartbeat() {
        stopHeartbeat()
        heartbeatHandler = android.os.Handler(android.os.Looper.getMainLooper())
        heartbeatRunnable = object : Runnable {
            override fun run() {
                if (!isConnected) return
                sendCommand(BeelineProtocol.setPhoneAppStatus())
                heartbeatHandler?.postDelayed(this, HEARTBEAT_INTERVAL_MS)
            }
        }
        // First heartbeat fires immediately
        heartbeatRunnable?.let { heartbeatHandler?.post(it) }
    }

    private fun stopHeartbeat() {
        heartbeatRunnable?.let { heartbeatHandler?.removeCallbacks(it) }
        heartbeatHandler = null
        heartbeatRunnable = null
    }

    // ========================================================================
    // GEO MAGNETICS (0x06)
    // ========================================================================

    /**
     * Send geomagnetic reference data computed from current GPS position.
     * Uses Android's GeomagneticField (WMM model) for declination, inclination, intensity.
     * Also starts/resets the 60-second periodic refresh timer.
     */
    override fun updateGeoMagnetics(lat: Double, lng: Double, altMeters: Float) {
        cachedLat = lat
        cachedLng = lng
        cachedAlt = altMeters
        sendGeoMagneticsNow()
        startGeoMagTimer()
    }

    private fun sendGeoMagneticsNow() {
        if (cachedLat.isNaN() || cachedLng.isNaN()) return
        val field = android.hardware.GeomagneticField(
            cachedLat.toFloat(), cachedLng.toFloat(), cachedAlt, System.currentTimeMillis()
        )
        sendCommand(BeelineProtocol.setGeoMagnetics(
            field.declination, field.inclination, field.fieldStrength
        ))
    }

    private fun startGeoMagTimer() {
        stopGeoMagTimer()
        geoMagHandler = android.os.Handler(android.os.Looper.getMainLooper())
        geoMagRunnable = object : Runnable {
            override fun run() {
                if (!isConnected) return
                sendGeoMagneticsNow()
                geoMagHandler?.postDelayed(this, GEO_MAG_INTERVAL_MS)
            }
        }
        geoMagRunnable?.let { geoMagHandler?.postDelayed(it, GEO_MAG_INTERVAL_MS) }
    }

    private fun stopGeoMagTimer() {
        geoMagRunnable?.let { geoMagHandler?.removeCallbacks(it) }
        geoMagHandler = null
        geoMagRunnable = null
    }

    // ========================================================================
    // SEND COMMANDS
    // ========================================================================

    /**
     * Send raw command to device
     */
    private fun sendCommand(data: ByteArray) {
        val cmdName = if (data.isNotEmpty()) commandName(data[0]) else "EMPTY"

        if (Log.isLoggable(TAG, Log.DEBUG)) {
            val caller = Thread.currentThread().stackTrace
                .firstOrNull { it.className == this::class.java.name && it.methodName != "sendCommand" }
                ?.methodName ?: "unknown"
            val hex = data.joinToString(" ") { "%02X".format(it) }
            log(Log.DEBUG, "BLE TX [$caller] $cmdName (${data.size}B): $hex")
        }

        if (!isConnected) {
            log(Log.WARN, "Cannot send command - not connected: $cmdName")
            return
        }
        txCharacteristic?.let { char ->
            writeCharacteristic(char, data)
                .fail { _, status ->
                    log(Log.ERROR, "Write failed $cmdName: $status")
                }
                .enqueue()
        } ?: log(Log.WARN, "Cannot send command - TX characteristic null: $cmdName")
    }

    /** Map command opcode to human-readable name */
    private fun commandName(opcode: Byte): String = when (opcode) {
        BeelineProtocol.CMD_GET_FIRMWARE_VERSION -> "GET_FW_VER"
        BeelineProtocol.CMD_GET_HARDWARE_VERSION -> "GET_HW_VER"
        BeelineProtocol.CMD_GET_DEVICE_UID -> "GET_UID"
        BeelineProtocol.CMD_GET_CHARGE_STATUS -> "GET_CHARGE"
        BeelineProtocol.CMD_REBOOT -> "REBOOT"
        BeelineProtocol.CMD_SET_GEO_MAGNETICS -> "SET_GEO_MAG"
        BeelineProtocol.CMD_SET_DEVICE_SETTING -> "SET_DEVICE_SET"
        BeelineProtocol.CMD_SET_DISTANCE -> "SET_DISTANCE"
        BeelineProtocol.CMD_SET_BEARING -> "SET_BEARING"
        BeelineProtocol.CMD_SET_SPEED -> "SET_SPEED"
        BeelineProtocol.CMD_SET_BACKLIGHT -> "SET_BACKLIGHT"
        BeelineProtocol.CMD_SET_PHONE_BATTERY -> "SET_PHONE_BAT"
        BeelineProtocol.CMD_SET_SCREEN -> "SET_SCREEN"
        BeelineProtocol.CMD_SET_NAVIGATION_OVERLAY -> "SET_NAV_OVERLAY"
        BeelineProtocol.CMD_FORCE_CALIBRATION -> "FORCE_CALIB"
        BeelineProtocol.CMD_SET_GPS_INFO -> "SET_GPS_INFO"
        BeelineProtocol.CMD_SET_WAYPOINT_INFO -> "SET_WAYPOINT_INFO"
        BeelineProtocol.CMD_SET_RIDE_TELEMETRY -> "SET_RIDE_TELEM"
        BeelineProtocol.CMD_SET_RIDE_STATS -> "SET_RIDE_STATS"
        BeelineProtocol.CMD_SET_RIDE_STATUS -> "SET_RIDE_STATUS"
        BeelineProtocol.CMD_SET_ANTICIPATION_BEARING -> "SET_ANTIC_BRG"
        BeelineProtocol.CMD_SET_ROUTE_PROGRESS_EXT -> "SET_ROUTE_PROG"
        BeelineProtocol.CMD_SET_ROUTE_STATUS -> "SET_ROUTE_STAT"
        BeelineProtocol.CMD_SET_JUNCTION_INDICATOR -> "SET_JUNCTION"
        BeelineProtocol.CMD_SET_NOTIFICATION -> "SET_NOTIF"
        BeelineProtocol.CMD_SET_DISTANCE_TO_DESTINATION -> "SET_DEST_DIST"
        BeelineProtocol.CMD_SET_POLYLINE -> "SET_POLYLINE"
        BeelineProtocol.CMD_SET_END_RIDE_BUTTON -> "SET_END_RIDE_BTN"
        BeelineProtocol.CMD_SET_ETA -> "SET_ETA"
        BeelineProtocol.CMD_SET_TIME_REMAINING -> "SET_TIME_REM"
        BeelineProtocol.CMD_PLAY_BEEPS -> "PLAY_BEEPS"
        BeelineProtocol.CMD_SET_MOVING_STATE -> "SET_MOVING_STATE"
        BeelineProtocol.CMD_SET_SPEED_LIMIT -> "SET_SPEED_LIMIT"
        BeelineProtocol.CMD_SET_LED -> "SET_LED"
        BeelineProtocol.CMD_SET_PHONE_APP_STATUS -> "PHONE_APP_STATUS"
        BeelineProtocol.CMD_SET_ELEVATION_DATA -> "SET_ELEV_DATA"
        BeelineProtocol.CMD_SET_CLIMB_STATE -> "SET_CLIMB_STATE"
        BeelineProtocol.CMD_SET_DESTINATIONS -> "SET_DESTS"
        BeelineProtocol.CMD_SET_SELECTED_DESTINATION -> "SET_SEL_DEST"
        BeelineProtocol.CMD_SET_ROUTE_DISTANCE -> "SET_ROUTE_DIST"
        else -> "CMD_0x%02X".format(opcode)
    }

    /** Map notification opcode to human-readable name */
    private fun notificationName(opcode: Byte): String = when (opcode) {
        BeelineProtocol.NOTIF_FIRMWARE_VERSION -> "FIRMWARE_VER"
        BeelineProtocol.NOTIF_HARDWARE_VERSION -> "HARDWARE_VER"
        BeelineProtocol.NOTIF_DEVICE_UID -> "DEVICE_UID"
        BeelineProtocol.NOTIF_USER_EVENT -> "USER_EVENT"
        BeelineProtocol.NOTIF_POWER_STATUS -> "POWER_STATUS"
        BeelineProtocol.NOTIF_MAC_ADDRESS -> "MAC_ADDR"
        BeelineProtocol.NOTIF_ORIENTATION_STATE -> "ORIENT_STATE"
        else -> "NOTIF_0x%02X".format(opcode)
    }

    /**
     * Set bearing to destination (0-360 degrees)
     */
    override fun setBearing(degrees: Float) {
        sendCommand(BeelineProtocol.setBearing(degrees))
    }

    /**
     * Set distance to destination in meters
     */
    fun setDistance(meters: Int) {
        sendCommand(BeelineProtocol.setDistance(meters))
    }

    /**
     * Set turn indicator with road name
     */
    fun setJunctionIndicator(turnType: Short, roadName: String? = null) {
        sendCommand(BeelineProtocol.setJunctionIndicator(turnType, 0, roadName))
    }

    /**
     * Clear turn indicator
     */
    fun clearJunctionIndicator() {
        sendCommand(BeelineProtocol.clearJunctionIndicator())
    }

    /**
     * Set current speed in m/s
     */
    fun setCurrentSpeed(speedMps: Float) {
        sendCommand(BeelineProtocol.setCurrentSpeed(speedMps))
    }

    /**
     * Set GPS info (heading, speed, accuracy) — 9-byte format matching official app.
     * Sent every navigation update cycle as the first command.
     *
     * @param headingDegrees GPS heading in degrees (0-360)
     * @param speedMps Current speed in m/s
     * @param accuracy GPS accuracy in meters
     */
    fun setGpsInfo(headingDegrees: Float, speedMps: Float, accuracy: Float = 0f) {
        sendCommand(BeelineProtocol.setGpsInfo(headingDegrees, speedMps, accuracy))
    }

    /**
     * Set waypoint info (current and total waypoints)
     */
    fun setWaypointInfo(currentWaypoint: Int, totalWaypoints: Int) {
        sendCommand(BeelineProtocol.setWaypointInfo(currentWaypoint, totalWaypoints))
    }

    /**
     * Send ride telemetry FULL (0x14, 17B) — on state changes (moving↔paused).
     * Fields from decompiled official app: distance, elevation, avgSpeed (cm/s), movingTime (s).
     */
    override fun sendRideTelemetry(
        distanceMeters: Int,
        elevationGainMeters: Int,
        avgSpeedCmps: Int,
        movingTimeSeconds: Int
    ) {
        sendCommand(BeelineProtocol.setRideTelemetryFull(distanceMeters, elevationGainMeters, avgSpeedCmps, movingTimeSeconds))
    }

    /**
     * Send ride telemetry SHORT (0x14, 13B) — every cycle.
     * Fields from decompiled official app: distance, elevation, avgSpeed (cm/s).
     */
    override fun sendRideTelemetryShort(
        distanceMeters: Int,
        elevationGainMeters: Int,
        avgSpeedCmps: Int
    ) {
        sendCommand(BeelineProtocol.setRideTelemetryShort(distanceMeters, elevationGainMeters, avgSpeedCmps))
    }

    /**
     * Send ride stats summary (0x15, 9B) — at ride start and end only.
     */
    override fun sendRideStats(tripDistanceMeters: Int) {
        sendCommand(BeelineProtocol.setRideStats(tripDistanceMeters))
    }

    /**
     * Set phone battery level
     */
    override fun setPhoneBattery(percent: Int) {
        val cmd = BeelineProtocol.setPhoneBattery(percent)
        log(Log.INFO, "Setting phone battery: $percent% (${cmd.joinToString(" ") { "%02X".format(it) }})")
        sendCommand(cmd)
    }

    /**
     * Set route progress (extended format with elevation)
     * @param routeProgress 0.0 to 1.0
     * @param elevationProgress 0.0 to 1.0
     */
    fun setRouteProgressExtended(routeProgress: Double, elevationProgress: Double = 0.0) {
        sendCommand(BeelineProtocol.setRouteProgressExtended(routeProgress, elevationProgress))
    }

    /**
     * Set total distance to final destination
     * @param meters Remaining distance in meters
     * @param elevationMeters Optional remaining elevation gain in meters
     */
    fun setDistanceToDestination(meters: Int, elevationMeters: Int? = null) {
        sendCommand(BeelineProtocol.setDistanceToDestination(meters, elevationMeters))
    }

    /**
     * Set estimated time of arrival (24-hour clock)
     */
    fun setETA(hours: Int, minutes: Int) {
        sendCommand(BeelineProtocol.setETA(hours, minutes))
    }

    /**
     * Set time remaining to destination
     */
    fun setTimeRemaining(hours: Int, minutes: Int) {
        sendCommand(BeelineProtocol.setTimeRemaining(hours, minutes))
    }

    /**
     * Set end-ride button visibility
     */
    fun setEndRideButton(visible: Boolean) {
        sendCommand(BeelineProtocol.setEndRideButton(visible))
    }

    /**
     * Set moving state (0=moving, 1=auto-paused, 2=manually-paused).
     * See BeelineProtocol.MOVING_STATE_MOVING / MOVING_STATE_AUTO_PAUSED / MOVING_STATE_MANUALLY_PAUSED.
     */
    fun setMovingState(state: Byte) {
        sendCommand(BeelineProtocol.setMovingState(state))
    }

    /**
     * Set speed limit for current road (0 = no limit)
     */
    fun setSpeedLimit(kmh: Int) {
        sendCommand(BeelineProtocol.setSpeedLimit(kmh))
    }

    /**
     * Set backlight brightness (0-255)
     */
    fun setBacklightBrightness(brightness: Int) {
        sendCommand(BeelineProtocol.setBacklightBrightness(brightness))
    }

    /**
     * Set climb state (climb progress on route)
     */
    fun setClimbState(climbIndex: Int, totalClimbs: Int, progress: Double) {
        sendCommand(BeelineProtocol.setClimbState(climbIndex, totalClimbs, progress))
    }

    /**
     * Send elevation profile data to device (0x29).
     * Splits into multi-packet BLE sequence with 20ms delays.
     *
     * @param elevationBytes 99-byte quantized elevation profile (0-200 per byte)
     */
    override fun sendElevationProfile(elevationBytes: ByteArray) {
        if (elevationBytes.isEmpty()) return

        val packets = BeelineProtocol.buildElevationDataPackets(elevationBytes)
        log(Log.INFO, "Sending elevation profile: ${elevationBytes.size} bytes, ${packets.size} packets")

        // Send all packets immediately — Nordic BLE library queues them
        packets.forEach { command ->
            sendCommand(command)
        }
    }

    /**
     * Stop navigation and return device to idle screen
     */
    fun stopNavigation() {
        log(Log.INFO, "Stopping navigation - sending END_RIDE and SET_SCREEN commands")

        // Send END_RIDE command
        sendCommand(BeelineProtocol.endRide())

        // Wait 100ms then set screen to idle
        android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
            sendCommand(BeelineProtocol.setScreenIdle())
        }, 100)
    }

    /**
     * Set LED color and effect
     */
    fun setLED(effect: Int, red: Int, green: Int, blue: Int,
               playbackCount: Int = 0, transition: Int = 0) {
        sendCommand(BeelineProtocol.setLED(
            effect.toByte(),
            red.toByte(),
            green.toByte(),
            blue.toByte(),
            playbackCount.toByte(),
            transition.toByte()
        ))
    }

    // Compass calibration is device-internal; no phone→device command exists.

    /**
     * Play custom beep pattern
     */
    fun playBeeps(count: Int, onDurationMs: Int, offDurationMs: Int) {
        sendCommand(BeelineProtocol.playBeeps(count, onDurationMs, offDurationMs))
    }

    /**
     * Play short beep (100ms)
     */
    fun beepShort() {
        sendCommand(BeelineProtocol.beepShort())
    }

    /**
     * Play long beep (2.5 seconds)
     */
    fun beepLong() {
        sendCommand(BeelineProtocol.beepLong())
    }

    /**
     * Play double beep alert
     */
    fun beepDouble() {
        sendCommand(BeelineProtocol.beepDouble())
    }

    /**
     * Play triple beep alert
     */
    fun beepTriple() {
        sendCommand(BeelineProtocol.beepTriple())
    }

    /**
     * Play rapid alert pattern (5 quick beeps)
     */
    fun beepAlert() {
        sendCommand(BeelineProtocol.beepAlert())
    }

    // ========================================================================
    // REQUEST INFO
    // ========================================================================

    fun requestFirmwareVersion() {
        sendCommand(BeelineProtocol.getFirmwareVersion())
    }

    fun requestHardwareVersion() {
        sendCommand(BeelineProtocol.getHardwareVersion())
    }

    fun requestChargeStatus() {
        sendCommand(BeelineProtocol.getChargeStatus())
    }

    fun requestDeviceUid() {
        sendCommand(BeelineProtocol.getDeviceUid())
    }

    /**
     * Send destination list to device (0x2B).
     * Builds and sends chunked protobuf packets.
     */
    fun sendDestinations(destinations: List<Destination>) {
        val packets = BeelineProtocol.buildDestinationPackets(destinations)
        log(Log.INFO, "Sending ${destinations.size} destinations, ${packets.size} packets")
        packets.forEach { command ->
            sendCommand(command)
        }
    }

    /**
     * Set selected destination on device (0x2C)
     */
    fun setSelectedDestination(destinationId: Int) {
        sendCommand(BeelineProtocol.setSelectedDestination(destinationId))
    }

    /**
     * Set route distance on device (0x2D)
     */
    fun setRouteDistance(meters: Int) {
        sendCommand(BeelineProtocol.setRouteDistance(meters))
    }

    /**
     * Activate arrow navigation mode (V3 firmware)
     * This is the critical command that makes navigation arrows appear on the device.
     */
    fun activateArrowNavigation() {
        // Try V2 (2-byte) command first as it's more compatible
        val command = BeelineProtocol.activateArrowNavigationV2()
        log(Log.INFO, "Activating arrow navigation (V2): ${command.joinToString(" ") { "%02X".format(it) }} (${command.size} bytes)")
        sendCommand(command)
    }

    /**
     * Activate arrow navigation mode (V3 firmware - 3 byte version)
     */
    fun activateArrowNavigationV3() {
        val command = BeelineProtocol.activateArrowNavigation()
        log(Log.INFO, "Activating arrow navigation (V3): ${command.joinToString(" ") { "%02X".format(it) }} (${command.size} bytes)")
        sendCommand(command)
    }

    /**
     * Activate map navigation mode (V3 firmware, if supported by device)
     */
    override fun activateMapNavigation() {
        sendCommand(BeelineProtocol.activateMapNavigation())
    }

    /**
     * Start a ride (required before navigation will display)
     */
    override fun startRide() {
        polylineSequenceNum = 0  // Reset sequence counter for new ride
        sendCommand(BeelineProtocol.startRide())
    }

    /**
     * Set route status to ON_ROUTE
     */
    override fun setOnRoute() {
        sendCommand(BeelineProtocol.setOnRoute())
    }

    /**
     * Send raw command (for testing)
     */
    override fun sendRawCommand(data: ByteArray) {
        sendCommand(data)
    }

    /**
     * Send the full navigation update cycle, matching the official Beeline app.
     * From Frida capture analysis, the official app sends these commands per GPS update:
     *
     *   1. SET_GPS_INFO (12 [heading_2B] 00 [speed_1B] 00 00 00 [acc_1B])
     *   2. SET_ELEV_CURRENT (2a ...)
     *   3. SET_ROUTE_STATUS (1b 00 = on route)
     *   4. SET_END_RIDE_BTN (20 01)
     *   5. SET_DEST_DIST (1e [dist_4B] [waypoints_4B])
     *   4. SET_ETA (21 HH MM)
     *   5. SET_TIME_REMAIN (22 HH MM)
     *   6. SET_SPEED_LIMIT (25 [kmh_4B])
     *   7. SET_ROUTE_PROGRESS_EXT (1a [route_2B] [elev_2B])
     *   8. SET_AVG_SPEED (13 [speed_2B])
     *   9. SET_DISTANCE (08 [dist_4B]) — distance to next turn
     *  10. SET_JUNCTION (1c [indicator_2B] [exit])
     *  11. POLYLINE (1f ...) — CLEAR + features + COMMIT
     *
     * @param distanceToDestination Total remaining distance in meters
     * @param remainingWaypoints Number of remaining turns/waypoints
     * @param etaHour ETA hour (24h format)
     * @param etaMinute ETA minute
     * @param timeRemainingHours Hours remaining
     * @param timeRemainingMinutes Minutes remaining
     * @param routeProgress Route progress 0.0-1.0
     * @param averageSpeedKmh Average speed in km/h
     * @param distanceToTurn Distance to next turn in meters
     * @param junctionIndicator Beeline junction indicator value (0 = no junction)
     * @param exitNumber Roundabout exit number (0 if N/A)
     * @param roadName Optional road name (max 10 chars)
     */
    override fun sendNavigationUpdate(
        distanceToDestination: Int,
        remainingWaypoints: Int,
        etaHour: Int,
        etaMinute: Int,
        timeRemainingHours: Int,
        timeRemainingMinutes: Int,
        routeProgress: Double,
        elevationProgress: Double,
        averageSpeedKmh: Float,
        distanceToTurn: Int,
        junctionIndicator: JunctionIndicator,
        exitNumber: Byte,
        roadName: String?,
        headingDegrees: Float,
        speedKmh: Float,
        gpsAccuracy: Int,
        elevationMeters: Int
    ) {
        // Official app order: 0x12 GPS info → route status → nav data
        setGpsInfo(headingDegrees, speedKmh / 3.6f, gpsAccuracy.toFloat())
        setOnRoute()
        setEndRideButton(false)  // HIDE during map nav
        setDistanceToDestination(distanceToDestination)
        setETA(etaHour, etaMinute)
        setTimeRemaining(timeRemainingHours, timeRemainingMinutes)
        setSpeedLimit(0)  // No speed limit data from ORS
        setRouteProgressExtended(routeProgress, elevationProgress)
        setWaypointInfo(1, remainingWaypoints)
        setDistance(distanceToTurn)
        sendCommand(BeelineProtocol.setJunctionIndicator(junctionIndicator.code.toShort(), exitNumber, roadName))
    }

    /**
     * Send the compass/arrow navigation update cycle.
     * From Frida capture of official app in compass mode, every GPS update sends:
     *   0x12 GPS_INFO → 0x1B ROUTE_STATUS → 0x20 END_RIDE_BTN(01) →
     *   0x1E DEST_DIST(5B) → 0x21 ETA(00 FF) → 0x22 TIME(00 FF) →
     *   0x25 SPEED_LIMIT → 0x1A PROGRESS → 0x13 AVG_SPEED →
     *   0x09 BEARING → 0x19 ANTICIPATION_BEARING(FF FF) → 0x08 DISTANCE → 0x1C JUNCTION
     */
    override fun sendCompassNavigationUpdate(
        headingDegrees: Float,
        speedKmh: Float,
        gpsAccuracy: Int,
        bearingToDestination: Float,
        distanceToDestination: Int,
        routeProgress: Double,
        averageSpeedKmh: Float
    ) {
        setGpsInfo(headingDegrees, speedKmh / 3.6f, gpsAccuracy.toFloat())
        setOnRoute()
        setEndRideButton(true)  // Visible in compass mode
        setDistanceToDestination(distanceToDestination)
        sendCommand(BeelineProtocol.disableETA())      // 21 00 FF = no ETA in compass mode
        sendCommand(BeelineProtocol.disableTimeRemaining())  // 22 00 FF = no time remaining
        setSpeedLimit(0)
        setRouteProgressExtended(routeProgress)
        setWaypointInfo(0, 0)
        setBearing(bearingToDestination)
        sendCommand(BeelineProtocol.disableAnticipationBearing())
        setDistance(distanceToDestination)
        sendCommand(BeelineProtocol.clearJunctionIndicator())
    }

    /**
     * Send polyline data for route visualization using V4 protobuf protocol.
     * Flow: CLEAR → feature packets → COMMIT (render).
     *
     * Matches the official Beeline app behavior from Frida capture:
     *   1. CLEAR with sequence number
     *   2. Feature 3 (ahead, 12 points) — multi-packet PartialStart + PartialComplete
     *   3. Feature 2 (rider position) — single packet, 2 points at rider
     *   4. Feature 1 (behind, 13 points) — multi-packet PartialStart + PartialComplete
     *   5. Start marker (featureId=-27)
     *   6. End marker (featureId=-28)
     *   7. COMMIT
     *
     * CRITICAL: All packets are sent immediately (no delays) so they queue
     * atomically on the BLE stack. Using postDelayed causes interleaving when
     * the next GPS update fires, which corrupts multi-packet features.
     *
     * @param points List of lat/lng coordinate pairs
     * @param currentLat Current latitude (reference point for meter conversion)
     * @param currentLng Current longitude (reference point for meter conversion)
     */
    override fun sendPolyline(
        points: List<Pair<Double, Double>>,
        currentLat: Double,
        currentLng: Double
    ) {
        if (points.isEmpty()) return

        val cosLat = kotlin.math.cos(currentLat * kotlin.math.PI / 180.0)

        // Convert all route points to absolute meters from rider's GPS position
        val rawMeters = points.map { (lat, lng) ->
            val x = ((lng - currentLng) * 111320.0 * cosLat).toInt()
            val y = ((lat - currentLat) * 111320.0).toInt()
            Pair(x, y)
        }

        // Interpolate: add points every ~25m along straight segments.
        // ORS returns sparse points (just at turns), but the device needs
        // dense points to draw lines that extend across the screen.
        val absMeters = interpolateRoute(rawMeters, 25.0)

        // Find closest route point to rider (closest to origin 0,0)
        var closestIdx = 0
        var closestDistSq = Long.MAX_VALUE
        absMeters.forEachIndexed { i, (x, y) ->
            val distSq = x.toLong() * x + y.toLong() * y
            if (distSq < closestDistSq) {
                closestDistSq = distSq
                closestIdx = i
            }
        }

        // Window by distance: ~300m ahead, ~200m behind along the route.

        // Walk forward along route until cumulative distance > 300m
        var aheadEnd = closestIdx
        var cumDist = 0.0
        for (i in closestIdx until absMeters.size - 1) {
            val dx = (absMeters[i + 1].first - absMeters[i].first).toDouble()
            val dy = (absMeters[i + 1].second - absMeters[i].second).toDouble()
            cumDist += kotlin.math.sqrt(dx * dx + dy * dy)
            aheadEnd = i + 1
            if (cumDist > 300) break
        }

        // Walk backward until cumulative distance > 200m
        var behindStart = closestIdx
        cumDist = 0.0
        for (i in closestIdx downTo 1) {
            val dx = (absMeters[i].first - absMeters[i - 1].first).toDouble()
            val dy = (absMeters[i].second - absMeters[i - 1].second).toDouble()
            cumDist += kotlin.math.sqrt(dx * dx + dy * dy)
            behindStart = i - 1
            if (cumDist > 200) break
        }

        // Feature IDs control rendering style on device:
        //   featureId=1 = thick/primary line (ahead/upcoming route)
        //   featureId=3 = thin/secondary line (behind/traveled route)
        val aheadAbs = absMeters.subList(closestIdx, minOf(aheadEnd + 1, absMeters.size))
        val behindAbs = absMeters.subList(behindStart, minOf(closestIdx + 1, absMeters.size))

        val aheadDown = downsample(aheadAbs, 12)
        // Reverse behind so it starts near rider and extends backward
        val behindDown = downsample(behindAbs, 13).reversed()

        val aheadDeltas = deltaEncode(aheadDown)
        val behindDeltas = deltaEncode(behindDown)

        // Build all BLE packets
        val allCommands = mutableListOf<ByteArray>()

        // CLEAR existing polyline
        allCommands.add(BeelineProtocol.clearPolyline(polylineSequenceNum))
        polylineSequenceNum++

        // Feature 1: ahead — thick primary line (upcoming route)
        allCommands.addAll(BeelineProtocol.setPolyline(aheadDeltas, featureId = 1))

        // Feature 3: behind — thin secondary line (traveled route)
        if (behindDeltas.size > 1) {
            allCommands.addAll(BeelineProtocol.setPolyline(behindDeltas, featureId = 3))
        }

        // Start marker
        val routeStartAbs = absMeters.first()
        allCommands.add(BeelineProtocol.buildStartEndMarker(
            true, BeelineProtocol.PolylinePoint(routeStartAbs.first, routeStartAbs.second)
        ))

        // End marker
        val routeEndAbs = absMeters.last()
        allCommands.add(BeelineProtocol.buildStartEndMarker(
            false, BeelineProtocol.PolylinePoint(routeEndAbs.first, routeEndAbs.second)
        ))

        // COMMIT - tells device to render
        allCommands.add(BeelineProtocol.commitPolyline())

        log(Log.INFO, "Polyline: ahead=${aheadDown.size} behind=${behindDown.size} pts, ${allCommands.size} pkts (seq=$polylineSequenceNum)")

        // Send all packets immediately — Nordic BLE library queues writes
        allCommands.forEach { command ->
            sendCommand(command)
        }
    }

    /**
     * Delta-encode a list of absolute meter positions.
     * Each point becomes the difference from the previous point.
     * First point is relative to (0,0) = rider's position.
     */
    private fun deltaEncode(absPoints: List<Pair<Int, Int>>): List<BeelineProtocol.PolylinePoint> {
        val deltas = mutableListOf<BeelineProtocol.PolylinePoint>()
        var prevX = 0
        var prevY = 0
        absPoints.forEach { (x, y) ->
            deltas.add(BeelineProtocol.PolylinePoint(x - prevX, y - prevY))
            prevX = x
            prevY = y
        }
        return deltas
    }

    /**
     * Interpolate points along the route so that no two consecutive points
     * are further apart than maxGap meters. This ensures ORS routes with
     * sparse points (only at turns) have enough density for the device
     * to draw visible lines across the screen.
     */
    private fun interpolateRoute(points: List<Pair<Int, Int>>, maxGap: Double): List<Pair<Int, Int>> {
        if (points.size < 2) return points
        val result = mutableListOf(points.first())
        for (i in 0 until points.size - 1) {
            val (x0, y0) = points[i]
            val (x1, y1) = points[i + 1]
            val dx = (x1 - x0).toDouble()
            val dy = (y1 - y0).toDouble()
            val dist = kotlin.math.sqrt(dx * dx + dy * dy)
            if (dist > maxGap) {
                val segments = kotlin.math.ceil(dist / maxGap).toInt()
                for (s in 1 until segments) {
                    val t = s.toDouble() / segments
                    result.add(Pair((x0 + dx * t).toInt(), (y0 + dy * t).toInt()))
                }
            }
            result.add(points[i + 1])
        }
        return result
    }

    /**
     * Downsample a list of points to at most maxPoints, preserving first, last,
     * and the sharpest turn point. This ensures the route shape at turns is visible.
     */
    private fun downsample(points: List<Pair<Int, Int>>, maxPoints: Int): List<Pair<Int, Int>> {
        if (points.size <= maxPoints) return points

        // Start with uniform sampling
        val result = mutableListOf<Pair<Int, Int>>()
        val step = (points.size - 1).toFloat() / (maxPoints - 1)
        val indices = mutableListOf<Int>()
        for (i in 0 until maxPoints) {
            indices.add((i * step).toInt())
        }

        // Find the sharpest turn point and swap it in for the nearest uniform sample
        var maxAngleChange = 0.0
        var turnIdx = -1
        for (i in 1 until points.size - 1) {
            val ax = (points[i].first - points[i - 1].first).toDouble()
            val ay = (points[i].second - points[i - 1].second).toDouble()
            val bx = (points[i + 1].first - points[i].first).toDouble()
            val by = (points[i + 1].second - points[i].second).toDouble()
            val lenA = kotlin.math.sqrt(ax * ax + ay * ay)
            val lenB = kotlin.math.sqrt(bx * bx + by * by)
            if (lenA < 1.0 || lenB < 1.0) continue
            val dot = (ax * bx + ay * by) / (lenA * lenB)
            val angle = kotlin.math.acos(dot.coerceIn(-1.0, 1.0))
            if (angle > maxAngleChange) {
                maxAngleChange = angle
                turnIdx = i
            }
        }

        // If there's a significant turn, replace the nearest uniform sample with it
        if (turnIdx > 0 && maxAngleChange > 0.3 && turnIdx !in indices) {
            var nearestSlot = 1  // don't replace first or last
            var nearestDist = Int.MAX_VALUE
            for (j in 1 until indices.size - 1) {
                val dist = kotlin.math.abs(indices[j] - turnIdx)
                if (dist < nearestDist) {
                    nearestDist = dist
                    nearestSlot = j
                }
            }
            indices[nearestSlot] = turnIdx
            indices.sort()
        }

        return indices.map { points[it] }
    }

    /**
     * Douglas-Peucker simplification for meter-space coordinates.
     * Preserves turn points (high perpendicular distance) while removing
     * points on straight sections. Keeps retrying with higher tolerance
     * until point count is within maxPoints.
     */
    private fun rdpSimplifyMeters(points: List<Pair<Int, Int>>, maxPoints: Int = 7): List<Pair<Int, Int>> {
        if (points.size <= 2) return points
        var tolerance = 3.0  // start with 3 meter tolerance
        var result = rdpCore(points, tolerance)
        // Increase tolerance to reduce points, but cap at 15m to preserve all turns
        while (result.size > maxPoints && tolerance < 15.0) {
            tolerance *= 1.5
            result = rdpCore(points, tolerance)
        }
        // If still too many points after max tolerance, uniform downsample the RDP result
        if (result.size > maxPoints) {
            result = downsample(result, maxPoints)
        }
        return result
    }

    private fun rdpCore(points: List<Pair<Int, Int>>, epsilon: Double): List<Pair<Int, Int>> {
        if (points.size <= 2) return points
        var maxDist = 0.0
        var maxIndex = 0
        val first = points.first()
        val last = points.last()
        for (i in 1 until points.size - 1) {
            val dist = perpDistInt(points[i], first, last)
            if (dist > maxDist) {
                maxDist = dist
                maxIndex = i
            }
        }
        return if (maxDist > epsilon) {
            val left = rdpCore(points.subList(0, maxIndex + 1), epsilon)
            val right = rdpCore(points.subList(maxIndex, points.size), epsilon)
            left.dropLast(1) + right
        } else {
            listOf(first, last)
        }
    }

    private fun perpDistInt(point: Pair<Int, Int>, lineStart: Pair<Int, Int>, lineEnd: Pair<Int, Int>): Double {
        val dx = (lineEnd.first - lineStart.first).toDouble()
        val dy = (lineEnd.second - lineStart.second).toDouble()
        val lenSq = dx * dx + dy * dy
        if (lenSq == 0.0) {
            val pdx = (point.first - lineStart.first).toDouble()
            val pdy = (point.second - lineStart.second).toDouble()
            return kotlin.math.sqrt(pdx * pdx + pdy * pdy)
        }
        val num = kotlin.math.abs(
            dy * (point.first - lineStart.first) - dx * (point.second - lineStart.second)
        )
        return num / kotlin.math.sqrt(lenSq)
    }

    private fun hexToBytes(hex: String): ByteArray {
        return hex.replace(" ", "").chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    }

    /**
     * Clear polyline from device
     */
    fun clearPolyline() {
        sendCommand(BeelineProtocol.clearPolyline())
    }

    // ========================================================================
    // HANDLE NOTIFICATIONS
    // ========================================================================

    private fun handleNotification(data: ByteArray) {
        if (data.isEmpty()) return

        onRawNotification?.invoke(data)

        val notifName = if (data.isNotEmpty()) notificationName(data[0]) else "EMPTY"
        val hex = data.joinToString(" ") { "%02X".format(it) }
        log(Log.INFO, "BLE RX $notifName (${data.size}B): $hex")

        when (data[0]) {
            BeelineProtocol.NOTIF_FIRMWARE_VERSION -> {
                BeelineProtocol.parseFirmwareVersion(data)?.let { version ->
                    firmwareVersion = version
                    onFirmwareVersion?.invoke(version)
                    log(Log.INFO, "Firmware: $version")
                }
            }

            BeelineProtocol.NOTIF_HARDWARE_VERSION -> {
                BeelineProtocol.parseHardwareVersion(data)?.let { version ->
                    hardwareVersion = version
                    onHardwareVersion?.invoke(version)
                    log(Log.INFO, "Hardware: $version")
                }
            }

            BeelineProtocol.NOTIF_USER_EVENT -> {
                BeelineProtocol.parseUserEvent(data)?.let { event ->
                    onUserEvent?.invoke(event)
                    log(Log.INFO, "User event: $event")
                }
            }

            BeelineProtocol.NOTIF_POWER_STATUS -> {
                BeelineProtocol.parsePowerStatus(data)?.let { status ->
                    batteryLevel = status.batteryLevel
                    isCharging = status.isCharging // Captured the missing state
                    onPowerStatus?.invoke(status)
                    log(Log.INFO, "Battery: ${status.batteryLevel}%, charging: ${status.isCharging}")
                }
            }

            BeelineProtocol.NOTIF_DEVICE_UID -> {
                BeelineProtocol.parseDeviceUid(data)?.let { uid ->
                    deviceUid = uid
                    onDeviceUid?.invoke(uid)
                    log(Log.INFO, "Device UID: $uid")
                }
            }

            else -> {
                log(Log.WARN, "Unknown notification type: 0x%02X".format(data[0]))
            }
        }
    }

    // --- Added by NavDevice implementation ---
    override fun connectToDevice(device: BluetoothDevice) {
        connectToDeviceInternal(device, false)
    }

    override fun applySettings(settings: DeviceSettings) {
        setAutoBrightness(settings.autoBrightness)
        setLanguage(settings.languageCode)
        setPhoneBattery(settings.phoneBatteryPercent)
    }

    override fun endRide() {
        stopNavigation()
    }

    override fun activateCompassNavigation() {
        activateArrowNavigation()
    }

    override fun activateFreeRide() {
        activateArrowNavigation() // fallback or mapping
    }

    override fun setMovingState(state: MovingState) {
        val byteState = when (state) {
            MovingState.STOPPED -> BeelineProtocol.MOVING_STATE_MANUALLY_PAUSED
            MovingState.ACTIVE -> BeelineProtocol.MOVING_STATE_MOVING
            MovingState.ARRIVED -> BeelineProtocol.MOVING_STATE_MANUALLY_PAUSED
        }
        sendCommand(BeelineProtocol.setMovingState(byteState))
    }

    override fun setOffRoute() {
        sendCommand(BeelineProtocol.setOffRoute())
    }

    override fun mapTurnType(orsType: Int): JunctionIndicator {
        return JunctionIndicator(TurnTypeMapper.orsToBeeline(orsType))
    }

    override fun setAutoBrightness(enabled: Boolean) {
        val value: Byte = if (enabled) 0x01 else 0x00
        sendCommand(byteArrayOf(BeelineProtocol.CMD_SET_DEVICE_SETTING, BeelineProtocol.SETTING_AUTO_BRIGHTNESS, value))
    }

    override fun setLanguage(langCode: Int) {
        sendCommand(byteArrayOf(BeelineProtocol.CMD_SET_DEVICE_SETTING, BeelineProtocol.SETTING_LANGUAGE, langCode.toByte()))
    }

    override val debugInfo: DeviceDebugInfo
        get() = DeviceDebugInfo(commands = emptyList(), presets = emptyMap(), payloadHints = emptyMap())

}
