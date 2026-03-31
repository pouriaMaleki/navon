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
