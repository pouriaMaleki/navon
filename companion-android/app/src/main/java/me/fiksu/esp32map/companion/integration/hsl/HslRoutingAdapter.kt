package me.fiksu.esp32map.companion.integration.hsl

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sqrt
import java.util.UUID
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

class HslRoutingAdapter : RoutingProvider {
    override val providerId: RouteProviderId = RouteProviderId.HSL
    override val isAvailableInV1: Boolean = true

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        val response = sampleDigitransitResponse(request)
        return normalizeResponse(response, request)
    }

    override suspend fun replanRoute(session: ActiveRouteSession, riderLocation: CoordinatePoint): RoutePreviewModel {
        val rerouteRequest = RoutePlanRequest(
            origin = riderLocation,
            destination = session.destinationCoordinate ?: riderLocation,
            providerId = session.providerId,
        )
        val response = sampleDigitransitResponse(rerouteRequest)
        val preview = normalizeResponse(response, rerouteRequest)
        return if (session.routeRevision != null) {
            preview.copy(routeRevision = session.routeRevision + 1)
        } else {
            preview
        }
    }

    override fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
        return preview.selectedAlternative?.normalizedPackage
            ?: error("No HSL alternative available for normalization")
    }

    fun makeGraphQlRequestBody(request: RoutePlanRequest): DigitransitGraphQlRequestBody {
        return DigitransitGraphQlRequestBody(
            query = ROUTE_PLAN_QUERY,
            variables = DigitransitGraphQlRequestBody.Variables(
                from = DigitransitGraphQlRequestBody.CoordinateVariable(request.origin.latitude, request.origin.longitude),
                to = DigitransitGraphQlRequestBody.CoordinateVariable(request.destination.latitude, request.destination.longitude),
                numItineraries = 2,
                transportModes = listOf(DigitransitGraphQlRequestBody.TransportMode("BICYCLE")),
                optimize = "SAFE",
            ),
        )
    }

    private fun normalizeResponse(response: DigitransitResponse, request: RoutePlanRequest): RoutePreviewModel {
        val alternatives = response.data.plan.itineraries.mapIndexed { index, itinerary ->
            normalizeItinerary(itinerary, request, index)
        }
        return RoutePreviewModel(
            alternatives = alternatives,
            selectedAlternativeId = alternatives.firstOrNull()?.id,
            routeIdentifier = alternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
            routeRevision = alternatives.firstOrNull()?.normalizedPackage?.revision,
        )
    }

    private fun normalizeItinerary(
        itinerary: DigitransitItinerary,
        request: RoutePlanRequest,
        alternativeIndex: Int,
    ): RouteAlternative {
        val routeId = buildRouteIdentifier(request, alternativeIndex)
        val geometry = deduplicatedGeometry(itinerary.legs)
        val maneuvers = buildManeuvers(itinerary, geometry)
        val totalDistance = itinerary.legs.sumOf { it.distanceMeters }
        val summary = RouteSummary(
            totalDistanceMeters = totalDistance,
            estimatedDurationSeconds = itinerary.durationSeconds,
            startLabel = "Current location",
            destinationLabel = "Selected destination",
        )
        val routePackage = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = routeId,
            revision = 1,
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
            title = if (alternativeIndex == 0) "Fastest bike route" else "Quieter streets",
            subtitle = itinerary.systemNotice,
            distanceMeters = totalDistance.toInt(),
            durationSeconds = itinerary.durationSeconds,
            normalizedPackage = routePackage,
        )
    }

    private fun buildManeuvers(itinerary: DigitransitItinerary, geometry: List<CoordinatePoint>): List<RouteManeuver> {
        val routeDistance = itinerary.legs.sumOf { it.distanceMeters }
        val maneuvers = mutableListOf<RouteManeuver>()
        maneuvers += RouteManeuver(
            id = "depart",
            maneuverType = RouteManeuverType.DEPART,
            location = geometry.firstOrNull() ?: CoordinatePoint(0.0, 0.0),
            distanceFromStartMeters = 0.0,
            distanceToNextMeters = itinerary.steps.firstOrNull()?.distanceFromStartMeters,
            instructionText = "Start riding",
        )
        itinerary.steps.forEachIndexed { index, step ->
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

    fun sampleDigitransitResponse(request: RoutePlanRequest): DigitransitResponse {
        val origin = request.origin
        val destination = request.destination
        val midpointA = CoordinatePoint(
            latitude = (origin.latitude + destination.latitude) / 2 + 0.0020,
            longitude = (origin.longitude + destination.longitude) / 2 - 0.0012,
        )
        val midpointB = CoordinatePoint(
            latitude = (origin.latitude + destination.latitude) / 2 - 0.0010,
            longitude = (origin.longitude + destination.longitude) / 2 + 0.0015,
        )
        val midpointC = CoordinatePoint(
            latitude = origin.latitude + (destination.latitude - origin.latitude) * 0.75 + 0.0008,
            longitude = origin.longitude + (destination.longitude - origin.longitude) * 0.72 - 0.0010,
        )
        val fastestGeometry = listOf(origin, midpointA, midpointC, destination)
        val quieterGeometry = listOf(origin, midpointB, midpointC, destination)
        return DigitransitResponse(
            data = DigitransitData(
                plan = DigitransitPlan(
                    itineraries = listOf(
                        makeItinerary("HSL Digitransit bike / fastest", fastestGeometry, listOf("RIGHT", "LEFT")),
                        makeItinerary("HSL Digitransit bike / quieter", quieterGeometry, listOf("LEFT", "RIGHT")),
                    ),
                ),
            ),
        )
    }

    private fun makeItinerary(
        systemNotice: String,
        geometry: List<CoordinatePoint>,
        turnInstructions: List<String>,
    ): DigitransitItinerary {
        val segmentDistances = geometry.zipWithNext().map { (start, end) ->
            approximateDistanceMeters(start, end)
        }
        val totalDistance = segmentDistances.sum()
        var distanceFromStart = segmentDistances.firstOrNull() ?: 0.0
        val turnLocations = geometry.drop(1).dropLast(1)
        val steps = turnLocations.mapIndexed { index, point ->
            val distanceToNext = segmentDistances.getOrNull(index + 1)
            val step = DigitransitStep(
                relativeDirection = turnInstructions[index],
                location = point,
                distanceFromStartMeters = distanceFromStart,
                distanceToNextMeters = distanceToNext,
                instruction = if (turnInstructions[index] == "LEFT") "Turn left" else "Turn right",
            )
            distanceFromStart += segmentDistances.getOrNull(index + 1) ?: 0.0
            step
        }
        return DigitransitItinerary(
            durationSeconds = (totalDistance / 4.2).toInt(),
            systemNotice = systemNotice,
            legs = listOf(DigitransitLeg(mode = "BICYCLE", distanceMeters = totalDistance, geometry = geometry)),
            steps = steps,
        )
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
                  systemNotices
                  legs {
                    mode
                    distance
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
