package me.fiksu.esp32map.companion.integration

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.os.Looper
import androidx.core.content.ContextCompat
import com.google.android.gms.location.FusedLocationProviderClient
import com.google.android.gms.location.LocationCallback
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.LocationResult
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.LocationErrorKind
import me.fiksu.esp32map.companion.domain.LocationService
import me.fiksu.esp32map.companion.domain.LocationState
import me.fiksu.esp32map.companion.integration.persistence.CompanionPersistence

class AndroidLocationService(
    private val context: Context,
    private val persistence: CompanionPersistence,
) : LocationService {

    private val client: FusedLocationProviderClient = LocationServices.getFusedLocationProviderClient(context)
    private val _state = MutableStateFlow(
        LocationState(lastKnownLocation = persistence.loadLastKnownRider())
    )
    override val state: StateFlow<LocationState> = _state.asStateFlow()

    private var watching = false

    private val callback = object : LocationCallback() {
        override fun onLocationResult(result: LocationResult) {
            val last: Location = result.lastLocation ?: return
            val point = CoordinatePoint(latitude = last.latitude, longitude = last.longitude)
            persistence.saveLastKnownRider(point)
            _state.value = _state.value.copy(
                currentLocation = point,
                lastKnownLocation = point,
                isLocating = false,
                lastError = null,
            )
        }
    }

    @SuppressLint("MissingPermission")
    override fun start() {
        if (watching) return
        if (!hasLocationPermission()) {
            _state.value = _state.value.copy(isLocating = false, lastError = LocationErrorKind.DENIED)
            return
        }
        watching = true
        _state.value = _state.value.copy(isLocating = true, lastError = null)
        val request = LocationRequest.Builder(Priority.PRIORITY_BALANCED_POWER_ACCURACY, 5_000L)
            .setMinUpdateDistanceMeters(10f)
            .setWaitForAccurateLocation(false)
            .build()
        try {
            client.requestLocationUpdates(request, callback, Looper.getMainLooper())
        } catch (security: SecurityException) {
            watching = false
            _state.value = _state.value.copy(isLocating = false, lastError = LocationErrorKind.DENIED)
        }
    }

    override fun stop() {
        if (!watching) return
        watching = false
        client.removeLocationUpdates(callback)
        _state.value = _state.value.copy(isLocating = false)
    }

    fun hasLocationPermission(): Boolean {
        val fine = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION)
        val coarse = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION)
        return fine == PackageManager.PERMISSION_GRANTED || coarse == PackageManager.PERMISSION_GRANTED
    }
}
