package me.fiksu.esp32map.companion.domain

interface RoutingProvider {
    val providerId: RouteProviderId
    val isAvailableInV1: Boolean
    suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel
    suspend fun replanRoute(session: ActiveRouteSession, riderLocation: CoordinatePoint): RoutePreviewModel
    fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage
}

interface RouteSyncTransport {
    suspend fun scanForDevices()
    suspend fun connectToLastKnownDevice()
    suspend fun sendRoute(route: NormalizedRoutePackage)
    suspend fun clearRoute(routeIdentifier: String?)
}

interface RouteSessionStore {
    fun loadRecentDestinations(): List<CoordinatePoint>
    fun saveRecentDestination(point: CoordinatePoint)
    fun loadLastSession(): ActiveRouteSession?
    fun saveSession(session: ActiveRouteSession)
}
