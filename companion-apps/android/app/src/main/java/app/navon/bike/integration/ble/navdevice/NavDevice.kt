package app.navon.bike.integration.ble.navdevice

import android.bluetooth.BluetoothDevice

// ============================================================================
// SHARED TYPES (device-agnostic)
// ============================================================================

/**
 * Moving state for ride tracking.
 * Replaces BeelineProtocol.MOVING_STOPPED / MOVING_ACTIVE / MOVING_ARRIVED byte constants.
 */
enum class MovingState {
    STOPPED,
    ACTIVE,
    ARRIVED
}

/**
 * Device-agnostic junction/turn indicator.
 * Wraps an Int code — different device types have different turn type sets,
 * so an enum would couple the interface to one protocol.
 */
@JvmInline
value class JunctionIndicator(val code: Int) {
    companion object {
        val NONE = JunctionIndicator(0)
    }
}

/**
 * Scanned BLE device info (moved from BeelineScanner inner class).
 */
data class ScannedDevice(
    val address: String,
    val name: String?,
    val rssi: Int,
    val deviceType: String = "unknown"
)

/**
 * Settings snapshot passed to [NavDevice.applySettings] on connect.
 * Lets devices ignore unsupported settings without a giant parameter list.
 */
data class DeviceSettings(
    val cumulativeDistanceMeters: Int = 0,
    val currentLatitude: Double = 0.0,
    val currentLongitude: Double = 0.0,
    val currentAltitude: Float = 0f,
    val languageCode: Int = 0,
    val autoBrightness: Boolean = false,
    val phoneBatteryPercent: Int = 0
)

/**
 * Debug command tables for device-specific debug UI.
 * Each device type provides its own commands, presets, and payload hints.
 */
data class DeviceDebugInfo(
    /** Label → opcode byte (null = preset or custom) */
    val commands: List<Pair<String, Byte?>>,
    /** Label → fixed byte payload (for preset commands) */
    val presets: Map<String, ByteArray>,
    /** Opcode → hint string for the payload text field */
    val payloadHints: Map<Byte, String>
)

/**
 * Device-agnostic user interaction events.
 * Concrete NavDevice implementations parse protocol-specific notifications
 * into these shared event types.
 */
sealed class UserEvent {
    object ButtonPressShort : UserEvent()
    object ButtonPressLong : UserEvent()
    object ButtonPressDouble : UserEvent()
    object DeviceEndRide : UserEvent()
    object DeviceState : UserEvent()
    object ScreenChangeIdle : UserEvent()
    data class ScreenChange(val loop: Byte, val screen: Byte) : UserEvent()
    data class ButtonState(val screen: Byte, val code: Byte) : UserEvent()
    object CompassCalibrationComplete : UserEvent()
    object CompassCalibrationFailed : UserEvent()
    object GpsAcquired : UserEvent()
    object RideStarted : UserEvent()
    object RidePaused : UserEvent()
    object RideComplete : UserEvent()
    data class Unknown(val code: Byte) : UserEvent()
}

/**
 * Device-agnostic power/battery status.
 */
data class PowerStatus(
    val batteryLevel: Int,  // 0-100%
    val isCharging: Boolean
)

// ============================================================================
// NAV DEVICE INTERFACE
// ============================================================================

/**
 * Abstraction over a BLE navigation device.
 * Callers program against this interface; concrete implementations
 * (BeelineDevice, future devices) handle protocol details internally.
 */
interface NavDevice {

    // -- Connection --
    fun connectToDevice(device: BluetoothDevice)
    fun disconnectDevice()

    // -- Callbacks --
    var onConnectionStateChanged: ((Boolean) -> Unit)?
    var onUserEvent: ((UserEvent) -> Unit)?
    var onPowerStatus: ((PowerStatus) -> Unit)?
    var onFirmwareVersion: ((String) -> Unit)?
    var onHardwareVersion: ((String) -> Unit)?
    var onRawNotification: ((ByteArray) -> Unit)?

    // -- Device info --
    val firmwareVersion: String?
    val hardwareVersion: String?
    val batteryLevel: Int
    val isCharging: Boolean

    // -- Setup --
    fun applySettings(settings: DeviceSettings)

    // -- Ride lifecycle --
    fun startRide()
    fun endRide()
    fun activateMapNavigation()
    fun activateCompassNavigation()
    fun activateFreeRide()

    // -- Navigation per-tick --
    fun sendNavigationUpdate(
        distanceToDestination: Int,
        remainingWaypoints: Int = 0,
        etaHour: Int = 0,
        etaMinute: Int = 0,
        timeRemainingHours: Int = 0,
        timeRemainingMinutes: Int = 0,
        routeProgress: Double = 0.0,
        elevationProgress: Double = 0.0,
        averageSpeedKmh: Float = 0f,
        distanceToTurn: Int = 0,
        junctionIndicator: JunctionIndicator = JunctionIndicator.NONE,
        exitNumber: Byte = 0,
        roadName: String? = null,
        headingDegrees: Float = 0f,
        speedKmh: Float = 0f,
        gpsAccuracy: Int = 0,
        elevationMeters: Int = 0
    )

    fun sendCompassNavigationUpdate(
        headingDegrees: Float,
        speedKmh: Float,
        gpsAccuracy: Int = 0,
        bearingToDestination: Float,
        distanceToDestination: Int,
        routeProgress: Double = 0.0,
        averageSpeedKmh: Float = 0f
    )

    // -- Route visualization --
    fun sendPolyline(
        points: List<Pair<Double, Double>>,
        currentLat: Double,
        currentLng: Double
    )
    fun sendElevationProfile(elevationBytes: ByteArray)

    // -- Telemetry --
    fun sendRideTelemetry(
        distanceMeters: Int,
        elevationGainMeters: Int = 0,
        avgSpeedCmps: Int = 0,
        movingTimeSeconds: Int = 0
    )
    fun sendRideTelemetryShort(
        distanceMeters: Int,
        elevationGainMeters: Int = 0,
        avgSpeedCmps: Int = 0
    )
    fun sendRideStats(tripDistanceMeters: Int = 0)

    // -- State --
    fun setMovingState(state: MovingState)
    fun setOnRoute()
    fun setOffRoute()
    fun setPhoneBattery(percent: Int)
    fun updateGeoMagnetics(lat: Double, lng: Double, altMeters: Float = 0f)
    fun setBearing(degrees: Float)

    // -- Turn mapping --
    fun mapTurnType(orsType: Int): JunctionIndicator

    // -- Settings --
    fun setAutoBrightness(enabled: Boolean)
    fun setLanguage(langCode: Int)

    // -- Escape hatch --
    fun sendRawCommand(data: ByteArray)

    // -- Debug --
    val debugInfo: DeviceDebugInfo
}
