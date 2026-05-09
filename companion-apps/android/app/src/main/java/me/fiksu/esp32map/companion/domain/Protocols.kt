package me.fiksu.esp32map.companion.domain

interface RoutingProvider {
    val providerId: RouteProviderId
    val isAvailableInV1: Boolean
    suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel
    suspend fun replanRoute(
        session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext? = null,
    ): RoutePreviewModel
    fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage
}

interface RouteSyncTransport {
    suspend fun scanForDevices()
    suspend fun connectToLastKnownDevice()
    suspend fun publishSet(route: NormalizedRoutePackage)
    suspend fun publishUpdate(route: NormalizedRoutePackage)
    suspend fun publishClear(routeIdentifier: String?)
    suspend fun resumePendingTransfer()
    fun armRetryableInterruptionOnNextTransfer()
    fun armFaultInjection(mode: RouteSyncFaultInjectionMode)
    suspend fun receiveStatus(message: RouteStatusMessage)
    suspend fun receiveRerouteRequest(message: RouteRerouteRequestMessage)
}

interface RouteSessionStore {
    fun loadRecentDestinations(): List<CoordinatePoint>
    fun saveRecentDestination(point: CoordinatePoint)
    fun loadLastSession(): ActiveRouteSession?
    fun saveSession(session: ActiveRouteSession)
}

enum class LocationErrorKind { DENIED, UNAVAILABLE, TIMEOUT, UNSUPPORTED }

data class LocationState(
    val currentLocation: CoordinatePoint? = null,
    val lastKnownLocation: CoordinatePoint? = null,
    val isLocating: Boolean = false,
    val lastError: LocationErrorKind? = null,
    /**
     * Instantaneous ground speed (m/s) for the most recent fix, or `null` if
     * the platform did not report it. Used by the speed badge.
     */
    val currentSpeedMps: Double? = null,
)

interface LocationService {
    val state: kotlinx.coroutines.flow.StateFlow<LocationState>
    /** Begin watching the device's foreground location. Idempotent. */
    fun start()
    /** Pause watching. Idempotent. */
    fun stop()
    /** Whether the platform location permission is granted. */
    fun hasLocationPermission(): Boolean
}
