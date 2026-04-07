package me.fiksu.esp32map.companion.integration.sample

import java.net.HttpURLConnection
import java.net.URL
import java.util.Locale
import java.util.UUID
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sqrt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RouteManeuver
import me.fiksu.esp32map.companion.domain.RouteManeuverType
import me.fiksu.esp32map.companion.domain.RoutePackageVersion
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProvenance
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteSummary
import me.fiksu.esp32map.companion.domain.RoutingProvider
import org.json.JSONArray
import org.json.JSONObject

class SampleRoutingAdapter(
    override val providerId: RouteProviderId,
) : RoutingProvider {
    override val isAvailableInV1: Boolean = false

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        return if (providerId == RouteProviderId.OSM) {
            buildLiveOsmPreview(request, revision = 1)
        } else {
            buildPreview(request, revision = 1, planningNotice = planningNotice(providerId))
        }
    }

    override suspend fun replanRoute(session: ActiveRouteSession, riderLocation: CoordinatePoint): RoutePreviewModel {
        val request = RoutePlanRequest(
            origin = riderLocation,
            destination = session.destinationCoordinate ?: riderLocation,
            providerId = providerId,
        )
        val revision = (session.routeRevision ?: 0) + 1
        return if (providerId == RouteProviderId.OSM) {
            buildLiveOsmPreview(request, revision)
        } else {
            buildPreview(request, revision, planningNotice = "Rerouted with ${providerId.displayName} sample adapter")
        }
    }

    override fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
        return preview.selectedAlternative?.normalizedPackage ?: error("No sample route alternatives are available")
    }

    private suspend fun buildLiveOsmPreview(request: RoutePlanRequest, revision: Int): RoutePreviewModel {
        return try {
            fetchLiveOsmPreview(request, revision)
        } catch (error: Exception) {
            buildPreview(
                request,
                revision,
                planningNotice = "Live OSM failed: ${error.message ?: error::class.simpleName}. Showing sample route instead.",
            )
        }
    }

    private suspend fun fetchLiveOsmPreview(request: RoutePlanRequest, revision: Int): RoutePreviewModel = withContext(Dispatchers.IO) {
        val coordinates = String.format(
            Locale.US,
            "%.6f,%.6f;%.6f,%.6f",
            request.origin.longitude,
            request.origin.latitude,
            request.destination.longitude,
            request.destination.latitude,
        )
        val url = URL("https://router.project-osrm.org/route/v1/bike/$coordinates?alternatives=3&overview=full&steps=true&geometries=polyline")
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
        if (statusCode !in 200..299) {
            throw IllegalStateException("HTTP $statusCode: $body")
        }

        val root = JSONObject(body)
        val code = root.optString("code")
        if (code != "Ok") {
            throw IllegalStateException(root.optString("message", "OSRM returned $code"))
        }
        val routesJson = root.optJSONArray("routes") ?: JSONArray()
        if (routesJson.length() == 0) {
            throw IllegalStateException("No live OSM alternatives were returned")
        }

        val alternatives = buildList {
            for (index in 0 until minOf(routesJson.length(), 3)) {
                mapLiveAlternative(routesJson.getJSONObject(index), request, revision, index)?.let(::add)
            }
        }
        if (alternatives.isEmpty()) {
            throw IllegalStateException("No usable live OSM alternatives were returned")
        }

        RoutePreviewModel(
            alternatives = alternatives,
            selectedAlternativeId = alternatives.firstOrNull()?.id,
            routeIdentifier = alternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
            routeRevision = alternatives.firstOrNull()?.normalizedPackage?.revision,
            planningNotice = "Live OSM bike routing via OSRM demo server",
        )
    }

    private fun mapLiveAlternative(
        routeJson: JSONObject,
        request: RoutePlanRequest,
        revision: Int,
        alternativeIndex: Int,
    ): RouteAlternative? {
        val geometry = decodePolyline(routeJson.optString("geometry"))
        if (geometry.size < 2) return null
        val distance = routeJson.optDouble("distance", 0.0)
        val duration = routeJson.optDouble("duration", 0.0)
        val summary = RouteSummary(
            totalDistanceMeters = distance,
            estimatedDurationSeconds = max(duration.toInt(), 60),
            startLabel = "Current location",
            destinationLabel = "Selected destination",
        )
        val routePackage = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = buildLiveRouteIdentifier(request, alternativeIndex),
            revision = revision,
            geometry = geometry,
            maneuvers = buildLiveManeuvers(routeJson, geometry),
            summary = summary,
            provenance = RouteProvenance(
                providerId = RouteProviderId.OSM,
                sourceReference = "OSRM bike route",
                generatedAtUnixMs = System.currentTimeMillis(),
            ),
        )
        val subtitle = routeJson.optJSONArray("legs")
            ?.optJSONObject(0)
            ?.optString("summary")
            ?.takeIf { it.isNotBlank() }
            ?: "OSRM bike route"
        return RouteAlternative(
            id = UUID.randomUUID().toString(),
            title = if (alternativeIndex == 0) "OSRM primary route" else "OSRM alternative route",
            subtitle = subtitle,
            distanceMeters = distance.toInt(),
            durationSeconds = summary.estimatedDurationSeconds,
            normalizedPackage = routePackage,
        )
    }

    private fun buildLiveManeuvers(routeJson: JSONObject, geometry: List<CoordinatePoint>): List<RouteManeuver> {
        val legsJson = routeJson.optJSONArray("legs") ?: JSONArray()
        val firstNextDistance = firstLiveStepDistance(legsJson)
        val maneuvers = mutableListOf<RouteManeuver>()
        maneuvers += RouteManeuver(
            id = "depart",
            maneuverType = RouteManeuverType.DEPART,
            location = geometry.firstOrNull() ?: CoordinatePoint(0.0, 0.0),
            distanceFromStartMeters = 0.0,
            distanceToNextMeters = firstNextDistance,
            instructionText = "Start riding",
        )

        var distanceFromStart = 0.0
        for (legIndex in 0 until legsJson.length()) {
            val stepsJson = legsJson.optJSONObject(legIndex)?.optJSONArray("steps") ?: continue
            for (stepIndex in 0 until stepsJson.length()) {
                val stepJson = stepsJson.getJSONObject(stepIndex)
                val distance = stepJson.optDouble("distance", 0.0)
                val maneuverJson = stepJson.optJSONObject("maneuver")
                if (maneuverJson == null) {
                    distanceFromStart += distance
                    continue
                }
                val type = maneuverJson.optString("type").lowercase(Locale.ROOT)
                if (type == "depart" || type == "arrive" || type == "notification" || type == "new name" || type == "continue") {
                    distanceFromStart += distance
                    continue
                }
                val locationJson = maneuverJson.optJSONArray("location")
                if (locationJson == null || locationJson.length() < 2) {
                    distanceFromStart += distance
                    continue
                }
                val maneuverLocation = CoordinatePoint(
                    latitude = locationJson.optDouble(1),
                    longitude = locationJson.optDouble(0),
                )
                val name = stepJson.optString("name")
                maneuvers += RouteManeuver(
                    id = if (name.isBlank()) "step-${maneuvers.size}" else "step-${maneuvers.size}-$name",
                    maneuverType = maneuverType(type, maneuverJson.optString("modifier")),
                    location = maneuverLocation,
                    distanceFromStartMeters = distanceFromStart,
                    distanceToNextMeters = distance.takeIf { it > 0.0 },
                    instructionText = instructionText(type, maneuverJson.optString("modifier"), name),
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

    private fun firstLiveStepDistance(legsJson: JSONArray): Double? {
        for (legIndex in 0 until legsJson.length()) {
            val stepsJson = legsJson.optJSONObject(legIndex)?.optJSONArray("steps") ?: continue
            for (stepIndex in 0 until stepsJson.length()) {
                val stepJson = stepsJson.getJSONObject(stepIndex)
                val maneuverType = stepJson.optJSONObject("maneuver")?.optString("type")?.lowercase(Locale.ROOT) ?: continue
                if (maneuverType != "depart") {
                    return stepJson.optDouble("distance").takeIf { it > 0.0 }
                }
            }
        }
        return null
    }

    private fun maneuverType(type: String, modifier: String?): RouteManeuverType {
        return when (type) {
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
    }

    private fun instructionText(type: String, modifier: String?, name: String): String {
        return when (type) {
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
    }

    private fun buildLiveRouteIdentifier(request: RoutePlanRequest, alternativeIndex: Int): String {
        val origin = String.format(Locale.US, "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
        val destination = String.format(Locale.US, "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
        return "osm-live:$origin->$destination:alt-$alternativeIndex"
    }

    private fun buildPreview(request: RoutePlanRequest, revision: Int, planningNotice: String): RoutePreviewModel {
        val alternatives = listOf(0, 1, 2).map { alternativeIndex ->
            buildAlternative(request, revision, alternativeIndex)
        }
        return RoutePreviewModel(
            alternatives = alternatives,
            selectedAlternativeId = alternatives.firstOrNull()?.id,
            routeIdentifier = alternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
            routeRevision = alternatives.firstOrNull()?.normalizedPackage?.revision,
            planningNotice = planningNotice,
        )
    }

    private fun buildAlternative(request: RoutePlanRequest, revision: Int, alternativeIndex: Int): RouteAlternative {
        val geometry = sampleGeometry(request.origin, request.destination, alternativeIndex)
        val maneuvers = buildManeuvers(geometry)
        val totalDistance = routeDistance(geometry)
        val summary = RouteSummary(
            totalDistanceMeters = totalDistance,
            estimatedDurationSeconds = max((totalDistance / providerAverageMetersPerSecond(providerId)).toInt(), 60),
            startLabel = "Current location",
            destinationLabel = "${providerId.displayName} sample destination",
        )
        val routePackage = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "${providerId.name.lowercase()}-sample-$alternativeIndex",
            revision = revision,
            geometry = geometry,
            maneuvers = maneuvers,
            summary = summary,
            provenance = RouteProvenance(
                providerId = providerId,
                sourceReference = "${providerId.displayName} sample adapter",
                generatedAtUnixMs = System.currentTimeMillis(),
            ),
        )
        return RouteAlternative(
            id = UUID.randomUUID().toString(),
            title = if (alternativeIndex == 0) "Sample primary route" else "Sample alternative route",
            subtitle = subtitle(providerId, alternativeIndex),
            distanceMeters = totalDistance.toInt(),
            durationSeconds = summary.estimatedDurationSeconds,
            normalizedPackage = routePackage,
        )
    }

    private fun sampleGeometry(origin: CoordinatePoint, destination: CoordinatePoint, alternativeIndex: Int): List<CoordinatePoint> {
        if (origin == destination) {
            return deduplicated(
                listOf(
                    origin,
                    CoordinatePoint(origin.latitude + 0.0016, origin.longitude + 0.0008),
                    CoordinatePoint(origin.latitude + 0.0025, origin.longitude + 0.0017),
                    CoordinatePoint(origin.latitude + 0.0021, origin.longitude - 0.0006),
                    CoordinatePoint(origin.latitude + 0.0009, origin.longitude - 0.0018),
                    origin,
                ),
            )
        }

        val latDelta = destination.latitude - origin.latitude
        val lonDelta = destination.longitude - origin.longitude
        val length = max(sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001)
        val perpendicularLat = -lonDelta / length
        val perpendicularLon = latDelta / length
        val baseOffset = providerOffset(providerId) * (0.55 + alternativeIndex * 0.18)
        val fractions = listOf(0.12, 0.24, 0.39, 0.56, 0.73, 0.88)
        val patterns = listOf(
            listOf(0.30, 0.58, 0.18, -0.08, 0.26, 0.05),
            listOf(-0.22, -0.46, -0.12, -0.34, -0.08, 0.03),
            listOf(0.18, 0.06, 0.42, 0.20, 0.34, 0.09),
        )
        val pattern = patterns[minOf(alternativeIndex, patterns.lastIndex)]

        val geometry = mutableListOf(origin)
        fractions.forEachIndexed { index, fraction ->
            val lateral = baseOffset * pattern[index]
            val forwardBias = baseOffset * 0.12 * (index - 2.5)
            geometry += CoordinatePoint(
                latitude = origin.latitude + latDelta * fraction + perpendicularLat * lateral + latDelta * forwardBias,
                longitude = origin.longitude + lonDelta * fraction + perpendicularLon * lateral + lonDelta * forwardBias,
            )
        }
        geometry += destination
        return deduplicated(geometry)
    }

    private fun buildManeuvers(geometry: List<CoordinatePoint>): List<RouteManeuver> {
        val cumulative = cumulativeDistances(geometry)
        val maneuvers = mutableListOf<RouteManeuver>()
        maneuvers += RouteManeuver(
            id = "depart",
            maneuverType = RouteManeuverType.DEPART,
            location = geometry.firstOrNull() ?: CoordinatePoint(0.0, 0.0),
            distanceFromStartMeters = 0.0,
            distanceToNextMeters = cumulative.drop(1).firstOrNull(),
            instructionText = "Start riding",
        )
        for (index in 1 until geometry.lastIndex) {
            val delta = turnDeltaDegrees(geometry[index - 1], geometry[index], geometry[index + 1])
            val maneuver = classifyTurn(delta) ?: continue
            val distanceToNext = if (index + 1 < cumulative.size) cumulative[index + 1] - cumulative[index] else null
            maneuvers += RouteManeuver(
                id = "step-$index",
                maneuverType = maneuver.first,
                location = geometry[index],
                distanceFromStartMeters = cumulative[index],
                distanceToNextMeters = distanceToNext,
                instructionText = maneuver.second,
            )
        }
        maneuvers += RouteManeuver(
            id = "arrive",
            maneuverType = RouteManeuverType.ARRIVE,
            location = geometry.lastOrNull() ?: CoordinatePoint(0.0, 0.0),
            distanceFromStartMeters = cumulative.lastOrNull() ?: 0.0,
            distanceToNextMeters = null,
            instructionText = "Arrive at destination",
        )
        return maneuvers
    }

    private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
        val cumulative = mutableListOf(0.0)
        geometry.zipWithNext().forEach { (start, end) ->
            cumulative += cumulative.last() + approximateDistanceMeters(start, end)
        }
        return cumulative
    }

    private fun routeDistance(geometry: List<CoordinatePoint>): Double {
        return geometry.zipWithNext().sumOf { (start, end) ->
            approximateDistanceMeters(start, end)
        }
    }

    private fun approximateDistanceMeters(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latMeters = (end.latitude - start.latitude) * 111_320.0
        val lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * PI / 180.0) * 111_320.0
        return sqrt(latMeters * latMeters + lonMeters * lonMeters)
    }

    private fun turnDeltaDegrees(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint): Double {
        val incoming = bearingDegrees(previous, current)
        val outgoing = bearingDegrees(current, next)
        var delta = outgoing - incoming
        while (delta <= -180.0) delta += 360.0
        while (delta > 180.0) delta -= 360.0
        return delta
    }

    private fun bearingDegrees(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latMeters = (end.latitude - start.latitude) * 111_320.0
        val lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * PI / 180.0) * 111_320.0
        return atan2(lonMeters, latMeters) * 180.0 / PI
    }

    private fun classifyTurn(deltaDegrees: Double): Pair<RouteManeuverType, String>? {
        val magnitude = abs(deltaDegrees)
        if (magnitude < 25.0) return null
        if (magnitude >= 170.0) return RouteManeuverType.UTURN to "Make a U-turn"
        if (magnitude >= 110.0) return if (deltaDegrees > 0) RouteManeuverType.SHARP_RIGHT to "Turn sharply right" else RouteManeuverType.SHARP_LEFT to "Turn sharply left"
        if (magnitude >= 50.0) return if (deltaDegrees > 0) RouteManeuverType.RIGHT to "Turn right" else RouteManeuverType.LEFT to "Turn left"
        return if (deltaDegrees > 0) RouteManeuverType.SLIGHT_RIGHT to "Bear right" else RouteManeuverType.SLIGHT_LEFT to "Bear left"
    }

    private fun deduplicated(points: List<CoordinatePoint>): List<CoordinatePoint> {
        val output = mutableListOf<CoordinatePoint>()
        points.forEach { point ->
            if (output.lastOrNull() != point) output += point
        }
        return output
    }

    private fun decodePolyline(encoded: String): List<CoordinatePoint> {
        if (encoded.isEmpty()) return emptyList()
        val coordinates = mutableListOf<CoordinatePoint>()
        var index = 0
        var latitude = 0
        var longitude = 0
        while (index < encoded.length) {
            var shift = 0
            var result = 0
            var value: Int
            do {
                value = encoded[index++].code - 63
                result = result or ((value and 0x1f) shl shift)
                shift += 5
            } while (value >= 0x20 && index < encoded.length)
            latitude += if ((result and 1) == 0) result shr 1 else (result shr 1).inv()

            shift = 0
            result = 0
            do {
                value = encoded[index++].code - 63
                result = result or ((value and 0x1f) shl shift)
                shift += 5
            } while (value >= 0x20 && index < encoded.length)
            longitude += if ((result and 1) == 0) result shr 1 else (result shr 1).inv()

            coordinates += CoordinatePoint(latitude / 100_000.0, longitude / 100_000.0)
        }
        return coordinates
    }

    private fun planningNotice(provider: RouteProviderId): String {
        return when (provider) {
            RouteProviderId.OSM -> "Using sample OSM fallback routes. Live OSRM bike routing is unavailable."
            RouteProviderId.GOOGLE_INGEST -> "Using sample Google ingest routes. Compliance and live ingestion are still pending."
            RouteProviderId.GPX_IMPORT -> "Using sample GPX import routes. File selection is not wired yet."
            RouteProviderId.FIT_IMPORT -> "Using sample FIT import routes. File selection is not wired yet."
            RouteProviderId.TCX_IMPORT -> "Using sample TCX import routes. File selection is not wired yet."
            RouteProviderId.GARMIN_API -> "Using sample Garmin API routes. Live Garmin integration is not wired yet."
            RouteProviderId.GARMIN_FILE -> "Using sample Garmin file routes. File selection is not wired yet."
            RouteProviderId.HSL -> "Using sample HSL route"
        }
    }

    private fun subtitle(provider: RouteProviderId, alternativeIndex: Int): String {
        val variant = if (alternativeIndex == 0) "primary" else "alternative"
        return "${provider.displayName} sample $variant"
    }

    private fun providerOffset(provider: RouteProviderId): Double {
        return when (provider) {
            RouteProviderId.OSM -> 0.0020
            RouteProviderId.GOOGLE_INGEST -> 0.0018
            RouteProviderId.GPX_IMPORT -> 0.0014
            RouteProviderId.FIT_IMPORT -> 0.0016
            RouteProviderId.TCX_IMPORT -> 0.0012
            RouteProviderId.GARMIN_API -> 0.0017
            RouteProviderId.GARMIN_FILE -> 0.0015
            RouteProviderId.HSL -> 0.0018
        }
    }

    private fun providerAverageMetersPerSecond(provider: RouteProviderId): Double {
        return when (provider) {
            RouteProviderId.OSM -> 5.2
            RouteProviderId.GOOGLE_INGEST -> 5.6
            RouteProviderId.GPX_IMPORT -> 4.8
            RouteProviderId.FIT_IMPORT -> 5.0
            RouteProviderId.TCX_IMPORT -> 4.7
            RouteProviderId.GARMIN_API -> 5.4
            RouteProviderId.GARMIN_FILE -> 4.9
            RouteProviderId.HSL -> 5.3
        }
    }
}
