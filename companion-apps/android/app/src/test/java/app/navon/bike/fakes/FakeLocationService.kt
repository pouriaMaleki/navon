package app.navon.bike.fakes

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.LocationErrorKind
import app.navon.bike.domain.LocationService
import app.navon.bike.domain.LocationState

/**
 * Test double for `LocationService`. Tests call `emitFix` / `emitError` to
 * drive the state flow; the real FusedLocationProvider is never touched.
 */
class FakeLocationService : LocationService {
    private val _state = MutableStateFlow(LocationState())
    override val state: StateFlow<LocationState> = _state.asStateFlow()

    override fun hasLocationPermission(): Boolean = true

    var startCount: Int = 0
        private set
    var stopCount: Int = 0
        private set

    override fun start() {
        startCount++
        _state.value = _state.value.copy(isLocating = true, lastError = null)
    }

    override fun stop() {
        stopCount++
        _state.value = _state.value.copy(isLocating = false)
    }

    fun emitFix(latitude: Double, longitude: Double) {
        val point = CoordinatePoint(latitude, longitude)
        _state.value = LocationState(
            currentLocation = point,
            lastKnownLocation = point,
            isLocating = false,
            lastError = null,
        )
    }

    fun emitError(kind: LocationErrorKind) {
        _state.value = _state.value.copy(lastError = kind, isLocating = false)
    }
}
