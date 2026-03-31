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
    ;

    val supportsCompanionPreview: Boolean
        get() = true
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

enum class RouteSyncStatusCode {
    ACCEPTED,
    APPLYING,
    ACTIVE,
    CLEARED,
    REJECTED,
    RETRYABLE_FAILURE,
    FATAL_FAILURE,
}

data class RouteSetMessage(
    val route: NormalizedRoutePackage,
)

data class RouteUpdateMessage(
    val routeIdentifier: String,
    val revision: Int,
    val route: NormalizedRoutePackage,
)

data class RouteClearMessage(
    val routeIdentifier: String?,
)

data class RouteStatusMessage(
    val routeIdentifier: String?,
    val revision: Int?,
    val status: RouteSyncStatusCode,
    val detail: String?,
)

data class RouteRerouteRequestMessage(
    val routeIdentifier: String,
    val riderLocation: CoordinatePoint,
    val reason: String,
)

sealed interface RouteSyncMessage {
    val kindLabel: String
    val debugSummary: String

    data class Set(val message: RouteSetMessage) : RouteSyncMessage {
        override val kindLabel: String = "set"
        override val debugSummary: String = "set ${message.route.routeIdentifier} rev ${message.route.revision}"
    }

    data class Update(val message: RouteUpdateMessage) : RouteSyncMessage {
        override val kindLabel: String = "update"
        override val debugSummary: String = "update ${message.routeIdentifier} rev ${message.revision}"
    }

    data class Clear(val message: RouteClearMessage) : RouteSyncMessage {
        override val kindLabel: String = "clear"
        override val debugSummary: String = "clear ${message.routeIdentifier ?: "current"}"
    }

    data class Status(val message: RouteStatusMessage) : RouteSyncMessage {
        override val kindLabel: String = "status"
        override val debugSummary: String = "status ${message.status.name.lowercase()} ${message.routeIdentifier ?: "none"}"
    }

    data class RerouteRequest(val message: RouteRerouteRequestMessage) : RouteSyncMessage {
        override val kindLabel: String = "reroute_request"
        override val debugSummary: String = "reroute_request ${message.routeIdentifier}"
    }
}

data class RoutePlanRequest(
    val origin: CoordinatePoint,
    val destination: CoordinatePoint,
    val providerId: RouteProviderId,
)

data class CompanionSettings(
    val preferLiveHslRouting: Boolean = false,
    val hslSubscriptionKey: String = "",
    val hslEndpointUrl: String = "https://api.digitransit.fi/routing/v2/hsl/gtfs/v1",
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
    val planningNotice: String? = null,
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

enum class RouteSyncFaultInjectionMode(val displayName: String) {
    RETRYABLE_INTERRUPTION("Retryable interruption"),
    WRITE_FAILURE("Write failure"),
    DISCONNECT_AFTER_CHUNK_WRITE("Disconnect after chunk"),
    DROP_NEXT_INBOUND_STATUS("Drop next inbound status"),
}

data class RouteTransferProgress(
    val transferIdentifier: String,
    val messageKind: String,
    val routeIdentifier: String?,
    val routeRevision: Int?,
    val payloadBytes: Int,
    val chunkSizeBytes: Int,
    val totalChunks: Int,
    val acknowledgedChunks: Int,
    val retryCount: Int,
    val checksumHex: String,
    val resumeChunkIndex: Int?,
    val lastError: String?,
) {
    val percentComplete: Int
        get() = if (totalChunks == 0) 0 else ((acknowledgedChunks.toDouble() / totalChunks.toDouble()) * 100.0).toInt()
}

data class SyncSessionState(
    val connectionState: DeviceConnectionState = DeviceConnectionState.DISCONNECTED,
    val routeSyncState: RouteSyncState = RouteSyncState.IDLE,
    val lastSyncResult: String = "Not sent yet",
    val lastDeviceName: String? = null,
    val pendingRouteIdentifier: String? = null,
    val pendingRouteRevision: Int? = null,
    val activeRouteIdentifier: String? = null,
    val activeRouteRevision: Int? = null,
    val activeRouteChecksumHex: String? = null,
    val transferProgress: RouteTransferProgress? = null,
    val retryableInterruptionArmed: Boolean = false,
    val armedFaultInjectionMode: RouteSyncFaultInjectionMode? = null,
    val lastOutboundMessage: RouteSyncMessage? = null,
    val lastInboundMessage: RouteSyncMessage? = null,
    val lastStatusCode: RouteSyncStatusCode? = null,
)

data class CompanionDiagnostics(
    val providerName: String,
    val routeIdentifier: String,
    val routeRevision: Int,
    val bleState: String,
    val lastSyncResult: String,
    val lastRerouteOutcome: String,
)
