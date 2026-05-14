package app.navon.bike.domain

import java.util.UUID

// MARK: - Event data (type-tag pattern for Gson compatibility)

data class RouteAltInfo(
    val providerName: String = "",
    val routeId: String = "",
    val label: String = "",
)

data class RoutingDiagEventData(
    val kind: String = "",
    // locationUpdate
    val lat: Double = 0.0,
    val lon: Double = 0.0,
    val heading: Double? = null,
    val speed: Double? = null,
    val accuracyM: Double? = null,
    // destinationChanged
    val label: String? = null,
    // routeAlternativesSuggested
    val alternatives: List<RouteAltInfo>? = null,
    // routeSelected
    val alternativeId: String? = null,
    val providerName: String? = null,
    val routeId: String? = null,
    // compassModeChanged
    val from: String? = null,
    val to: String? = null,
    // audioCueDispatched
    val cueType: String? = null,
    val messageText: String? = null,
    // nextTurnAlerted
    val instructionText: String? = null,
    val distanceRemainingM: Double? = null,
    // offRouteDetected
    val distanceM: Double? = null,
    // routeStopped
    val reason: String? = null,
    // rerouteCompleted
    val result: String? = null,
) {
    companion object {
        fun locationUpdate(lat: Double, lon: Double, heading: Double? = null, speed: Double? = null, accuracyM: Double? = null) =
            RoutingDiagEventData(kind = "locationUpdate", lat = lat, lon = lon, heading = heading, speed = speed, accuracyM = accuracyM)

        fun destinationChanged(label: String, lat: Double, lon: Double) =
            RoutingDiagEventData(kind = "destinationChanged", label = label, lat = lat, lon = lon)

        fun routeAlternativesSuggested(alternatives: List<RouteAltInfo>) =
            RoutingDiagEventData(kind = "routeAlternativesSuggested", alternatives = alternatives)

        fun routeSelected(alternativeId: String, providerName: String, routeId: String, label: String) =
            RoutingDiagEventData(kind = "routeSelected", alternativeId = alternativeId, providerName = providerName, routeId = routeId, label = label)

        fun routeStarted() = RoutingDiagEventData(kind = "routeStarted")

        fun routeStopped(reason: String? = null) = RoutingDiagEventData(kind = "routeStopped", reason = reason)

        fun exploreAlternatives() = RoutingDiagEventData(kind = "exploreAlternatives")

        fun compassModeChanged(from: String, to: String) = RoutingDiagEventData(kind = "compassModeChanged", from = from, to = to)

        fun audioCueDispatched(cueType: String, messageText: String) = RoutingDiagEventData(kind = "audioCueDispatched", cueType = cueType, messageText = messageText)

        fun nextTurnAlerted(instructionText: String, distanceRemainingM: Double) = RoutingDiagEventData(kind = "nextTurnAlerted", instructionText = instructionText, distanceRemainingM = distanceRemainingM)

        fun offRouteDetected(distanceM: Double) = RoutingDiagEventData(kind = "offRouteDetected", distanceM = distanceM)

        fun rerouteRequested() = RoutingDiagEventData(kind = "rerouteRequested")

        fun rerouteCompleted(result: String) = RoutingDiagEventData(kind = "rerouteCompleted", result = result)
    }
}

// MARK: - Event

data class RoutingDiagEvent(
    val id: String = "",
    val timestampMs: Long = 0L,
    val data: RoutingDiagEventData = RoutingDiagEventData(),
)

// MARK: - Route Geometry Entry

data class RouteGeometryEntry(
    val routeId: String = "",
    val providerName: String = "",
    val geometry: List<CoordinatePoint> = emptyList(),
)

// MARK: - Session

data class RoutingDiagSession(
    val id: String = "",
    val createdAtMs: Long = 0L,
    val updatedAtMs: Long = 0L,
    val events: List<RoutingDiagEvent> = emptyList(),
    val routeGeometries: List<RouteGeometryEntry>? = null,
) {
    val eventCount: Int get() = events.size
    val durationMs: Long get() = if (events.isEmpty()) 0L else updatedAtMs - createdAtMs

    fun debugPackageText(): String {
        val gson = com.google.gson.GsonBuilder().setPrettyPrinting().create()
        val pkg = mutableMapOf<String, Any?>(
            "formatVersion" to 1,
            "sessionId" to id,
            "createdAtMs" to createdAtMs,
            "eventCount" to eventCount,
            "events" to events,
        )
        routeGeometries?.let { pkg["routeGeometries"] = it }
        return gson.toJson(pkg)
    }
}

// MARK: - Constants

const val ROUTING_DIAGNOSTICS_SESSION_LIMIT = 20
const val LOCATION_EVENT_THROTTLE_MS = 5000L

private var nextEventCounter = 0

fun newSessionId(): String = "rd-${System.currentTimeMillis()}-${UUID.randomUUID().toString().take(6)}"

fun newEventId(): String = "e${++nextEventCounter}"

fun nowMs(): Long = System.currentTimeMillis()
