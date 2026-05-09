package me.fiksu.esp32map.companion.integration.hsl

import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sqrt
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
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
import me.fiksu.esp32map.companion.domain.RerouteContext
import me.fiksu.esp32map.companion.domain.RouteSummary
import me.fiksu.esp32map.companion.domain.RoutingProvider
import org.json.JSONArray
import org.json.JSONObject

class HslRoutingAdapter(
    private val settingsProvider: () -> CompanionSettings = { CompanionSettings() },
) : RoutingProvider {
    companion object {
        private const val MIN_HEADING_SPEED_MPS = 2.0
        private const val REROUTE_FORWARD_SHIFT_M = 15.0
    }
    override val providerId: RouteProviderId = RouteProviderId.HSL
    override val isAvailableInV1: Boolean = true

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        return planPreview(request, revisionOverride = null)
    }

    override suspend fun replanRoute(
        session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?,
    ): RoutePreviewModel {
        val rerouteOrigin = headingBiasedOrigin(riderLocation, rerouteContext, "hsl")
        val rerouteRequest = RoutePlanRequest(
            origin = rerouteOrigin,
            destination = session.destinationCoordinate ?: riderLocation,
            providerId = session.providerId,
        )
        return planPreview(rerouteRequest, revisionOverride = session.routeRevision?.plus(1))
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

    override fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
        return preview.selectedAlternative?.normalizedPackage
            ?: error("No HSL alternative available for normalization")
    }

    private suspend fun planPreview(
        request: RoutePlanRequest,
        revisionOverride: Int?,
    ): RoutePreviewModel {
        val settings = settingsProvider()
        if (settings.preferLiveHslRouting) {
            val trimmedKey = settings.hslSubscriptionKey.trim()
            if (trimmedKey.isEmpty()) {
                return normalizeResponse(
                    sampleDigitransitResponse(request, "Fallback sample: missing HSL subscription key"),
                    request,
                    revisionOverride,
                    planningNotice = "No HSL subscription key configured. Showing sample route instead.",
                )
            }
            return try {
                val liveResponse = fetchLiveDigitransitResponse(request, settings)
                normalizeResponse(liveResponse, request, revisionOverride, planningNotice = "Live HSL Digitransit")
            } catch (error: Exception) {
                normalizeResponse(
                    sampleDigitransitResponse(request, "Fallback sample after live HSL failure"),
                    request,
                    revisionOverride,
                    planningNotice = "Live HSL failed: ${error.message ?: error::class.simpleName}. Showing sample route instead.",
                )
            }
        }

        return normalizeResponse(
            sampleDigitransitResponse(request, "Sample HSL route"),
            request,
            revisionOverride,
            planningNotice = "Using sample HSL routes. Enable live HSL in Settings.",
        )
    }

    fun makeGraphQlRequestBody(request: RoutePlanRequest): DigitransitGraphQlRequestBody {
        return DigitransitGraphQlRequestBody(
            query = ROUTE_PLAN_QUERY,
            variables = DigitransitGraphQlRequestBody.Variables(
                from = DigitransitGraphQlRequestBody.CoordinateVariable(request.origin.latitude, request.origin.longitude),
                to = DigitransitGraphQlRequestBody.CoordinateVariable(request.destination.latitude, request.destination.longitude),
                numItineraries = 3,
                transportModes = listOf(DigitransitGraphQlRequestBody.TransportMode("BICYCLE")),
                optimize = "SAFE",
            ),
        )
    }

    private fun fetchLiveDigitransitResponse(
        request: RoutePlanRequest,
        settings: CompanionSettings,
    ): DigitransitResponse {
        val url = URL(settings.hslEndpointUrl)
        val connection = (url.openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            doOutput = true
            setRequestProperty("content-type", "application/json")
            setRequestProperty("digitransit-subscription-key", settings.hslSubscriptionKey)
        }
        val requestBody = makeGraphQlRequestJson(request)
        connection.outputStream.use { output ->
            output.write(requestBody.toByteArray(Charsets.UTF_8))
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
        val errors = root.optJSONArray("errors")
        if (errors != null && errors.length() > 0) {
            val message = buildString {
                for (index in 0 until errors.length()) {
                    if (index > 0) append(" | ")
                    append(errors.getJSONObject(index).optString("message", "Unknown error"))
                }
            }
            throw IllegalStateException(message)
        }

        val itinerariesJson = root
            .optJSONObject("data")
            ?.optJSONObject("plan")
            ?.optJSONArray("itineraries")
            ?: throw IllegalStateException("No HSL route alternatives were returned")

        val itineraries = buildList {
            for (index in 0 until itinerariesJson.length()) {
                val itineraryJson = itinerariesJson.getJSONObject(index)
                add(parseLiveItinerary(itineraryJson, index))
            }
        }

        return DigitransitResponse(DigitransitData(DigitransitPlan(itineraries)))
    }

    private fun makeGraphQlRequestJson(request: RoutePlanRequest): String {
        val body = JSONObject()
        body.put("query", ROUTE_PLAN_QUERY)
        body.put(
            "variables",
            JSONObject().apply {
                put("from", JSONObject().put("lat", request.origin.latitude).put("lon", request.origin.longitude))
                put("to", JSONObject().put("lat", request.destination.latitude).put("lon", request.destination.longitude))
                put("numItineraries", 3)
                put("transportModes", JSONArray().put(JSONObject().put("mode", "BICYCLE")))
                put("optimize", "SAFE")
            },
        )
        return body.toString()
    }

    private fun parseLiveItinerary(json: JSONObject, index: Int): DigitransitItinerary {
        val legsJson = json.optJSONArray("legs") ?: JSONArray()
        val legs = buildList {
            for (legIndex in 0 until legsJson.length()) {
                parseLiveLeg(legsJson.getJSONObject(legIndex))?.let(::add)
            }
        }
        val firstLeg = legsJson.optJSONObject(0)
        val lastLeg = legsJson.optJSONObject(legsJson.length() - 1)
        return DigitransitItinerary(
            durationSeconds = json.optDouble("duration", 0.0).toInt(),
            systemNotice = if (index == 0) "HSL Digitransit live / fastest" else "HSL Digitransit live / alternative",
            legs = legs,
            steps = emptyList(),
            startLabel = firstLeg?.optJSONObject("from")?.optString("name").orEmpty().ifBlank { "Current location" },
            destinationLabel = lastLeg?.optJSONObject("to")?.optString("name").orEmpty().ifBlank { "Selected destination" },
        )
    }

    private fun parseLiveLeg(json: JSONObject): DigitransitLeg? {
        val geometry = decodePolyline(json.optJSONObject("legGeometry")?.optString("points").orEmpty())
        val fallback = buildList {
            json.optJSONObject("from")?.let {
                add(CoordinatePoint(it.optDouble("lat"), it.optDouble("lon")))
            }
            json.optJSONObject("to")?.let {
                val point = CoordinatePoint(it.optDouble("lat"), it.optDouble("lon"))
                if (lastOrNull() != point) add(point)
            }
        }
        val points = if (geometry.size >= 2) geometry else fallback
        if (points.size < 2) return null
        return DigitransitLeg(
            mode = json.optString("mode", "BICYCLE"),
            distanceMeters = json.optDouble("distance", 0.0),
            geometry = points,
        )
    }

    private fun normalizeResponse(
        response: DigitransitResponse,
        request: RoutePlanRequest,
        revisionOverride: Int?,
        planningNotice: String?,
    ): RoutePreviewModel {
        val cyclingSpeedKph = settingsProvider().cyclingSpeedKph
        val alternatives = response.data.plan.itineraries.mapIndexed { index, itinerary ->
            normalizeItinerary(itinerary, request, index, revisionOverride ?: 1, cyclingSpeedKph)
        }
        return RoutePreviewModel(
            alternatives = alternatives,
            selectedAlternativeId = alternatives.firstOrNull()?.id,
            routeIdentifier = alternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
            routeRevision = alternatives.firstOrNull()?.normalizedPackage?.revision,
            planningNotice = planningNotice,
        )
    }

    private fun normalizeItinerary(
        itinerary: DigitransitItinerary,
        request: RoutePlanRequest,
        alternativeIndex: Int,
        revision: Int,
        cyclingSpeedKph: Double,
    ): RouteAlternative {
        val routeId = buildRouteIdentifier(request, alternativeIndex)
        val geometry = deduplicatedGeometry(itinerary.legs)
        val maneuvers = buildManeuvers(itinerary, geometry)
        val totalDistance = itinerary.legs.sumOf { it.distanceMeters }
        // Digitransit's bike speed is conservative for actual riders; recompute
        // the ETA from the user-set cycling speed so listed times match
        // real-world riding.
        val durationSeconds = overrideDurationSeconds(
            totalDistanceMeters = totalDistance,
            cyclingSpeedKph = cyclingSpeedKph,
            fallbackSeconds = itinerary.durationSeconds,
        )
        val summary = RouteSummary(
            totalDistanceMeters = totalDistance,
            estimatedDurationSeconds = durationSeconds,
            startLabel = itinerary.startLabel,
            destinationLabel = itinerary.destinationLabel,
        )
        val routePackage = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = routeId,
            revision = revision,
            geometry = geometry,
            maneuvers = maneuvers,
            summary = summary,
            provenance = RouteProvenance(
                providerId = RouteProviderId.HSL,
                sourceReference = itinerary.systemNotice,
                generatedAtUnixMs = System.currentTimeMillis(),
            ),
        )
        return RouteAlternative(
            id = UUID.randomUUID().toString(),
            title = if (alternativeIndex == 0) "Fastest bike route" else "Alternative bike route",
            subtitle = itinerary.systemNotice,
            distanceMeters = totalDistance.toInt(),
            durationSeconds = durationSeconds,
            normalizedPackage = routePackage,
        )
    }

    private fun buildManeuvers(itinerary: DigitransitItinerary, geometry: List<CoordinatePoint>): List<RouteManeuver> {
        val routeDistance = itinerary.legs.sumOf { it.distanceMeters }
        val steps = if (itinerary.steps.isEmpty()) deriveSteps(geometry) else itinerary.steps
        val maneuvers = mutableListOf<RouteManeuver>()
        maneuvers += RouteManeuver(
            id = "depart",
            maneuverType = RouteManeuverType.DEPART,
            location = geometry.firstOrNull() ?: CoordinatePoint(0.0, 0.0),
            distanceFromStartMeters = 0.0,
            distanceToNextMeters = steps.firstOrNull()?.distanceFromStartMeters,
            instructionText = "Start riding",
        )
        steps.forEachIndexed { index, step ->
            maneuvers += RouteManeuver(
                id = "step-$index",
                maneuverType = maneuverType(step.relativeDirection),
                location = step.location,
                distanceFromStartMeters = step.distanceFromStartMeters,
                distanceToNextMeters = step.distanceToNextMeters,
                instructionText = step.instruction,
            )
        }
        maneuvers += RouteManeuver(
            id = "arrive",
            maneuverType = RouteManeuverType.ARRIVE,
            location = geometry.lastOrNull() ?: CoordinatePoint(0.0, 0.0),
            distanceFromStartMeters = routeDistance,
            distanceToNextMeters = null,
            instructionText = "Arrive at destination",
        )
        return maneuvers
    }

    private fun deriveSteps(geometry: List<CoordinatePoint>): List<DigitransitStep> {
        if (geometry.size < 3) return emptyList()
        val cumulative = cumulativeDistances(geometry)
        return buildList {
            for (index in 1 until geometry.lastIndex) {
                val delta = turnDeltaDegrees(geometry[index - 1], geometry[index], geometry[index + 1])
                val classification = classifyTurn(delta) ?: continue
                val distanceToNext = if (index + 1 < cumulative.size) cumulative[index + 1] - cumulative[index] else null
                add(
                    DigitransitStep(
                        relativeDirection = classification.first,
                        location = geometry[index],
                        distanceFromStartMeters = cumulative[index],
                        distanceToNextMeters = distanceToNext,
                        instruction = classification.second,
                    ),
                )
            }
        }
    }

    private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
        val cumulative = mutableListOf(0.0)
        geometry.zipWithNext().forEach { (start, end) ->
            cumulative += cumulative.last() + approximateDistanceMeters(start, end)
        }
        return cumulative
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
        val latScale = 111_320.0
        val lonScale = cos(((start.latitude + end.latitude) / 2) * PI / 180.0) * 111_320.0
        val latMeters = (end.latitude - start.latitude) * latScale
        val lonMeters = (end.longitude - start.longitude) * lonScale
        return atan2(lonMeters, latMeters) * 180.0 / PI
    }

    private fun classifyTurn(deltaDegrees: Double): Pair<String, String>? {
        val magnitude = kotlin.math.abs(deltaDegrees)
        if (magnitude < 25.0) return null
        if (magnitude >= 170.0) return if (deltaDegrees > 0) "UTURN_RIGHT" to "Make a U-turn" else "UTURN_LEFT" to "Make a U-turn"
        if (magnitude >= 110.0) return if (deltaDegrees > 0) "HARD_RIGHT" to "Turn sharply right" else "HARD_LEFT" to "Turn sharply left"
        if (magnitude >= 50.0) return if (deltaDegrees > 0) "RIGHT" to "Turn right" else "LEFT" to "Turn left"
        return if (deltaDegrees > 0) "SLIGHTLY_RIGHT" to "Bear right" else "SLIGHTLY_LEFT" to "Bear left"
    }

    private fun deduplicatedGeometry(legs: List<DigitransitLeg>): List<CoordinatePoint> {
        val points = mutableListOf<CoordinatePoint>()
        legs.flatMap { it.geometry }.forEach { point ->
            if (points.lastOrNull() != point) {
                points += point
            }
        }
        return points
    }

    private fun buildRouteIdentifier(request: RoutePlanRequest, alternativeIndex: Int): String {
        val origin = "%.5f,%.5f".format(request.origin.latitude, request.origin.longitude)
        val destination = "%.5f,%.5f".format(request.destination.latitude, request.destination.longitude)
        return "hsl:$origin->$destination:alt-$alternativeIndex"
    }

    private fun maneuverType(relativeDirection: String): RouteManeuverType {
        return when (relativeDirection.uppercase()) {
            "CONTINUE" -> RouteManeuverType.STRAIGHT
            "SLIGHTLY_LEFT" -> RouteManeuverType.SLIGHT_LEFT
            "LEFT" -> RouteManeuverType.LEFT
            "HARD_LEFT" -> RouteManeuverType.SHARP_LEFT
            "SLIGHTLY_RIGHT" -> RouteManeuverType.SLIGHT_RIGHT
            "RIGHT" -> RouteManeuverType.RIGHT
            "HARD_RIGHT" -> RouteManeuverType.SHARP_RIGHT
            "UTURN_LEFT", "UTURN_RIGHT", "UTURN" -> RouteManeuverType.UTURN
            "CIRCLE_COUNTERCLOCKWISE", "CIRCLE_CLOCKWISE" -> RouteManeuverType.ROUNDABOUT
            "ELEVATOR", "TRANSFER" -> RouteManeuverType.RAMP
            else -> RouteManeuverType.STRAIGHT
        }
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

    fun sampleDigitransitResponse(request: RoutePlanRequest, descriptor: String): DigitransitResponse {
        val origin = request.origin
        val destination = request.destination
        val itineraries = listOf(
            "fastest" to sampleGeometry(origin, destination, 0, 0.0013),
            "quieter" to sampleGeometry(origin, destination, 1, 0.0016),
            "simpler" to sampleGeometry(origin, destination, 2, 0.0010),
        ).map { (label, geometry) ->
            makeItinerary("$descriptor / $label", geometry, "Current location", "Selected destination")
        }
        return DigitransitResponse(
            data = DigitransitData(
                plan = DigitransitPlan(itineraries = itineraries),
            ),
        )
    }

    private fun makeItinerary(
        systemNotice: String,
        geometry: List<CoordinatePoint>,
        startLabel: String,
        destinationLabel: String,
    ): DigitransitItinerary {
        val segmentDistances = geometry.zipWithNext().map { (start, end) ->
            approximateDistanceMeters(start, end)
        }
        val totalDistance = segmentDistances.sum()
        return DigitransitItinerary(
            durationSeconds = (totalDistance / 4.2).toInt(),
            systemNotice = systemNotice,
            legs = listOf(DigitransitLeg(mode = "BICYCLE", distanceMeters = totalDistance, geometry = geometry)),
            steps = emptyList(),
            startLabel = startLabel,
            destinationLabel = destinationLabel,
        )
    }

    private fun sampleGeometry(origin: CoordinatePoint, destination: CoordinatePoint, alternativeIndex: Int, offsetScale: Double): List<CoordinatePoint> {
        if (origin == destination) {
            return listOf(
                origin,
                CoordinatePoint(origin.latitude + 0.0015, origin.longitude + 0.0009),
                CoordinatePoint(origin.latitude + 0.0024, origin.longitude + 0.0016),
                CoordinatePoint(origin.latitude + 0.0019, origin.longitude - 0.0004),
                CoordinatePoint(origin.latitude + 0.0008, origin.longitude - 0.0016),
                origin,
            )
        }

        val latDelta = destination.latitude - origin.latitude
        val lonDelta = destination.longitude - origin.longitude
        val length = max(sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001)
        val perpendicularLat = -lonDelta / length
        val perpendicularLon = latDelta / length
        val fractions = listOf(0.10, 0.22, 0.38, 0.54, 0.72, 0.88)
        val patterns = listOf(
            listOf(0.26, 0.52, 0.22, 0.00, 0.16, 0.04),
            listOf(-0.20, -0.42, -0.18, -0.28, -0.10, 0.02),
            listOf(0.12, 0.06, 0.28, 0.14, 0.24, 0.08),
        )
        val pattern = patterns[minOf(alternativeIndex, patterns.lastIndex)]

        val geometry = mutableListOf(origin)
        fractions.forEachIndexed { index, fraction ->
            val lateral = offsetScale * pattern[index]
            val forwardBias = offsetScale * 0.10 * (index - 2.5)
            geometry += CoordinatePoint(
                latitude = origin.latitude + latDelta * fraction + perpendicularLat * lateral + latDelta * forwardBias,
                longitude = origin.longitude + lonDelta * fraction + perpendicularLon * lateral + lonDelta * forwardBias,
            )
        }
        geometry += destination
        return geometry
    }

    private fun approximateDistanceMeters(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latScale = 111_320.0
        val lonScale = cos(((start.latitude + end.latitude) / 2) * PI / 180.0) * 111_320.0
        val latMeters = (end.latitude - start.latitude) * latScale
        val lonMeters = (end.longitude - start.longitude) * lonScale
        return sqrt(latMeters * latMeters + lonMeters * lonMeters)
    }

    data class DigitransitGraphQlRequestBody(
        val query: String,
        val variables: Variables,
    ) {
        data class Variables(
            val from: CoordinateVariable,
            val to: CoordinateVariable,
            val numItineraries: Int,
            val transportModes: List<TransportMode>,
            val optimize: String,
        )

        data class CoordinateVariable(
            val lat: Double,
            val lon: Double,
        )

        data class TransportMode(
            val mode: String,
        )
    }

    data class DigitransitResponse(
        val data: DigitransitData,
    )

    data class DigitransitData(
        val plan: DigitransitPlan,
    )

    data class DigitransitPlan(
        val itineraries: List<DigitransitItinerary>,
    )

    data class DigitransitItinerary(
        val durationSeconds: Int,
        val systemNotice: String,
        val legs: List<DigitransitLeg>,
        val steps: List<DigitransitStep>,
        val startLabel: String,
        val destinationLabel: String,
    )

    data class DigitransitLeg(
        val mode: String,
        val distanceMeters: Double,
        val geometry: List<CoordinatePoint>,
    )

    data class DigitransitStep(
        val relativeDirection: String,
        val location: CoordinatePoint,
        val distanceFromStartMeters: Double,
        val distanceToNextMeters: Double?,
        val instruction: String,
    )

    companion object {
        fun overrideDurationSeconds(
            totalDistanceMeters: Double,
            cyclingSpeedKph: Double,
            fallbackSeconds: Int,
        ): Int {
            if (!cyclingSpeedKph.isFinite() || cyclingSpeedKph <= 0.0) return fallbackSeconds
            val mps = cyclingSpeedKph / 3.6
            return kotlin.math.max(1L, kotlin.math.round(totalDistanceMeters / mps).toLong()).toInt()
        }

        val ROUTE_PLAN_QUERY = """
            query RoutePlan(${'$'}from: InputCoordinates!, ${'$'}to: InputCoordinates!, ${'$'}numItineraries: Int!, ${'$'}transportModes: [TransportMode!]!, ${'$'}optimize: OptimizeType!) {
              plan(
                from: ${'$'}from,
                to: ${'$'}to,
                numItineraries: ${'$'}numItineraries,
                transportModes: ${'$'}transportModes,
                optimize: ${'$'}optimize
              ) {
                itineraries {
                  duration
                  legs {
                    mode
                    distance
                    from {
                      lat
                      lon
                      name
                    }
                    to {
                      lat
                      lon
                      name
                    }
                    legGeometry {
                      points
                    }
                  }
                }
              }
            }
        """.trimIndent()
    }
}
