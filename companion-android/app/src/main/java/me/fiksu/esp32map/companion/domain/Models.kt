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

data class RoutePackageVersion(
    val major: Int,
    val minor: Int,
) {
    companion object {
        val CURRENT = RoutePackageVersion(1, 0)
    }
}

enum class RouteManeuverType {
    DEPART,
    STRAIGHT,
    SLIGHT_LEFT,
    LEFT,
    SHARP_LEFT,
    SLIGHT_RIGHT,
    RIGHT,
    SHARP_RIGHT,
    UTURN,
    ROUNDABOUT,
    MERGE,
    RAMP,
    ARRIVE,
}

data class RouteManeuver(
    val id: String,
    val maneuverType: RouteManeuverType,
    val location: CoordinatePoint,
    val distanceFromStartMeters: Double,
    val distanceToNextMeters: Double?,
    val instructionText: String?,
)

data class RouteSummary(
    val totalDistanceMeters: Double,
    val estimatedDurationSeconds: Int,
    val startLabel: String?,
    val destinationLabel: String?,
)

data class RouteProvenance(
    val providerId: RouteProviderId,
    val sourceReference: String?,
    val generatedAtUnixMs: Long,
)

data class NormalizedRoutePackage(
    val version: RoutePackageVersion,
    val routeIdentifier: String,
    val revision: Int,
    val geometry: List<CoordinatePoint>,
    val maneuvers: List<RouteManeuver>,
    val summary: RouteSummary,
    val provenance: RouteProvenance,
) {
    val geometryPointCount: Int get() = geometry.size
    val maneuverCount: Int get() = maneuvers.size
    val summaryLine: String get() = "${summary.totalDistanceMeters.toInt()} m • ${maxOf(summary.estimatedDurationSeconds / 60, 1)} min"
}

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
    val normalizedPackage: NormalizedRoutePackage,
)

data class RoutePreviewModel(
    val alternatives: List<RouteAlternative> = emptyList(),
    val selectedAlternativeId: String? = null,
    val routeIdentifier: String? = null,
    val routeRevision: Int? = null,
) {
    val selectedAlternative: RouteAlternative?
        get() = alternatives.firstOrNull { it.id == selectedAlternativeId } ?: alternatives.firstOrNull()
}

data class ActiveRouteSession(
    val routeIdentifier: String? = null,
    val routeRevision: Int? = null,
    val destinationLabel: String = "No destination",
    val destinationCoordinate: CoordinatePoint? = null,
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
