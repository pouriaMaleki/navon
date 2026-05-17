package app.navon.bike.integration.cycling

import java.util.Locale
import java.util.UUID
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sqrt
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.RouteAlternative
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.domain.RoutePackageVersion
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.RouteProvenance
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RouteSummary
import org.json.JSONArray
import org.json.JSONObject

/**
 * Convert a BRouter Feature (`features[0]` from `format=geojson&timode=2`)
 * into a `RouteAlternative` ready to drop into a `RoutePreviewModel`.
 * Pure function — no I/O, callable from tests with the parity fixture.
 *
 * BRouter `voicehints` use integer command codes documented in BRouter's
 * source (`VoiceHint` class). This mapping mirrors the web companion's
 * mapBrouterToRoute.ts byte-for-byte.
 */
fun mapBrouterToAlternative(
    feature: JSONObject,
    request: RoutePlanRequest,
    revision: Int,
    profile: BrouterProfile,
    title: String,
): RouteAlternative? {
    val geomJson = feature.optJSONObject("geometry") ?: return null
    val coordsJson = geomJson.optJSONArray("coordinates") ?: return null
    if (coordsJson.length() < 2) return null
    val geometry = mutableListOf<CoordinatePoint>()
    for (i in 0 until coordsJson.length()) {
        val pair = coordsJson.optJSONArray(i) ?: continue
        if (pair.length() < 2) continue
        geometry += CoordinatePoint(latitude = pair.optDouble(1), longitude = pair.optDouble(0))
    }
    if (geometry.size < 2) return null
    val props = feature.optJSONObject("properties") ?: JSONObject()
    val distance = props.optString("track-length").toDoubleOrNull() ?: 0.0
    val duration = props.optString("total-time").toIntOrNull() ?: 0
    val maneuvers = buildManeuversFromVoiceHints(geometry, props.optJSONArray("voicehints") ?: JSONArray())
    val routePackage = NormalizedRoutePackage(
        version = RoutePackageVersion.CURRENT,
        routeIdentifier = brouterRouteId(request, profile),
        revision = revision,
        geometry = geometry,
        maneuvers = maneuvers,
        summary = RouteSummary(
            totalDistanceMeters = distance,
            estimatedDurationSeconds = maxOf(duration, 60),
            startLabel = "Current location",
            destinationLabel = "Selected destination",
        ),
        provenance = RouteProvenance(
            providerId = RouteProviderId.OSM,
            sourceReference = "BRouter ${profile.key}",
            generatedAtUnixMs = System.currentTimeMillis(),
        ),
    )
    val km = String.format(Locale.US, "%.1f", distance / 1000.0)
    val min = maxOf((duration / 60.0).toInt(), 1)
    return RouteAlternative(
        id = UUID.randomUUID().toString(),
        title = title,
        subtitle = "$km km • $min min",
        distanceMeters = distance.toInt(),
        durationSeconds = maxOf(duration, 60),
        normalizedPackage = routePackage,
    )
}

/**
 * BRouter `voicehints` shape: [geomIdx, cmdType, exitCount, distToNext, angle].
 * Cmd codes: 1=continue, 2=left, 3=slight-left, 4=sharp-left, 5=right,
 * 6=slight-right, 7=sharp-right, 8=keep-left, 9=keep-right, 10=u-turn,
 * 12=roundabout, 13=arrive.
 */
private fun buildManeuversFromVoiceHints(
    geometry: List<CoordinatePoint>,
    voicehints: JSONArray,
): List<RouteManeuver> {
    val maneuvers = mutableListOf<RouteManeuver>()
    val firstHint = if (voicehints.length() > 0) voicehints.optJSONArray(0) else null
    val firstNextDistance = firstHint?.optDouble(3)?.takeIf { it > 0.0 }
    maneuvers += RouteManeuver(
        id = "depart",
        maneuverType = RouteManeuverType.DEPART,
        location = geometry.firstOrNull() ?: CoordinatePoint(0.0, 0.0),
        distanceFromStartMeters = 0.0,
        distanceToNextMeters = firstNextDistance,
        instructionText = "Start riding",
    )
    for (i in 0 until voicehints.length()) {
        val hint = voicehints.optJSONArray(i) ?: continue
        if (hint.length() < 4) continue
        val geomIdx = hint.optInt(0)
        val cmd = hint.optInt(1)
        val distToNext = hint.optDouble(3, 0.0).takeIf { it > 0.0 }
        val type = cmdToManeuver(cmd)
        if (type == RouteManeuverType.STRAIGHT) continue
        val cappedIdx = geomIdx.coerceIn(0, geometry.lastIndex)
        maneuvers += RouteManeuver(
            id = "vh-$i",
            maneuverType = type,
            location = geometry[cappedIdx],
            distanceFromStartMeters = approxDistanceFromStart(geometry, cappedIdx),
            distanceToNextMeters = distToNext,
            instructionText = cmdInstruction(cmd),
        )
    }
    maneuvers += RouteManeuver(
        id = "arrive",
        maneuverType = RouteManeuverType.ARRIVE,
        location = geometry.lastOrNull() ?: CoordinatePoint(0.0, 0.0),
        distanceFromStartMeters = polylineLengthMeters(geometry),
        distanceToNextMeters = null,
        instructionText = "Arrive at destination",
    )
    return maneuvers
}

private fun cmdToManeuver(cmd: Int): RouteManeuverType = when (cmd) {
    2 -> RouteManeuverType.LEFT
    3 -> RouteManeuverType.SLIGHT_LEFT
    4 -> RouteManeuverType.SHARP_LEFT
    5 -> RouteManeuverType.RIGHT
    6 -> RouteManeuverType.SLIGHT_RIGHT
    7 -> RouteManeuverType.SHARP_RIGHT
    8 -> RouteManeuverType.SLIGHT_LEFT // keep-left ≈ slight-left
    9 -> RouteManeuverType.SLIGHT_RIGHT // keep-right ≈ slight-right
    10 -> RouteManeuverType.UTURN
    12 -> RouteManeuverType.ROUNDABOUT
    13 -> RouteManeuverType.ARRIVE
    else -> RouteManeuverType.STRAIGHT
}

private fun cmdInstruction(cmd: Int): String = when (cmd) {
    1 -> "Continue"
    2 -> "Turn left"
    3 -> "Slight left"
    4 -> "Turn sharply left"
    5 -> "Turn right"
    6 -> "Slight right"
    7 -> "Turn sharply right"
    8 -> "Keep left"
    9 -> "Keep right"
    10 -> "Make a U-turn"
    12 -> "Enter roundabout"
    13 -> "Arrive at destination"
    else -> "Continue"
}

private fun approxDistanceFromStart(geometry: List<CoordinatePoint>, targetIdx: Int): Double {
    var total = 0.0
    val cap = targetIdx.coerceIn(1, geometry.lastIndex)
    for (i in 1..cap) total += haversine(geometry[i - 1], geometry[i])
    return total
}

private fun polylineLengthMeters(geometry: List<CoordinatePoint>): Double {
    var total = 0.0
    for (i in 1 until geometry.size) total += haversine(geometry[i - 1], geometry[i])
    return total
}

private fun haversine(a: CoordinatePoint, b: CoordinatePoint): Double {
    val metersPerDegLat = 111_320.0
    val meanLatRad = (a.latitude + b.latitude) / 2.0 * PI / 180.0
    val dN = (b.latitude - a.latitude) * metersPerDegLat
    val dE = (b.longitude - a.longitude) * cos(meanLatRad) * metersPerDegLat
    return sqrt(dN * dN + dE * dE)
}

private fun brouterRouteId(request: RoutePlanRequest, profile: BrouterProfile): String {
    val o = String.format(Locale.US, "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
    val d = String.format(Locale.US, "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
    return "osm-brouter-${profile.key}:$o->$d"
}
