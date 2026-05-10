package me.fiksu.esp32map.companion.feature.device

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import me.fiksu.esp32map.companion.integration.ble.RouteSyncBluetoothClient

/**
 * Periodically reads the phone's GPS from [AndroidLocationService] and writes
 * it to the device's phone-GPS BLE characteristic at ~1 Hz while phone GPS
 * mode is active. The firmware auto-detects sample writes and switches to
 * Phone GPS mode; when samples stop (disconnect / toggle off), it auto-falls
 * back to Internal GPS after a timeout window (currently 120 seconds in
 * firmware).
 */
class PhoneGpsForwarder(
    private val bleClient: RouteSyncBluetoothClient,
    private val locationService: me.fiksu.esp32map.companion.integration.AndroidLocationService,
) {
    private val _isForwarding = MutableStateFlow(false)
    val isForwarding: StateFlow<Boolean> = _isForwarding.asStateFlow()

    private var forwardingJob: Job? = null
    private val scope = CoroutineScope(Dispatchers.Main)

    fun start(intervalMs: Long = 1000L) {
        if (_isForwarding.value) return
        _isForwarding.value = true
        forwardingJob = scope.launch {
            while (true) {
                val state = locationService.state.value
                val location = state.currentLocation ?: state.lastKnownLocation
                if (location != null) {
                    val speed = state.currentSpeedMps ?: 0.0
                    try {
                        bleClient.writePhoneGpsSample(
                            lat = location.latitude,
                            lon = location.longitude,
                            speed = speed,
                            course = null,
                            accuracy = null,
                        )
                    } catch (_: Exception) {
                        // Swallow transient write errors; will retry on next
                        // interval. The firmware auto-falls back to Internal
                        // GPS after a longer timeout window, so a single
                        // missed write is
                        // harmless.
                    }
                }
                delay(intervalMs)
            }
        }
    }

    fun stop() {
        forwardingJob?.cancel()
        forwardingJob = null
        _isForwarding.value = false
    }
}
