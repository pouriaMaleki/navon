package me.fiksu.esp32map.companion.domain

enum class RouteProviderId(val displayName: String, val isAvailableInV1: Boolean) {
    HSL("HSL", true),
    OSM("OSM", false),
    GOOGLE_INGEST("Google Ingest", false),
    GPX_IMPORT("GPX Import", false),
    FIT_IMPORT("FIT Import", false),
    TCX_IMPORT("TCX Import", false),
    GARMIN_API("Garmin API", false),
    GARMIN_FILE("Garmin File", false),
}

data class CoordinatePoint(
    val latitude: Double,
    val longitude: Double,
)

data class RoutePlanRequest(
    val origin: CoordinatePoint,
    val destination: CoordinatePoint,
    val providerId: RouteProviderId,
)

data class RouteAlternative(
    val id: String,
    val title: String,
    val subtitle: String,
    val distanceMeters: Int,
    val durationSeconds: Int,
)

data class RoutePreviewModel(
    val alternatives: List<RouteAlternative> = emptyList(),
    val selectedAlternativeId: String? = null,
    val routeIdentifier: String? = null,
    val routeRevision: Int? = null,
)

data class ActiveRouteSession(
    val routeIdentifier: String? = null,
    val routeRevision: Int? = null,
    val destinationLabel: String = "No destination",
    val providerId: RouteProviderId = RouteProviderId.HSL,
    val lastRerouteReason: String? = null,
    val lastRerouteTimestamp: String? = null,
)

enum class DeviceConnectionState {
    DISCONNECTED,
    SCANNING,
    CONNECTING,
    CONNECTED,
}

enum class RouteSyncState {
    IDLE,
    PREPARING,
    TRANSFERRING,
    AWAITING_ACK,
    SYNCED,
    FAILED,
}

data class SyncSessionState(
    val connectionState: DeviceConnectionState = DeviceConnectionState.DISCONNECTED,
    val routeSyncState: RouteSyncState = RouteSyncState.IDLE,
    val lastSyncResult: String = "Not sent yet",
    val lastDeviceName: String? = null,
)

data class CompanionDiagnostics(
    val providerName: String,
    val routeIdentifier: String,
    val routeRevision: Int,
    val bleState: String,
    val lastSyncResult: String,
    val lastRerouteOutcome: String,
)

data class NormalizedRoutePackage(
    val routeIdentifier: String,
    val revision: Int,
    val providerId: RouteProviderId,
    val summary: String,
    val geometryPointCount: Int,
    val maneuverCount: Int,
)
