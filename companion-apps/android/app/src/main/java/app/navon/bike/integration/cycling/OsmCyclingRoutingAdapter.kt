package app.navon.bike.integration.cycling

import java.net.HttpURLConnection
import java.net.URL
import java.util.Locale
import java.util.UUID
import kotlin.math.abs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import app.navon.bike.domain.ActiveRouteSession
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.RouteAlternative
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.domain.RoutePackageVersion
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.RoutePreviewModel
import app.navon.bike.domain.RouteProvenance
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RerouteContext
import app.navon.bike.domain.RouteSummary
import app.navon.bike.domain.RoutingProvider
import app.navon.bike.integration.sample.SampleRoutingAdapter
import org.json.JSONArray
import org.json.JSONObject

/**
 * OSM cycling routing orchestrator. Issues parallel requests to BRouter
 * (`fastbike` paths-preferred + `trekking` balanced) AND OSRM bike, then
 * exposes whichever succeed as 1-3 alternatives in the route preview.
 *
 * Mirrors the web companion's `OsmCyclingRoutingAdapter.ts` byte-for-byte
 * in semantics. See `docs/companion-app-architecture.md` for the design.
 */
class OsmCyclingRoutingAdapter : RoutingProvider {
    companion object {
        private const val MIN_HEADING_SPEED_MPS = 2.0
        private const val REROUTE_FORWARD_SHIFT_M = 15.0
    }
    override val providerId: RouteProviderId = RouteProviderId.OSM
    override val isAvailableInV1: Boolean = true

    /** Last-resort fallback when all live sources fail. */
    private val sample = SampleRoutingAdapter(RouteProviderId.OSM)

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        return fanOutOrFallback(request, revision = 1)
    }

    override suspend fun replanRoute(
        session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?,
    ): RoutePreviewModel {
        val rerouteOrigin = headingBiasedOrigin(riderLocation, rerouteContext, "osm")
        val request = RoutePlanRequest(
            origin = rerouteOrigin,
            destination = session.destinationCoordinate ?: riderLocation,
            providerId = providerId,
        )
        return fanOutOrFallback(request, (session.routeRevision ?: 0) + 1)
    }

    private fun headingBiasedOrigin(
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?,
        providerLabel: String,
    ): CoordinatePoint {
        val heading = rerouteContext?.headingDegrees
        val speed = rerouteContext?.speedMps
        if (heading == null || !heading.isFinite()) {
            println("[reroute_heading] provider=$providerLabel reason=no_heading")
            return riderLocation
        }
        if (speed == null || !speed.isFinite() || speed < MIN_HEADING_SPEED_MPS) {
            println("[reroute_heading] provider=$providerLabel reason=low_speed speed=${speed ?: "nil"}")
            return riderLocation
        }
        val shifted = shiftPointByHeading(riderLocation, heading, REROUTE_FORWARD_SHIFT_M)
        if (!shifted.latitude.isFinite() || !shifted.longitude.isFinite() ||
            (shifted.latitude == riderLocation.latitude && shifted.longitude == riderLocation.longitude)
        ) {
            println("[reroute_heading] provider=$providerLabel reason=shift_failed")
            return riderLocation
        }
        println("[reroute_heading] provider=$providerLabel reason=applied")
        return shifted
    }

    private fun shiftPointByHeading(
        point: CoordinatePoint,
        headingDegrees: Double,
        distanceMeters: Double,
    ): CoordinatePoint {
        val metersPerDegLat = 111_320.0
        val rad = Math.toRadians(headingDegrees)
        val northM = kotlin.math.cos(rad) * distanceMeters
        val eastM = kotlin.math.sin(rad) * distanceMeters
        val lat = point.latitude + northM / metersPerDegLat
        val lonScale = metersPerDegLat * kotlin.math.cos(Math.toRadians(point.latitude))
        val lon = if (lonScale == 0.0) point.longitude else point.longitude + eastM / lonScale
        return CoordinatePoint(latitude = lat, longitude = lon)
    }

    override fun normalizePreview(
        preview: RoutePreviewModel,
        request: RoutePlanRequest,
    ): NormalizedRoutePackage {
        return preview.selectedAlternative?.normalizedPackage
            ?: error("OsmCyclingRoutingAdapter: preview has no alternatives")
    }

    private suspend fun fanOutOrFallback(
        request: RoutePlanRequest,
        revision: Int,
    ): RoutePreviewModel = coroutineScope {
        val tasks = listOf(
            async(Dispatchers.IO) {
                runCatching {
                    val feature = BrouterClient.fetch(BrouterProfile.FASTBIKE, request.origin, request.destination)
                    mapBrouterToAlternative(feature, request, revision, BrouterProfile.FASTBIKE, "Bike-paths first")
                }.getOrNull()
            },
            async(Dispatchers.IO) {
                runCatching {
                    val feature = BrouterClient.fetch(BrouterProfile.TREKKING, request.origin, request.destination)
                    mapBrouterToAlternative(feature, request, revision, BrouterProfile.TREKKING, "Balanced cycling")
                }.getOrNull()
            },
            async(Dispatchers.IO) {
                runCatching { fetchOsrmAlternative(request, revision) }.getOrNull()
            },
        )
        val results = tasks.map { it.await() }.filterNotNull()
        val deduped = dedupeAlternatives(results)
        if (deduped.isEmpty()) {
            return@coroutineScope sample.planRoute(request).copy(
                planningNotice = "Showing sample route — live routing failed",
            )
        }
        val failedCount = tasks.size - deduped.size
        RoutePreviewModel(
            alternatives = deduped,
            selectedAlternativeId = deduped.first().id,
            routeIdentifier = deduped.first().normalizedPackage.routeIdentifier,
            routeRevision = deduped.first().normalizedPackage.revision,
            planningNotice = if (failedCount == 0) {
                "Cycling alternatives via BRouter + OSRM"
            } else {
                "Cycling alternatives — $failedCount source${if (failedCount == 1) "" else "s"} unavailable"
            },
        )
    }

    private suspend fun fetchOsrmAlternative(
        request: RoutePlanRequest,
        revision: Int,
    ): RouteAlternative? = withContext(Dispatchers.IO) {
        val coordinates = String.format(
            Locale.US,
            "%.6f,%.6f;%.6f,%.6f",
            request.origin.longitude,
            request.origin.latitude,
            request.destination.longitude,
            request.destination.latitude,
        )
        val url = URL(
            "https://router.project-osrm.org/route/v1/bike/$coordinates?alternatives=false&overview=full&steps=true&geometries=geojson",
        )
        val connection = (url.openConnection() as HttpURLConnection).apply {
            requestMethod = "GET"
            connectTimeout = 10_000
            readTimeout = 10_000
        }
        val statusCode = connection.responseCode
        val body = runCatching {
            val stream = if (statusCode in 200..299) connection.inputStream else connection.errorStream
            stream?.bufferedReader()?.use { it.readText() }.orEmpty()
        }.getOrDefault("")
        if (statusCode !in 200..299) throw IllegalStateException("HTTP $statusCode")
        val root = JSONObject(body)
        if (root.optString("code") != "Ok") throw IllegalStateException("OSRM error")
        val routes = root.optJSONArray("routes") ?: throw IllegalStateException("OSRM no routes")
        if (routes.length() == 0) throw IllegalStateException("OSRM empty routes")
        mapOsrmRoute(routes.getJSONObject(0), request, revision)
    }
}

private fun mapOsrmRoute(
    routeJson: JSONObject,
    request: RoutePlanRequest,
    revision: Int,
): RouteAlternative? {
    val geomJson = routeJson.optJSONObject("geometry") ?: return null
    val coordsJson = geomJson.optJSONArray("coordinates") ?: return null
    if (coordsJson.length() < 2) return null
    val geometry = mutableListOf<CoordinatePoint>()
    for (i in 0 until coordsJson.length()) {
        val pair = coordsJson.optJSONArray(i) ?: continue
        if (pair.length() < 2) continue
        geometry += CoordinatePoint(latitude = pair.optDouble(1), longitude = pair.optDouble(0))
    }
    if (geometry.size < 2) return null
    val distance = routeJson.optDouble("distance", 0.0)
    val duration = routeJson.optDouble("duration", 0.0).toInt().coerceAtLeast(60)
    val maneuvers = buildOsrmManeuvers(routeJson, geometry)
    val routePackage = NormalizedRoutePackage(
        version = RoutePackageVersion.CURRENT,
        routeIdentifier = osrmRouteId(request),
        revision = revision,
        geometry = geometry,
        maneuvers = maneuvers,
        summary = RouteSummary(
            totalDistanceMeters = distance,
            estimatedDurationSeconds = duration,
            startLabel = "Current location",
            destinationLabel = "Selected destination",
        ),
        provenance = RouteProvenance(
            providerId = RouteProviderId.OSM,
            sourceReference = "OSRM bike",
            generatedAtUnixMs = System.currentTimeMillis(),
        ),
    )
    val km = String.format(Locale.US, "%.1f", distance / 1000.0)
    val min = maxOf(duration / 60, 1)
    return RouteAlternative(
        id = UUID.randomUUID().toString(),
        title = "Fastest",
        subtitle = "$km km • $min min",
        distanceMeters = distance.toInt(),
        durationSeconds = duration,
        normalizedPackage = routePackage,
    )
}

private fun buildOsrmManeuvers(
    routeJson: JSONObject,
    geometry: List<CoordinatePoint>,
): List<RouteManeuver> {
    val maneuvers = mutableListOf<RouteManeuver>()
    val legsJson = routeJson.optJSONArray("legs") ?: JSONArray()
    val firstNext = firstOsrmStepDistance(legsJson)
    maneuvers += RouteManeuver(
        id = "depart",
        maneuverType = RouteManeuverType.DEPART,
        location = geometry.firstOrNull() ?: CoordinatePoint(0.0, 0.0),
        distanceFromStartMeters = 0.0,
        distanceToNextMeters = firstNext,
        instructionText = "Start riding",
    )
    var distanceFromStart = 0.0
    for (li in 0 until legsJson.length()) {
        val steps = legsJson.optJSONObject(li)?.optJSONArray("steps") ?: continue
        for (si in 0 until steps.length()) {
            val step = steps.getJSONObject(si)
            val distance = step.optDouble("distance", 0.0)
            val maneuverJson = step.optJSONObject("maneuver")
            if (maneuverJson == null) {
                distanceFromStart += distance
                continue
            }
            val type = maneuverJson.optString("type").lowercase(Locale.ROOT)
            if (type == "depart" || type == "arrive" || type == "notification" ||
                type == "new name" || type == "continue"
            ) {
                distanceFromStart += distance
                continue
            }
            val locationJson = maneuverJson.optJSONArray("location")
            if (locationJson == null || locationJson.length() < 2) {
                distanceFromStart += distance
                continue
            }
            val location = CoordinatePoint(
                latitude = locationJson.optDouble(1),
                longitude = locationJson.optDouble(0),
            )
            val name = step.optString("name")
            maneuvers += RouteManeuver(
                id = if (name.isBlank()) "step-${maneuvers.size}" else "step-${maneuvers.size}-$name",
                maneuverType = osrmManeuverType(type, maneuverJson.optString("modifier")),
                location = location,
                distanceFromStartMeters = distanceFromStart,
                distanceToNextMeters = distance.takeIf { it > 0.0 },
                instructionText = osrmInstructionText(type, maneuverJson.optString("modifier"), name),
            )
            distanceFromStart += distance
        }
    }
    maneuvers += RouteManeuver(
        id = "arrive",
        maneuverType = RouteManeuverType.ARRIVE,
        location = geometry.lastOrNull() ?: CoordinatePoint(0.0, 0.0),
        distanceFromStartMeters = routeJson.optDouble("distance", distanceFromStart),
        distanceToNextMeters = null,
        instructionText = "Arrive at destination",
    )
    return maneuvers
}

private fun firstOsrmStepDistance(legsJson: JSONArray): Double? {
    for (li in 0 until legsJson.length()) {
        val steps = legsJson.optJSONObject(li)?.optJSONArray("steps") ?: continue
        for (si in 0 until steps.length()) {
            val step = steps.getJSONObject(si)
            val type = step.optJSONObject("maneuver")?.optString("type")?.lowercase(Locale.ROOT)
                ?: continue
            if (type != "depart") return step.optDouble("distance").takeIf { it > 0.0 }
        }
    }
    return null
}

private fun osrmManeuverType(type: String, modifier: String?): RouteManeuverType = when (type) {
    "roundabout", "rotary" -> RouteManeuverType.ROUNDABOUT
    "merge", "fork", "on ramp", "off ramp" -> RouteManeuverType.MERGE
    "arrive" -> RouteManeuverType.ARRIVE
    else -> when (modifier?.lowercase(Locale.ROOT)) {
        "uturn" -> RouteManeuverType.UTURN
        "sharp right" -> RouteManeuverType.SHARP_RIGHT
        "right" -> RouteManeuverType.RIGHT
        "slight right" -> RouteManeuverType.SLIGHT_RIGHT
        "sharp left" -> RouteManeuverType.SHARP_LEFT
        "left" -> RouteManeuverType.LEFT
        "slight left" -> RouteManeuverType.SLIGHT_LEFT
        else -> RouteManeuverType.STRAIGHT
    }
}

private fun osrmInstructionText(type: String, modifier: String?, name: String): String = when (type) {
    "roundabout", "rotary" -> "Enter roundabout"
    "merge" -> "Merge"
    "fork" -> when {
        modifier?.contains("left", ignoreCase = true) == true -> "Keep left"
        modifier?.contains("right", ignoreCase = true) == true -> "Keep right"
        else -> "Keep to the fork"
    }
    else -> when (modifier?.lowercase(Locale.ROOT)) {
        "uturn" -> "Make a U-turn"
        "sharp right" -> "Turn sharply right"
        "right" -> "Turn right"
        "slight right" -> "Bear right"
        "sharp left" -> "Turn sharply left"
        "left" -> "Turn left"
        "slight left" -> "Bear left"
        else -> if (name.isBlank()) "Continue" else "Continue on $name"
    }
}

private fun osrmRouteId(request: RoutePlanRequest): String {
    val o = String.format(Locale.US, "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
    val d = String.format(Locale.US, "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
    return "osm-osrm:$o->$d"
}

private fun dedupeAlternatives(candidates: List<RouteAlternative>): List<RouteAlternative> {
    val kept = mutableListOf<RouteAlternative>()
    for (c in candidates) {
        if (kept.none { areNearIdentical(it, c) }) kept += c
    }
    return kept
}

private fun areNearIdentical(a: RouteAlternative, b: RouteAlternative): Boolean {
    val aLen = a.normalizedPackage.summary.totalDistanceMeters
    val bLen = b.normalizedPackage.summary.totalDistanceMeters
    if (aLen <= 0 || bLen <= 0) return false
    val lengthDelta = abs(aLen - bLen) / maxOf(aLen, bLen)
    if (lengthDelta > 0.03) return false
    val ag = a.normalizedPackage.geometry
    val bg = b.normalizedPackage.geometry
    if (ag.isEmpty() || bg.isEmpty()) return false
    return samePoint(ag.first(), bg.first()) &&
        samePoint(ag.last(), bg.last()) &&
        samePoint(ag[ag.size / 2], bg[bg.size / 2])
}

private fun samePoint(a: CoordinatePoint, b: CoordinatePoint): Boolean {
    return abs(a.latitude - b.latitude) < 1e-4 && abs(a.longitude - b.longitude) < 1e-4
}
