package app.navon.bike.integration.ble.navdevice.beeline

import app.navon.bike.integration.ble.navdevice.*

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Beeline BLE Protocol Constants and Command Builders
 * Aligned with decompiled Beeline v4.2.7134 APK (EnumC2965u.java, EnumC2970z.java)
 */

object BeelineProtocol {

    // UUIDs (Beeline Custom Service)
    const val SERVICE_UUID = "c5d70001-c45d-4f12-8693-7ef838e96446"
    const val TX_CHAR_UUID = "c5d70002-c45d-4f12-8693-7ef838e96446"  // Write to device
    const val RX_CHAR_UUID = "c5d70003-c45d-4f12-8693-7ef838e96446"  // Notifications from device

    // Device Info Commands
    const val CMD_GET_FIRMWARE_VERSION: Byte = 0x01
    const val CMD_GET_HARDWARE_VERSION: Byte = 0x02
    const val CMD_GET_DEVICE_UID: Byte = 0x03
    const val CMD_GET_CHARGE_STATUS: Byte = 0x04     // Battery/charge status query
    const val CMD_REBOOT: Byte = 0x05                // Reboot device [type]
    const val CMD_SET_GEO_MAGNETICS: Byte = 0x06     // Compass calibration reference data (13B, 4-byte Int fields)

    // Basic Navigation Commands
    const val CMD_SET_DISTANCE: Byte = 0x08          // Distance to destination
    const val CMD_SET_BEARING: Byte = 0x09           // Direction to destination
    const val CMD_SET_SPEED: Byte = 0x0A             // Current speed (cm/s as 4B int)
    const val CMD_SET_BACKLIGHT: Byte = 0x0B         // Set backlight on/off

    // Navigation overlay
    const val CMD_SET_GPS_INFO: Byte = 0x12          // GPS heading, speed, accuracy (9 bytes)

    // Advanced Navigation Commands
    const val CMD_SET_ANTICIPATION_BEARING: Byte = 0x19    // Same encoding as SET_BEARING; FF FF = disabled
    const val CMD_SET_ROUTE_PROGRESS_EXT: Byte = 0x1A     // Extended route+elevation progress (0-4095 scale)
    const val CMD_SET_JUNCTION_INDICATOR: Byte = 0x1C
    const val CMD_SET_NOTIFICATION: Byte = 0x1D            // Show notification popup on device: [type] [display_count]
    const val CMD_SET_DISTANCE_TO_DESTINATION: Byte = 0x1E  // Total distance to final destination
    const val CMD_SET_POLYLINE: Byte = 0x1F               // Send route polyline for map display
    const val CMD_SET_END_RIDE_BUTTON: Byte = 0x20
    const val CMD_SET_ETA: Byte = 0x21                    // Estimated time of arrival
    const val CMD_SET_TIME_REMAINING: Byte = 0x22         // Time remaining to destination

    // Route Management
    const val CMD_SET_RIDE_STATUS: Byte = 0x16        // Start/stop ride
    const val CMD_SET_ROUTE_STATUS: Byte = 0x1B       // Set route status (on/off route)
    const val CMD_SET_DESTINATIONS: Byte = 0x2B
    const val CMD_SET_SELECTED_DESTINATION: Byte = 0x2C
    const val CMD_SET_ROUTE_DISTANCE: Byte = 0x2D

    // Waypoint Info
    const val CMD_SET_WAYPOINT_INFO: Byte = 0x13    // [current_waypoint] [total_waypoints]
    const val CMD_SET_RIDE_TELEMETRY: Byte = 0x14    // 17B full (transitions) or 13B short (stopped cycles)
    const val CMD_SET_RIDE_STATS: Byte = 0x15        // 9B: trip distance summary (sent at ride start/end)

    // Elevation / Climb
    const val CMD_SET_ELEVATION_DATA: Byte = 0x29
    const val CMD_SET_CLIMB_STATE: Byte = 0x2A      // Climb progress [index_2B] [total_2B] [progress_2B]

    // Display
    const val CMD_SET_PHONE_BATTERY: Byte = 0x0C    // Phone battery level (0-100%)
    const val CMD_SET_SCREEN: Byte = 0x0D           // V3 firmware - activate navigation mode
    const val CMD_SET_NAVIGATION_OVERLAY: Byte = 0x0E // Navigation overlay
    const val CMD_FORCE_CALIBRATION: Byte = 0x0F    // NO-OP on 4.3.x: opcode 0x0F is a small-param setter (2/3B), not a calibration trigger. Calibration is IMU-driven internally.

    // Settings
    const val CMD_SET_DEVICE_SETTING: Byte = 0x07  // 0x07 [setting_id] [value]
    const val SETTING_BACKLIGHT_BRIGHTNESS: Byte = 0x05  // NO-OP via 0x07: apply_settings_byte_by_id has no case 5. Use opcode 0x0B directly (see `setBacklightBrightness`).
    const val SETTING_LANGUAGE: Byte = 0x0B
    const val SETTING_AUTO_BRIGHTNESS: Byte = 0x0C

    // Audio/Visual
    const val CMD_SET_LED: Byte = 0x26              // [effect, R, G, B, playbackCount, transition]
    const val CMD_PLAY_BEEPS: Byte = 0x23

    // Sensors
    // NOTE: Compass calibration is handled internally by the Velo2 device — there is no
    // phone-side force-calibration command on 4.3.x firmware. The device auto-calibrates
    // from IMU motion patterns and notifies the phone of calibration state via EVENT_LOCATION_RATING (0x02).

    // ========================================================================
    // NOTIFICATIONS FROM DEVICE
    // ========================================================================

    const val NOTIF_FIRMWARE_VERSION: Byte = 0x01
    const val NOTIF_HARDWARE_VERSION: Byte = 0x02
    const val NOTIF_DEVICE_UID: Byte = 0x03
    const val NOTIF_USER_EVENT: Byte = 0x04
    const val NOTIF_POWER_STATUS: Byte = 0x05
    const val NOTIF_MAC_ADDRESS: Byte = 0x0A
    const val NOTIF_ORIENTATION_STATE: Byte = 0x0B

    // User Event Types (from NOTIF_USER_EVENT, byte[1])
    // Corrected from decompiled EnumC2970z.java (Beeline v4.2.7134)
    const val EVENT_RESERVED: Byte = 0x00
    const val EVENT_BACKLIGHT_CHANGE: Byte = 0x01    // Backlight toggled
    const val EVENT_LOCATION_RATING: Byte = 0x02     // Gyro calibration result
    const val EVENT_SCREEN_CHANGE: Byte = 0x03       // Screen changed — bytes[2..3] = new screen mode
    const val EVENT_END_RIDE: Byte = 0x04            // End-ride button pressed on device
    const val EVENT_BUTTON_PRESS_SHORT: Byte = 0x05
    const val EVENT_BUTTON_PRESS_LONG: Byte = 0x06
    const val EVENT_RESUME_RIDE: Byte = 0x07         // Stop navigation from device
    const val EVENT_PAUSE_RIDE: Byte = 0x08          // Ride stats acknowledgement
    const val EVENT_NOTIFICATION_SCREEN_CHANGE: Byte = 0x09  // [type] [action]
    const val EVENT_WAYPOINT_SKIP: Byte = 0x0A       // NEXT=1, PREVIOUS=2
    const val EVENT_REQUEST_START_RIDE: Byte = 0x0B  // [navType] [routeId?]

    // ========================================================================
    // NAVIGATION MODES (for SET_SCREEN V3)
    // ========================================================================

    // Loop modes
    const val LOOP_NONE: Byte = 0x00
    const val LOOP_IDLE: Byte = 0x01
    const val LOOP_TRACKING: Byte = 0x03
    const val LOOP_ARROW_NAVIGATION: Byte = 0x04
    const val LOOP_MAP_NAVIGATION: Byte = 0x05

    // Screen types
    const val SCREEN_NONE: Byte = 0x00
    const val SCREEN_HOME: Byte = 0x01
    const val SCREEN_ARROW_NAVIGATION: Byte = 0x05
    const val SCREEN_MAP_NAVIGATION: Byte = 0x06

    // ========================================================================
    // JUNCTION INDICATOR VALUES (from y8.b enum, confirmed via Frida capture)
    // Format: 2-byte big-endian short in SET_JUNCTION_INDICATOR (0x1C)
    // ========================================================================

    const val JUNCTION_NONE: Short = 0

    // Roundabout (generic, without exit number arrows)
    const val ROUNDABOUT_CLOCKWISE: Short = 256       // 0x0100
    const val ROUNDABOUT_ANTI_CLOCKWISE: Short = 257  // 0x0101

    // Fork
    const val FORK_LEFT: Short = 512                  // 0x0200
    const val FORK_RIGHT: Short = 513                 // 0x0201

    // Keep
    const val KEEP_LEFT: Short = 768                  // 0x0300
    const val KEEP_RIGHT: Short = 769                 // 0x0301

    // Core turn types (0x10xx range) — confirmed from Frida walks
    const val ARRIVE_STRAIGHT: Short = 4096           // 0x1000
    const val ARRIVE_LEFT: Short = 4097               // 0x1001
    const val ARRIVE_RIGHT: Short = 4098              // 0x1002
    const val DEPART_STRAIGHT: Short = 4099           // 0x1003
    const val DEPART_LEFT: Short = 4100               // 0x1004
    const val DEPART_RIGHT: Short = 4101              // 0x1005
    const val END_OF_ROAD_LEFT: Short = 4102          // 0x1006 — confirmed walk2
    const val END_OF_ROAD_RIGHT: Short = 4103         // 0x1007
    const val FORK_STRAIGHT: Short = 4104             // 0x1008
    const val FORK_SLIGHT_LEFT: Short = 4105          // 0x1009
    const val FORK_SLIGHT_RIGHT: Short = 4106         // 0x100A
    const val TURN_LEFT: Short = 4107                 // 0x100B — confirmed walk1/walk2
    const val TURN_RIGHT: Short = 4108                // 0x100C — confirmed walk3
    const val TURN_SHARP_LEFT: Short = 4109           // 0x100D
    const val TURN_SHARP_RIGHT: Short = 4110          // 0x100E
    const val TURN_SLIGHT_LEFT: Short = 4111          // 0x100F
    const val TURN_SLIGHT_RIGHT: Short = 4112         // 0x1010
    const val STRAIGHT: Short = 4113                  // 0x1011
    const val MERGE_LEFT: Short = 4114                // 0x1012
    const val MERGE_RIGHT: Short = 4115               // 0x1013
    const val MERGE_SLIGHT_LEFT: Short = 4116         // 0x1014
    const val MERGE_SLIGHT_RIGHT: Short = 4117        // 0x1015
    const val OFF_RAMP_LEFT: Short = 4118             // 0x1016
    const val OFF_RAMP_RIGHT: Short = 4119            // 0x1017
    const val OFF_RAMP_SLIGHT_LEFT: Short = 4120      // 0x1018
    const val OFF_RAMP_SLIGHT_RIGHT: Short = 4121     // 0x1019

    // Roundabout with directional exits (LHS = left-hand-side traffic)
    const val ROUNDABOUT_LHS_LEFT: Short = 4122       // 0x101A
    const val ROUNDABOUT_LHS_SHARP_LEFT: Short = 4123 // 0x101B
    const val ROUNDABOUT_LHS_SLIGHT_LEFT: Short = 4124 // 0x101C
    const val ROUNDABOUT_LHS_RIGHT: Short = 4125      // 0x101D
    const val ROUNDABOUT_LHS_SHARP_RIGHT: Short = 4126 // 0x101E
    const val ROUNDABOUT_LHS_SLIGHT_RIGHT: Short = 4127 // 0x101F
    const val ROUNDABOUT_LHS_STRAIGHT: Short = 4128   // 0x1020

    // Roundabout (RHS = right-hand-side traffic)
    const val ROUNDABOUT_RHS_LEFT: Short = 4129       // 0x1021
    const val ROUNDABOUT_RHS_SHARP_LEFT: Short = 4130 // 0x1022
    const val ROUNDABOUT_RHS_SLIGHT_LEFT: Short = 4131 // 0x1023
    const val ROUNDABOUT_RHS_RIGHT: Short = 4132      // 0x1024
    const val ROUNDABOUT_RHS_SHARP_RIGHT: Short = 4133 // 0x1025
    const val ROUNDABOUT_RHS_SLIGHT_RIGHT: Short = 4134 // 0x1026
    const val ROUNDABOUT_RHS_STRAIGHT: Short = 4135   // 0x1027

    // U-turn and dog-leg
    const val U_TURN_LEFT: Short = 4136               // 0x1028
    const val U_TURN_RIGHT: Short = 4137              // 0x1029
    const val DOG_LEG_LEFT: Short = 4138                 // 0x102A — dog leg left (left-right zigzag)
    const val DOG_LEG_RIGHT: Short = 4139                // 0x102B — dog leg right (right-left zigzag)

    // Phone app status / keepalive
    const val CMD_SET_PHONE_APP_STATUS: Byte = 0x28  // Sent every 15s; 0x00=foreground, 0x01=background

    // State
    const val CMD_SET_MOVING_STATE: Byte = 0x24
    const val CMD_SET_SPEED_LIMIT: Byte = 0x25

    // Moving state values (for CMD_SET_MOVING_STATE)
    // Official enum: MOVING=0, AUTO_PAUSED=1, MANUALLY_PAUSED=2
    // Frida captures show: 0x00 sent when stopped, 0x01 when moving, 0x02 at arrival
    // Names below match observed navigation behavior, not internal enum names
    // Official enum (from decompiled EnumC2944j.java): MOVING=0, AUTO_PAUSED=1, MANUALLY_PAUSED=2
    const val MOVING_STATE_MOVING: Byte = 0x00          // Official: MOVING (ordinal 0)
    const val MOVING_STATE_AUTO_PAUSED: Byte = 0x01   // Official: AUTO_PAUSED (ordinal 1)
    const val MOVING_STATE_MANUALLY_PAUSED: Byte = 0x02 // Official: MANUALLY_PAUSED (ordinal 2)

    // ========================================================================
    // COMMAND BUILDERS
    // ========================================================================

    /**
     * Set bearing to destination (0-360 degrees).
     * From Device Test dump: 09 [2-byte big-endian degrees]
     *   0°=09 00 00, 45°=09 00 2d, 90°=09 00 5a, 180°=09 00 b4, 270°=09 01 0e
     */
    fun setBearing(degrees: Float): ByteArray {
        val bearing = (((degrees.toInt() % 360) + 360) % 360).toShort()
        return ByteBuffer.allocate(3)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_BEARING)
            .putShort(bearing)
            .array()
    }

    /**
     * Set distance to destination in meters
     */
    fun setDistance(meters: Int): ByteArray {
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_DISTANCE)
            .putInt(meters)
            .array()
    }

    /**
     * Set junction indicator (turn arrow + road name)
     *
     * @param turnType Turn type code (see TURN_* constants)
     * @param exitNumber Optional exit number (0 if not applicable)
     * @param roadName Optional road name (max 10 chars, UTF-8)
     */
    fun setJunctionIndicator(
        turnType: Short,
        exitNumber: Byte = 0,
        roadName: String? = null
    ): ByteArray {
        val nameBytes = roadName?.toByteArray(Charsets.UTF_8)?.let {
            if (it.size <= 10) it else it.copyOf(10)
        } ?: ByteArray(0)
        val buffer = ByteBuffer.allocate(4 + nameBytes.size)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_JUNCTION_INDICATOR)
            .putShort(turnType)
            .put(exitNumber)

        if (nameBytes.isNotEmpty()) {
            buffer.put(nameBytes)
        }

        return buffer.array()
    }

    /**
     * Clear junction indicator (send zero indicator + zero exit)
     * From Frida capture: 1c 00 00 00
     */
    fun clearJunctionIndicator(): ByteArray {
        return byteArrayOf(CMD_SET_JUNCTION_INDICATOR, 0x00, 0x00, 0x00)
    }

    /**
     * Set current speed.
     * From decompiled source: 5 bytes, speed as cm/s (m/s * 100) in 4-byte int.
     *
     * @param speedMps Current speed in m/s
     */
    fun setCurrentSpeed(speedMps: Float): ByteArray {
        val fixed = (speedMps * 100.0f).toInt()
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_SPEED)
            .putInt(fixed)
            .array()
    }

    /**
     * Set GPS info (bearing, accuracy, speed) — sent every nav update cycle.
     * From decompiled source: 9 bytes total.
     *
     * Format: 12 [bearing_2B_BE] [accuracy_2B_BE] [speed_4B_BE cm/s]
     *
     * @param headingDegrees GPS heading/bearing in degrees (0-360)
     * @param speedMps Current speed in m/s
     * @param accuracy GPS accuracy in meters
     */
    fun setGpsInfo(headingDegrees: Float, speedMps: Float, accuracy: Float = 0f): ByteArray {
        val bearing = (((headingDegrees.toInt() + 360) % 360)).toShort()
        val acc = accuracy.toInt().coerceIn(0, 65535).toShort()
        val speed = (speedMps * 100.0f).toInt()  // m/s → cm/s fixed-point
        return ByteBuffer.allocate(9)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_GPS_INFO)
            .putShort(bearing)
            .putShort(acc)
            .putInt(speed)
            .array()
    }

    /**
     * Set phone battery level (displays on device)
     * @param percent Battery percentage (0-100)
     */
    fun setPhoneBattery(percent: Int): ByteArray {
        return byteArrayOf(CMD_SET_PHONE_BATTERY, percent.coerceIn(0, 100).toByte())
    }

    /**
     * Set route progress (extended format with elevation).
     * From jadx decompilation (t8/q.java): 5 bytes total.
     * Format: 1A [routeProgress_2B_BE] [elevProgress_2B_BE]
     * Both values are on a 0-4095 scale (0.0-1.0 mapped to 0-4095).
     *
     * From Frida captures:
     *   1a 0f ff 00 00 = 100% route, 0% elevation (walk1, no elevation data)
     *   1a 01 e7 01 d8 = ~12% route, ~12% elevation (walk2 mid-route)
     *
     * @param routeProgress Route progress 0.0 to 1.0
     * @param elevationProgress Elevation profile progress 0.0 to 1.0
     */
    fun setRouteProgressExtended(routeProgress: Double, elevationProgress: Double = 0.0): ByteArray {
        val routeVal = (routeProgress.coerceIn(0.0, 1.0) * 4095).toInt()
        val elevVal = (elevationProgress.coerceIn(0.0, 1.0) * 4095).toInt()
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_ROUTE_PROGRESS_EXT)
            .putShort(routeVal.toShort())
            .putShort(elevVal.toShort())
            .array()
    }

    /**
     * Set total distance to final destination (in meters), with optional elevation.
     * From decompiled source: 5B base [0x1E dist_4B], or 9B with elevation [0x1E dist_4B elev_4B].
     *
     * @param meters Total remaining distance in meters
     * @param elevationMeters Optional remaining elevation gain in meters
     */
    fun setDistanceToDestination(meters: Int, elevationMeters: Int? = null): ByteArray {
        val size = if (elevationMeters != null) 9 else 5
        val buf = ByteBuffer.allocate(size)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_DISTANCE_TO_DESTINATION)
            .putInt(meters.coerceAtLeast(0))
        elevationMeters?.let { buf.putInt(it.coerceAtLeast(0)) }
        return buf.array()
    }

    /**
     * Set ETA (Estimated Time of Arrival)
     * From Device Test dump: 21 HH MM
     *   e.g. 21 10 11 = arrival at 16:17
     *   Disable: 21 00 FF (minutes=0xFF hides the ETA display)
     *
     * @param hours Hour of arrival (0-24, 24-hour format)
     * @param minutes Minute of arrival (0-59, or 0xFF to disable)
     */
    fun setETA(hours: Int, minutes: Int): ByteArray {
        return byteArrayOf(
            CMD_SET_ETA,
            hours.coerceIn(0, 24).toByte(),
            minutes.coerceIn(0, 0xFF).toByte()
        )
    }

    /**
     * Disable ETA display on device.
     * From Device Test dump: 21 00 FF
     */
    fun disableETA(): ByteArray {
        return byteArrayOf(CMD_SET_ETA, 0x00, 0xFF.toByte())
    }

    /**
     * Set time remaining to destination
     * From Device Test dump: 22 HH MM
     *   e.g. 22 00 01 = 0 hours 1 minute remaining
     *   Disable: 22 00 FF (minutes=0xFF hides the display)
     *
     * @param hours Hours remaining (0-222)
     * @param minutes Minutes remaining (0-59, or 0xFF to disable)
     */
    fun setTimeRemaining(hours: Int, minutes: Int): ByteArray {
        return byteArrayOf(
            CMD_SET_TIME_REMAINING,
            hours.coerceAtLeast(0).toByte(),
            minutes.coerceIn(0, 0xFF).toByte()
        )
    }

    /**
     * Disable time remaining display on device.
     * From Device Test dump: 22 00 FF
     */
    fun disableTimeRemaining(): ByteArray {
        return byteArrayOf(CMD_SET_TIME_REMAINING, 0x00, 0xFF.toByte())
    }

    /**
     * Start ride.
     * From decompiled source: [0x16, epoch_4B] — sends current Unix timestamp.
     */
    fun startRide(): ByteArray {
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_RIDE_STATUS)
            .putInt(0)
            .array()
    }

    /**
     * Set route status to ON_ROUTE
     * Tells device we're following a route
     */
    fun setOnRoute(): ByteArray {
        return byteArrayOf(CMD_SET_ROUTE_STATUS, 0x00)  // 0x00 = ON_ROUTE
    }

    /**
     * Set route status to OFF_ROUTE
     */
    fun setOffRoute(): ByteArray {
        return byteArrayOf(CMD_SET_ROUTE_STATUS, 0x01)  // 0x01 = OFF_ROUTE_REROUTE_ENABLED
    }

    /**
     * Set navigation mode (V3 firmware - activates arrow/map navigation)
     * This is the critical command that enables navigation display on the device.
     *
     * @param loopMode Loop mode (LOOP_ARROW_NAVIGATION, LOOP_MAP_NAVIGATION, etc.)
     * @param screenType Screen type (SCREEN_ARROW_NAVIGATION, SCREEN_MAP_NAVIGATION, etc.)
     */
    fun setNavigationMode(loopMode: Byte = LOOP_ARROW_NAVIGATION, screenType: Byte = SCREEN_ARROW_NAVIGATION): ByteArray {
        return byteArrayOf(CMD_SET_SCREEN, loopMode, screenType)
    }

    /**
     * Activate arrow navigation mode (V3 firmware - 3 bytes)
     */
    fun activateArrowNavigation(): ByteArray {
        return byteArrayOf(CMD_SET_SCREEN, LOOP_ARROW_NAVIGATION, SCREEN_ARROW_NAVIGATION)
    }

    /**
     * Activate MAP navigation mode (what the official app actually uses!)
     */
    fun activateMapNavigationV3(): ByteArray {
        return byteArrayOf(CMD_SET_SCREEN, 0x05, 0x06)  // MAP_NAVIGATION loop + screen
    }

    /**
     * Activate arrow navigation mode (V2 firmware - 2 bytes, simpler)
     */
    fun activateArrowNavigationV2(): ByteArray {
        return byteArrayOf(CMD_SET_SCREEN, 0x00)  // 0x00 = NAVIGATION screen
    }

    /**
     * Activate MAP navigation mode (0x0D 0x05 0x06).
     * Confirmed from HCI capture of official Beeline app: this is the screen mode
     * used for polyline route display. The official app sends this TWICE —
     * once before ride setup metadata and once after START_RIDE.
     */
    fun activateMapNavigation(): ByteArray {
        return byteArrayOf(CMD_SET_SCREEN, LOOP_MAP_NAVIGATION, SCREEN_MAP_NAVIGATION)
    }

    /**
     * Get firmware version
     */
    fun getFirmwareVersion(): ByteArray {
        return byteArrayOf(CMD_GET_FIRMWARE_VERSION)
    }

    /**
     * Get hardware version
     */
    fun getHardwareVersion(): ByteArray {
        return byteArrayOf(CMD_GET_HARDWARE_VERSION)
    }

    /**
     * Get charge status (battery level, charging state, voltage)
     */
    fun getChargeStatus(): ByteArray {
        return byteArrayOf(CMD_GET_CHARGE_STATUS)
    }

    /**
     * Get device UID
     */
    fun getDeviceUid(): ByteArray {
        return byteArrayOf(CMD_GET_DEVICE_UID)
    }

    /**
     * Set LED color and effect.
     *
     * NO-OP on Velo 2 firmware 4.3.x. Opcode 0x26 is not in the dispatcher's switch table
     * for any length — packets are silently dropped. The Velo 2 has a backlit monochrome
     * LCD, not an addressable RGB LED, so this is a SmartHalo-style holdover. Retained
     * for protocol-shape parity with captures from the official app.
     */
    fun setLED(effect: Byte, red: Byte, green: Byte, blue: Byte,
               playbackCount: Byte = 0, transition: Byte = 0): ByteArray {
        return byteArrayOf(CMD_SET_LED, effect, red, green, blue, playbackCount, transition)
    }

    // Compass calibration is device-internal — no command to send.
    // The device notifies the phone when calibration is needed/complete.

    /**
     * Play haptic vibration pattern on device.
     *
     * Firmware 4.3.x semantics (opcode 0x23, length exactly 4 bytes):
     *   byte 1 = haptic pattern ID (passed straight to `haptic_play_pattern`; not a repeat count)
     *   byte 2 = on-duration; firmware multiplies by ~25 to get tick units
     *   byte 3 = off-duration; same scaling
     *
     * The `count` parameter is misnamed — it's actually the pattern selector. Existing
     * empirical values (`beepShort = playBeeps(1, 10, 0)`, etc.) work because pattern 1
     * is a single-shot vibration; values like 2, 3, 5 select other built-in patterns. Tune
     * these visually/audibly rather than reasoning about them as "count × on_ms / off_ms".
     */
    fun playBeeps(count: Int, onDuration: Int, offDuration: Int): ByteArray {
        return byteArrayOf(
            CMD_PLAY_BEEPS,
            count.toByte(),
            onDuration.toByte(),
            offDuration.toByte()
        )
    }

    /**
     * Set waypoint info (current and total waypoint counts).
     * From decompiled source: [0x13, currentWaypoint, totalWaypoints]
     */
    fun setWaypointInfo(currentWaypoint: Int, totalWaypoints: Int): ByteArray {
        return byteArrayOf(CMD_SET_WAYPOINT_INFO, currentWaypoint.toByte(), totalWaypoints.toByte())
    }

    /**
     * Ride telemetry FULL (0x14) — 17 bytes, sent on state changes (moving↔paused).
     * Layout: 14 [distance_4B] [elevation_4B] [avgSpeed_4B] [movingTime_4B]
     *
     * Confirmed from decompiled official app (SetRideStats class in p464s9/C15766p.java):
     *   distance     = Math.round(distanceTravelledMeters)
     *   elevation    = Math.round(totalElevationGainedMeters)
     *   averageSpeed = Math.round(averageSpeedMs * 100)  (centimeters/second)
     *   movingTime   = moving time in seconds (only in FULL format, on state change)
     *
     * @param distanceMeters      Cumulative distance traveled in meters
     * @param elevationGainMeters Total elevation gained in meters
     * @param avgSpeedCmps        Average moving speed in cm/s (m/s × 100)
     * @param movingTimeSeconds   Moving time in seconds
     */
    fun setRideTelemetryFull(
        distanceMeters: Int,
        elevationGainMeters: Int = 0,
        avgSpeedCmps: Int = 0,
        movingTimeSeconds: Int = 0
    ): ByteArray {
        return ByteBuffer.allocate(17)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_RIDE_TELEMETRY)
            .putInt(distanceMeters.coerceAtLeast(0))
            .putInt(elevationGainMeters.coerceAtLeast(0))
            .putInt(avgSpeedCmps.coerceAtLeast(0))
            .putInt(movingTimeSeconds.coerceAtLeast(0))
            .array()
    }

    /**
     * Ride telemetry SHORT (0x14) — 13 bytes, sent every cycle (no state change).
     * Layout: 14 [distance_4B] [elevation_4B] [avgSpeed_4B]
     * Same first 3 fields as FULL, without movingTime.
     *
     * @param distanceMeters      Cumulative distance traveled in meters
     * @param elevationGainMeters Total elevation gained in meters
     * @param avgSpeedCmps        Average moving speed in cm/s (m/s × 100)
     */
    fun setRideTelemetryShort(
        distanceMeters: Int,
        elevationGainMeters: Int = 0,
        avgSpeedCmps: Int = 0
    ): ByteArray {
        return ByteBuffer.allocate(13)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_RIDE_TELEMETRY)
            .putInt(distanceMeters.coerceAtLeast(0))
            .putInt(elevationGainMeters.coerceAtLeast(0))
            .putInt(avgSpeedCmps.coerceAtLeast(0))
            .array()
    }

    /**
     * Ride stats summary (0x15) — 9 bytes.
     * Sent at connect, ride start, and ride end.
     * From cold-connect dump:
     *   Connect: 15 00 00 14 fd 00 00 00 00  (5373m = all-time cumulative)
     *   End:     15 00 00 17 7f 00 00 00 00  (6015m = 5373 + 642m ride)
     *
     * @param cumulativeDistanceMeters All-time cumulative distance across all rides
     */
    fun setRideStats(cumulativeDistanceMeters: Int = 0): ByteArray {
        return ByteBuffer.allocate(9)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_RIDE_STATS)
            .putInt(cumulativeDistanceMeters.coerceAtLeast(0))
            .putInt(0)  // field2 — always zero
            .array()
    }

    // ========================================================================
    // ELEVATION DATA COMMANDS (for elevation profile on device)
    // ========================================================================

    // Elevation message type (upper nibble of header byte)
    private const val MSG_TYPE_ELEVATION: Int = 3  // ElevationOverview
    private const val MSG_TYPE_DESTINATION: Int = 5  // Destination

    /**
     * Build BLE packets for elevation profile data (0x29).
     *
     * Protobuf message: field1 = count (varint), field2 = data (length-delimited bytes)
     * Split into multi-packet BLE sequence with proper transmission headers:
     *   0x31 = PartialStart, 0x32 = PartialContinue, 0x33 = PartialComplete
     *
     * From Frida captures: official app always sends 99 bytes, split across 6 packets.
     *
     * @param elevationBytes Quantized elevation data (each byte 0-200)
     * @return List of BLE packets to send sequentially
     */
    fun buildElevationDataPackets(elevationBytes: ByteArray): List<ByteArray> {
        val count = elevationBytes.size

        // Build protobuf: [field1: count] [field2: data]
        val protobuf = mutableListOf<Byte>()
        protobuf.add(0x08)  // field 1, wire type 0 (varint)
        encodeVarint(count).forEach { protobuf.add(it) }
        protobuf.add(0x12)  // field 2, wire type 2 (length-delimited)
        encodeVarint(count).forEach { protobuf.add(it) }
        elevationBytes.forEach { protobuf.add(it) }

        val protobufBytes = protobuf.toByteArray()
        val packets = mutableListOf<ByteArray>()

        // Split into BLE packets (max 18 bytes payload after cmd+header)
        val chunks = protobufBytes.toList().chunked(BLE_MAX_PAYLOAD)

        if (chunks.size == 1) {
            // Single packet
            val header = ((MSG_TYPE_ELEVATION shl 4) or TX_SINGLE).toByte()
            val packet = ByteArray(2 + chunks[0].size)
            packet[0] = CMD_SET_ELEVATION_DATA
            packet[1] = header
            chunks[0].forEachIndexed { i, b -> packet[2 + i] = b }
            packets.add(packet)
        } else {
            chunks.forEachIndexed { index, chunk ->
                val txType = when (index) {
                    0 -> TX_PARTIAL_START
                    chunks.size - 1 -> TX_PARTIAL_COMPLETE
                    else -> TX_PARTIAL_CONTINUE
                }
                val header = ((MSG_TYPE_ELEVATION shl 4) or txType).toByte()
                val packet = ByteArray(2 + chunk.size)
                packet[0] = CMD_SET_ELEVATION_DATA
                packet[1] = header
                chunk.forEachIndexed { i, b -> packet[2 + i] = b }
                packets.add(packet)
            }
        }

        return packets
    }

    // ========================================================================
    // DESTINATION MANAGEMENT COMMANDS
    // ========================================================================

    /**
     * Build BLE packets for destination list (0x2B).
     *
     * Protobuf message: repeated field1 = Destination { field1=type, field2=id, field3=name }
     * Uses same chunked transmission protocol as elevation and polyline data,
     * with messageType=5 (Destination).
     *
     * @param destinations List of destinations to send
     * @return List of BLE packets to send sequentially
     */
    fun buildDestinationPackets(destinations: List<Destination>): List<ByteArray> {
        // Build protobuf: for each destination, encode as nested message in field 1 (repeated)
        val protobuf = mutableListOf<Byte>()
        for (dest in destinations) {
            // Encode the inner Destination message
            val inner = mutableListOf<Byte>()
            // field 1: type (uint32, wire type 0) — tag = 0x08
            inner.add(0x08.toByte())
            encodeVarint(dest.type).forEach { inner.add(it) }
            // field 2: id (uint32, wire type 0) — tag = 0x10
            inner.add(0x10.toByte())
            encodeVarint(dest.id).forEach { inner.add(it) }
            // field 3: name (string, wire type 2) — tag = 0x1A
            val nameBytes = dest.name.toByteArray(Charsets.UTF_8)
            inner.add(0x1A.toByte())
            encodeVarint(nameBytes.size).forEach { inner.add(it) }
            nameBytes.forEach { inner.add(it) }

            // Wrap in outer field 1 (length-delimited, wire type 2) — tag = 0x0A
            val innerBytes = inner.toByteArray()
            protobuf.add(0x0A.toByte())
            encodeVarint(innerBytes.size).forEach { protobuf.add(it) }
            innerBytes.forEach { protobuf.add(it) }
        }

        val protobufBytes = protobuf.toByteArray()
        val packets = mutableListOf<ByteArray>()

        // Split into BLE packets (max 18 bytes payload after cmd+header)
        val chunks = protobufBytes.toList().chunked(BLE_MAX_PAYLOAD)

        if (chunks.size == 1) {
            // Single packet
            val header = ((MSG_TYPE_DESTINATION shl 4) or TX_SINGLE).toByte()
            val packet = ByteArray(2 + chunks[0].size)
            packet[0] = CMD_SET_DESTINATIONS
            packet[1] = header
            chunks[0].forEachIndexed { i, b -> packet[2 + i] = b }
            packets.add(packet)
        } else {
            chunks.forEachIndexed { index, chunk ->
                val txType = when (index) {
                    0 -> TX_PARTIAL_START
                    chunks.size - 1 -> TX_PARTIAL_COMPLETE
                    else -> TX_PARTIAL_CONTINUE
                }
                val header = ((MSG_TYPE_DESTINATION shl 4) or txType).toByte()
                val packet = ByteArray(2 + chunk.size)
                packet[0] = CMD_SET_DESTINATIONS
                packet[1] = header
                chunk.forEachIndexed { i, b -> packet[2 + i] = b }
                packets.add(packet)
            }
        }

        return packets
    }

    /**
     * Set selected destination (0x2C).
     * Format: [0x2C] [id_4B_BE] — 5 bytes, simple int.
     *
     * NO-OP on Velo 2 firmware 4.3.x. Opcode 0x2C is not in the dispatcher's switch
     * table; the packet is silently dropped. May have been a 4.2.x command that was
     * removed, or one the decompiled app emits but no shipping firmware ever consumed.
     *
     * @param destinationId The destination ID to select
     */
    fun setSelectedDestination(destinationId: Int): ByteArray {
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_SELECTED_DESTINATION)
            .putInt(destinationId)
            .array()
    }

    /**
     * Set route distance (0x2D).
     * Format: [0x2D] [distance_4B_BE] — 5 bytes, simple int.
     *
     * NO-OP on Velo 2 firmware 4.3.x. Opcode 0x2D is not in the dispatcher's switch
     * table; the packet is silently dropped. Use `setDistanceToDestination` (opcode 0x1E)
     * to update the displayed total-distance value.
     *
     * @param meters Route distance in meters
     */
    fun setRouteDistance(meters: Int): ByteArray {
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_ROUTE_DISTANCE)
            .putInt(meters.coerceAtLeast(0))
            .array()
    }

    /**
     * Phone app status / keepalive command.
     * Sent every 15 seconds. 0x00 = foreground, 0x01 = background.
     */
    fun setPhoneAppStatus(foreground: Boolean = true): ByteArray {
        return byteArrayOf(CMD_SET_PHONE_APP_STATUS, if (foreground) 0x00 else 0x01)
    }

    // Predefined beep patterns
    fun beepShort(): ByteArray = playBeeps(1, 10, 0)           // Single 100ms beep
    fun beepLong(): ByteArray = playBeeps(1, 255, 0)           // Long 2.5s beep
    fun beepDouble(): ByteArray = playBeeps(2, 10, 5)          // Two quick beeps
    fun beepTriple(): ByteArray = playBeeps(3, 10, 5)          // Three quick beeps
    fun beepAlert(): ByteArray = playBeeps(5, 5, 3)            // Rapid alert pattern

    /**
     * Set end-ride button visibility on device screen
     * From Frida capture: 20 00 (hidden) or 20 01 (visible)
     */
    fun setEndRideButton(visible: Boolean): ByteArray {
        return byteArrayOf(CMD_SET_END_RIDE_BUTTON, if (visible) 0x01 else 0x00)
    }

    /**
     * Set moving state.
     * From Frida capture: 24 00=stopped, 24 01=moving, 24 02=arrived
     * State 2 (arrived) is sent when ride ends at destination.
     *
     * Note: Official app enum names are MOVING=0, AUTO_PAUSED=1, MANUALLY_PAUSED=2
     * but Frida captures during navigation show 0x00 sent when stopped and 0x01 when moving.
     */
    fun setMovingState(state: Byte): ByteArray {
        return byteArrayOf(CMD_SET_MOVING_STATE, state)
    }

    /**
     * Set geomagnetic reference data for compass calibration.
     * From decompiled source: 7 bytes total, sent at connection + every 60s.
     * Format: 06 [declination_2B_BE millideg] [inclination_2B_BE millideg] [intensity_2B_BE nT]
     *
     * @param declinationDeg Magnetic declination in degrees (from GeomagneticField)
     * @param inclinationDeg Magnetic inclination in degrees (from GeomagneticField)
     * @param intensityNt Total field intensity in nanoTeslas (from GeomagneticField)
     */
    fun setGeoMagnetics(declinationDeg: Float, inclinationDeg: Float, intensityNt: Float): ByteArray {
        val declMillideg = (declinationDeg * 1000).toInt()
        val inclMillideg = (inclinationDeg * 1000).toInt()
        val intensity = intensityNt.toInt()
        return ByteBuffer.allocate(13)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_GEO_MAGNETICS)
            .putInt(declMillideg)
            .putInt(inclMillideg)
            .putInt(intensity)
            .array()
    }

    /**
     * Set backlight brightness (0-255).
     *
     * Uses opcode 0x0B directly — NOT `SET_DEVICE_SETTING(0x07, 0x05, …)`. On Velo 2
     * firmware 4.3.x, `apply_settings_byte_by_id` has no case for sub-id 5, so the
     * setting-id route is silently dropped. Opcode 0x0B's 3-byte form `[0B 01 brightness]`
     * is the correct path: `[0B 00]` turns the display off, `[0B 01 NN]` turns it on at
     * brightness NN.
     */
    fun setBacklightBrightness(brightness: Int): ByteArray {
        val b = brightness.coerceIn(0, 255)
        return if (b == 0) {
            byteArrayOf(CMD_SET_BACKLIGHT, 0x00)
        } else {
            byteArrayOf(CMD_SET_BACKLIGHT, 0x01, b.toByte())
        }
    }

    /**
     * Set speed limit for current road segment (in km/h).
     * From Device Test dump: same encoding as speed in SET_GPS_INFO (cm/s).
     *   25 km/h = 25 00 00 02 b6 (694 cm/s)
     *   0 km/h  = 25 00 00 00 00 (no limit)
     */
    fun setSpeedLimit(kmh: Int): ByteArray {
        val speedCmPerSec = (kmh / 3.6 * 100).toInt()
        return ByteBuffer.allocate(5)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_SPEED_LIMIT)
            .putInt(speedCmPerSec.coerceAtLeast(0))
            .array()
    }

    /**
     * Set anticipation bearing (0x19) — same encoding as SET_BEARING (2-byte big-endian degrees).
     * From Device Test dump: identical to bearing section with same degree→hex mapping.
     * In compass mode: sent as 19 FF FF every cycle (FF FF = disabled / no anticipation).
     * In map mode: not sent.
     */
    fun setAnticipationBearing(degrees: Float): ByteArray {
        val bearing = (((degrees.toInt() % 360) + 360) % 360).toShort()
        return ByteBuffer.allocate(3)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_ANTICIPATION_BEARING)
            .putShort(bearing)
            .array()
    }

    /**
     * Disable anticipation bearing (compass mode default).
     * Sends 19 FF FF.
     */
    fun disableAnticipationBearing(): ByteArray {
        return byteArrayOf(CMD_SET_ANTICIPATION_BEARING, 0xFF.toByte(), 0xFF.toByte())
    }

    /**
     * Show notification popup on device (0x1D).
     * Format: 1d [type] [display_count]
     *
     * Types:
     *   0x00=NONE, 0x01=LOW_BATTERY, 0x02=CHARGING, 0x03=SAD_FACE,
     *   0x04=HAPPY_FACE, 0x05=RESUME_RIDE, 0x06=STOP_RIDE,
     *   0x07=PAUSE_RIDE, 0x08=CONNECTION_LOST, 0x09=RECONNECTED,
     *   0x0A=ONBOARDING_END_RIDE, 0x0B=ONBOARDING_STOP_RIDE,
     *   0x0C=ONBOARDING_ROAD_RATE_POS, 0x0D=ONBOARDING_ROAD_RATE_NEG,
     *   0x0E=OOBE_GREETING, 0x0F=MABEL, 0x10=LOCATION_PERMISSION_REQ,
     *   0x11=LOCATION_SERVICES_REQ, 0x12=CLIMB_COMPLETE
     *
     * Device responds: NOTIF_USER_EVENT 04 09 [type] [status]
     *   status: 0x00=DISPLAYED, 0x02=DISMISSED_TIMEOUT, 0x03=DISMISSED_NO_LONGER_RELEVANT
     *
     * @param type Notification type (see constants above)
     * @param displayCount Display parameter (0x0A typical)
     */
    fun showNotification(type: Byte, displayCount: Byte = 0x0A): ByteArray {
        return byteArrayOf(CMD_SET_NOTIFICATION, type, displayCount)
    }

    // Notification type constants
    const val NOTIF_TYPE_NONE: Byte = 0x00
    const val NOTIF_TYPE_LOW_BATTERY: Byte = 0x01
    const val NOTIF_TYPE_CHARGING: Byte = 0x02
    const val NOTIF_TYPE_SAD_FACE: Byte = 0x03
    const val NOTIF_TYPE_HAPPY_FACE: Byte = 0x04
    const val NOTIF_TYPE_RESUME_RIDE: Byte = 0x05
    const val NOTIF_TYPE_STOP_RIDE: Byte = 0x06
    const val NOTIF_TYPE_PAUSE_RIDE: Byte = 0x07
    const val NOTIF_TYPE_CONNECTION_LOST: Byte = 0x08
    const val NOTIF_TYPE_RECONNECTED: Byte = 0x09
    const val NOTIF_TYPE_ONBOARDING_END_RIDE: Byte = 0x0A
    const val NOTIF_TYPE_ONBOARDING_STOP_RIDE: Byte = 0x0B
    const val NOTIF_TYPE_ONBOARDING_ROAD_RATE_POS: Byte = 0x0C
    const val NOTIF_TYPE_ONBOARDING_ROAD_RATE_NEG: Byte = 0x0D
    const val NOTIF_TYPE_OOBE_GREETING: Byte = 0x0E
    const val NOTIF_TYPE_MABEL: Byte = 0x0F
    const val NOTIF_TYPE_LOCATION_PERMISSION_REQ: Byte = 0x10
    const val NOTIF_TYPE_LOCATION_SERVICES_REQ: Byte = 0x11
    const val NOTIF_TYPE_CLIMB_COMPLETE: Byte = 0x12

    /**
     * Set climb state (climb progress on route).
     * From decompiled source: [0x2A, climbIndex_2B, totalClimbs_2B, progress_2B]
     * Progress is 0-4095 scale (0.0-1.0 mapped to 0-4095).
     */
    fun setClimbState(climbIndex: Int, totalClimbs: Int, progress: Double): ByteArray {
        val prog = (progress.coerceIn(0.0, 1.0) * 4095).toInt()
        return ByteBuffer.allocate(7)
            .order(ByteOrder.BIG_ENDIAN)
            .put(CMD_SET_CLIMB_STATE)
            .putShort(climbIndex.toShort())
            .putShort(totalClimbs.toShort())
            .putShort(prog.toShort())
            .array()
    }

    // ========================================================================
    // NOTIFICATION PARSERS
    // ========================================================================

    /**
     * Parse firmware version notification
     * Can be either binary (4 bytes: major.minor.patch.build) or ASCII text
     */
    fun parseFirmwareVersion(data: ByteArray): String? {
        if (data.isEmpty() || data[0] != NOTIF_FIRMWARE_VERSION) return null

        // Try binary format first (4 bytes: major.minor.patch.build)
        if (data.size == 5) {
            val major = data[1].toInt() and 0xFF
            val minor = data[2].toInt() and 0xFF
            val patch = data[3].toInt() and 0xFF
            val build = data[4].toInt() and 0xFF
            return "$major.$minor.$patch.$build"
        }

        // Fall back to UTF-8 text format
        return String(data, 1, data.size - 1, Charsets.UTF_8)
    }

    /**
     * Parse hardware version notification
     * Can be either binary (3-4 bytes) or ASCII text
     */
    fun parseHardwareVersion(data: ByteArray): String? {
        if (data.isEmpty() || data[0] != NOTIF_HARDWARE_VERSION) return null

        // Try binary format (bytes as version components)
        if (data.size >= 4 && data.size <= 5) {
            val parts = mutableListOf<Int>()
            for (i in 1 until data.size) {
                val value = data[i].toInt() and 0xFF
                if (value > 0) parts.add(value)
            }
            if (parts.isNotEmpty()) {
                return parts.joinToString(".")
            }
        }

        // Fall back to UTF-8 text format
        return String(data, 1, data.size - 1, Charsets.UTF_8)
    }

    /**
     * Parse user event notification
     */
    fun parseUserEvent(data: ByteArray): UserEvent? {
        if (data.size < 2 || data[0] != NOTIF_USER_EVENT) return null
        return when (data[1]) {
            EVENT_BACKLIGHT_CHANGE -> UserEvent.ScreenChangeIdle
            EVENT_LOCATION_RATING -> if (data.size >= 3 && data[2] != 0.toByte()) UserEvent.CompassCalibrationComplete else UserEvent.CompassCalibrationFailed
            EVENT_SCREEN_CHANGE -> {
                if (data.size >= 4 && data[2] == LOOP_IDLE && data[3] == SCREEN_HOME) {
                    UserEvent.ScreenChangeIdle
                } else {
                    UserEvent.ScreenChange(
                        if (data.size >= 3) data[2] else 0,
                        if (data.size >= 4) data[3] else 0
                    )
                }
            }
            EVENT_END_RIDE -> UserEvent.DeviceEndRide
            EVENT_BUTTON_PRESS_SHORT -> UserEvent.ButtonPressShort
            EVENT_BUTTON_PRESS_LONG -> UserEvent.ButtonPressLong
            EVENT_RESUME_RIDE -> UserEvent.RideStarted
            EVENT_PAUSE_RIDE -> UserEvent.RidePaused
            else -> UserEvent.Unknown(data[1])
        }
    }

    /**
     * Parse power/charge status notification.
     * Format: [0x05, percentage, chargingState, voltage_hi, voltage_lo]
     */
    fun parsePowerStatus(data: ByteArray): PowerStatus? {
        if (data.size < 3 || data[0] != NOTIF_POWER_STATUS) return null
        val voltage = if (data.size >= 5) {
            ((data[3].toInt() and 0xFF) shl 8) or (data[4].toInt() and 0xFF)
        } else 0
        return PowerStatus(
            batteryLevel = data[1].toInt() and 0xFF,
            isCharging = ChargeState.fromByte(data[2].toInt() and 0xFF) == ChargeState.CHARGING
        )
    }

    /**
     * Parse device UID notification.
     * Format: [0x03] [b1] [b2] [b3] [b4] → "7B:C2:07:D4" (uppercase hex, colon-separated)
     * Based on decompiled C2948n.m15946d() which converts bytes[1..4] to uppercase hex pairs.
     *
     * @return Parsed UID string, or null if data is invalid
     */
    fun parseDeviceUid(data: ByteArray): String? {
        if (data.size != 5 || data[0] != NOTIF_DEVICE_UID) return null
        return (1..4).joinToString(":") { "%02X".format(data[it]) }
    }

    // ========================================================================
    // POLYLINE COMMANDS (for route visualization on device)
    // ========================================================================

    data class PolylinePoint(val x: Int, val y: Int)

    /**
     * Convert lat/lng coordinates to Beeline polyline format.
     * Uses flat-earth approximation to convert GPS → meters relative to rider,
     * then delta-encodes as signed bytes (matching iOS binary behavior).
     *
     * @param points List of (latitude, longitude) pairs
     * @param referenceLat Reference latitude (typically current location)
     * @param referenceLng Reference longitude (typically current location)
     * @return List of delta-encoded points (each x/y is a signed byte delta in meters)
     */
    fun encodePolyline(
        points: List<Pair<Double, Double>>,
        referenceLat: Double,
        referenceLng: Double
    ): List<PolylinePoint> {
        val cosLat = kotlin.math.cos(referenceLat * kotlin.math.PI / 180.0)

        // Convert all points to absolute positions in meters from reference.
        // HCI capture confirms the official app uses meters (values up to ±149).
        // Coordinates are zigzag-varint encoded in the protobuf, so no byte-range limit.
        val absolutePoints = points.map { (lat, lng) ->
            val xMeters = (lng - referenceLng) * 111320.0 * cosLat
            val yMeters = (lat - referenceLat) * 111320.0
            Pair(xMeters.toInt(), yMeters.toInt())
        }

        // Delta-encode: each point relative to previous
        val deltaEncoded = mutableListOf<PolylinePoint>()
        var prevX = 0
        var prevY = 0

        absolutePoints.forEach { (x, y) ->
            val dx = x - prevX
            val dy = y - prevY
            deltaEncoded.add(PolylinePoint(dx, dy))
            prevX = x
            prevY = y
        }

        return deltaEncoded
    }

    // BLE packet payload limit: 19 bytes total (1-byte header + 18 bytes data)
    private const val BLE_MAX_PAYLOAD = 18

    // Transmission types (lower nibble of header byte)
    private const val TX_SINGLE: Int = 0x00
    private const val TX_PARTIAL_START: Int = 0x01
    private const val TX_PARTIAL_CONTINUE: Int = 0x02
    private const val TX_PARTIAL_COMPLETE: Int = 0x03

    /**
     * Encode an unsigned varint (protobuf-style variable-length integer).
     */
    private fun encodeVarint(value: Int): ByteArray {
        val result = mutableListOf<Byte>()
        var v = value
        while (v > 0x7F) {
            result.add(((v and 0x7F) or 0x80).toByte())
            v = v ushr 7
        }
        result.add((v and 0x7F).toByte())
        return result.toByteArray()
    }

    /**
     * Encode a signed integer using zigzag encoding, then as a varint.
     * Zigzag maps signed → unsigned: 0→0, -1→1, 1→2, -2→3, 2→4, ...
     * Formula: (n << 1) ^ (n >> 31)
     */
    private fun encodeZigzagVarint(value: Int): ByteArray {
        val zigzag = (value shl 1) xor (value shr 31)
        return encodeVarint(zigzag)
    }

    /**
     * Build the protobuf envelope for a polyline feature.
     *
     * Format: [field 1 = featureId as varint] [field 2 = packed zigzag-varint coordinates]
     *   - Field 1: 0x08 + varint(featureId)
     *   - Field 2: 0x12 + varint(length) + packed zigzag-varint coordinate pairs
     *
     * HCI capture confirms coordinates are zigzag-varint encoded (not raw bytes).
     * This allows values beyond ±127 (e.g., -149 in official app captures).
     *
     * @param points Delta-encoded coordinate points
     * @param featureId Feature identifier (e.g., 3 for ahead segment)
     * @return Complete protobuf message bytes
     */
    private fun buildProtobufEnvelope(points: List<PolylinePoint>, featureId: Int): ByteArray {
        // Build coordinate data as packed zigzag varints (x, y, x, y, ...)
        val coordBytes = mutableListOf<Byte>()
        points.forEach { point ->
            encodeZigzagVarint(point.x).forEach { coordBytes.add(it) }
            encodeZigzagVarint(point.y).forEach { coordBytes.add(it) }
        }
        val coordData = coordBytes.toByteArray()

        val result = mutableListOf<Byte>()

        // Field 1: varint featureId
        result.add(0x08)  // field 1, wire type 0 (varint)
        encodeVarint(featureId).forEach { result.add(it) }

        // Field 2: length-delimited coordinate data
        result.add(0x12)  // field 2, wire type 2 (length-delimited)
        encodeVarint(coordData.size).forEach { result.add(it) }
        coordData.forEach { result.add(it) }

        return result.toByteArray()
    }

    /**
     * Generate polyline BLE packets for route display.
     * Builds a V4 protobuf envelope, then splits it into BLE-sized packets
     * with proper framing headers.
     *
     * Header byte format: (messageType << 4) | transmissionType
     *   messageType=1 for feature data
     *   transmissionType: 0=Single, 1=PartialStart, 2=PartialContinue, 3=PartialComplete
     *
     * @param points Delta-encoded coordinate points
     * @param featureId Feature identifier (1=ahead thick, 3=behind thin)
     * @return List of BLE packets to send sequentially
     */
    fun setPolyline(
        points: List<PolylinePoint>,
        featureId: Int = 1
    ): List<ByteArray> {
        if (points.isEmpty()) {
            return emptyList()
        }

        val protobuf = buildProtobufEnvelope(points, featureId)
        val packets = mutableListOf<ByteArray>()

        if (protobuf.size <= BLE_MAX_PAYLOAD) {
            val header = ((1 shl 4) or TX_SINGLE).toByte()  // 0x10
            val packet = ByteArray(1 + 1 + protobuf.size)
            packet[0] = CMD_SET_POLYLINE
            packet[1] = header
            protobuf.copyInto(packet, 2)
            packets.add(packet)
        } else {
            val chunks = protobuf.toList().chunked(BLE_MAX_PAYLOAD)
            chunks.forEachIndexed { index, chunk ->
                val txType = when {
                    chunks.size == 1 -> TX_SINGLE
                    index == 0 -> TX_PARTIAL_START
                    index == chunks.size - 1 -> TX_PARTIAL_COMPLETE
                    else -> TX_PARTIAL_CONTINUE
                }
                val header = ((1 shl 4) or txType).toByte()
                val packet = ByteArray(1 + 1 + chunk.size)
                packet[0] = CMD_SET_POLYLINE
                packet[1] = header
                chunk.forEachIndexed { i, b -> packet[2 + i] = b }
                packets.add(packet)
            }
        }

        return packets
    }

    /**
     * Clear polyline from device and prepare for new route data.
     * Official protocol: [0x1F] [0x00] [optional: field1=sequenceNum] [field3=10]
     *
     * From HCI capture, the CLEAR command always includes protobuf field 3 = 10 (0x18 0x0a),
     * and an incrementing sequence number starting from the second update onward.
     *
     * @param sequenceNum Incrementing counter (0 = first/no sequence field, 1+ = included)
     */
    fun clearPolyline(sequenceNum: Int = 0): ByteArray {
        val result = mutableListOf<Byte>(CMD_SET_POLYLINE, 0x00)
        if (sequenceNum > 0) {
            result.add(0x08)  // protobuf field 1, wire type 0 (varint)
            encodeVarint(sequenceNum).forEach { result.add(it) }
        }
        result.add(0x18)  // protobuf field 3, wire type 0 (varint)
        result.add(0x0A)  // value = 10
        return result.toByteArray()
    }

    /**
     * Build a start or end point marker packet.
     * From HCI capture, the official app sends these as single Feature packets
     * with special negative featureIds (encoded as 10-byte protobuf varints).
     *
     * Start marker featureId varint: CA FF FF FF FF FF FF FF FF 01 (zigzag -27)
     * End marker featureId varint:   C8 FF FF FF FF FF FF FF FF 01 (zigzag -28)
     *
     * @param isStart true for start marker, false for end marker
     * @param point The coordinate point (delta-encoded, relative to reference)
     */
    fun buildStartEndMarker(isStart: Boolean, point: PolylinePoint): ByteArray {
        // Protobuf field 1 (featureId) as 10-byte varint
        val featureIdBytes = if (isStart) {
            byteArrayOf(0xCA.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(),
                0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0x01)
        } else {
            byteArrayOf(0xC8.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(),
                0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0x01)
        }

        // Protobuf field 2 (coordinates) = length-delimited, zigzag-varint encoded
        val coordBytes = mutableListOf<Byte>()
        encodeZigzagVarint(point.x).forEach { coordBytes.add(it) }
        encodeZigzagVarint(point.y).forEach { coordBytes.add(it) }
        val coordData = coordBytes.toByteArray()

        // Build: [0x1F] [0x10 = single Feature] [0x08] [featureId varint] [0x12] [len] [coords]
        val result = mutableListOf<Byte>()
        result.add(CMD_SET_POLYLINE)
        result.add(0x10)  // header: messageType=1 (Feature), txType=0 (Single)
        result.add(0x08)  // protobuf field 1, wire type 0
        featureIdBytes.forEach { result.add(it) }
        result.add(0x12)  // protobuf field 2, wire type 2 (length-delimited)
        result.add(coordData.size.toByte())
        coordData.forEach { result.add(it) }

        return result.toByteArray()
    }

    /**
     * Commit/render polyline on device display.
     * Must be sent after all polyline feature packets to trigger rendering.
     * Official protocol: [0x1F] [0x20] [0x08] [0x01] (protobuf field1=1)
     */
    fun commitPolyline(): ByteArray {
        return byteArrayOf(CMD_SET_POLYLINE, 0x20, 0x08, 0x01)
    }

    /**
     * End ride and return to idle screen
     */
    fun endRide(): ByteArray {
        return byteArrayOf(
            CMD_SET_RIDE_STATUS,
            0xFF.toByte(),
            0xFF.toByte(),
            0xFF.toByte(),
            0xFF.toByte()
        )
    }

    /**
     * Set screen to idle/home mode
     */
    fun setScreenIdle(): ByteArray {
        return byteArrayOf(
            CMD_SET_SCREEN,
            0x01,
            0x01
        )
    }

}

enum class ChargeState(val byteValue: Int) {
    DISCHARGING(0x00),
    CHARGING(0x01),
    CHARGED(0x02),
    FAULT(0x03);

    companion object {
        fun fromByte(value: Int): ChargeState = values().find { it.byteValue == value } ?: DISCHARGING
    }
}

data class Destination(val type: Int, val id: Int, val name: String) {
    companion object {
        const val TYPE_NONE = 0
        const val TYPE_LOCATION = 1
        const val TYPE_ROUTE = 2
    }
}
