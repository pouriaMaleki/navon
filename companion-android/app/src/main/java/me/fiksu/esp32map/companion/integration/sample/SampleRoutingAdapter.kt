package me.fiksu.esp32map.companion.integration.sample

import java.util.UUID
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sqrt
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

class SampleRoutingAdapter(
    override val providerId: RouteProviderId,
) : RoutingProvider {
    override val isAvailableInV1: Boolean = false

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        return buildPreview(request, revision = 1, planningNotice = planningNotice(providerId))
    }

    override suspend fun replanRoute(session: ActiveRouteSession, riderLocation: CoordinatePoint): RoutePreviewModel {
        val request = RoutePlanRequest(
            origin = riderLocation,
            destination = session.destinationCoordinate ?: riderLocation,
            providerId = providerId,
        )
        return buildPreview(request, revision = (session.routeRevision ?: 0) + 1, planningNotice = "Rerouted with ${providerId.displayName} sample adapter")
    }

    override fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
        return preview.selectedAlternative?.normalizedPackage ?: error("No sample route alternatives are available")
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
            return listOf(
                origin,
                CoordinatePoint(origin.latitude + 0.002, origin.longitude + 0.0015),
                CoordinatePoint(origin.latitude + 0.003, origin.longitude - 0.001),
                CoordinatePoint(origin.latitude + 0.001, origin.longitude - 0.002),
                origin,
            )
        }

        val latDelta = destination.latitude - origin.latitude
        val lonDelta = destination.longitude - origin.longitude
        val length = max(sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001)
        val perpendicularLat = -lonDelta / length
        val perpendicularLon = latDelta / length
        val baseOffset = providerOffset(providerId) * if (alternativeIndex == 0) 1.0 else -1.0

        val first = CoordinatePoint(
            latitude = origin.latitude + latDelta * 0.28 + perpendicularLat * baseOffset,
            longitude = origin.longitude + lonDelta * 0.28 + perpendicularLon * baseOffset,
        )
        val second = CoordinatePoint(
            latitude = origin.latitude + latDelta * 0.56 - perpendicularLat * baseOffset * 0.8,
            longitude = origin.longitude + lonDelta * 0.56 - perpendicularLon * baseOffset * 0.8,
        )
        val third = CoordinatePoint(
            latitude = origin.latitude + latDelta * 0.82 + perpendicularLat * baseOffset * 0.35,
            longitude = origin.longitude + lonDelta * 0.82 + perpendicularLon * baseOffset * 0.35,
        )

        return deduplicated(listOf(origin, first, second, third, destination))
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

    private fun planningNotice(provider: RouteProviderId): String {
        return when (provider) {
            RouteProviderId.OSM -> "Using sample OSM fallback routes. Live OSM routing is not wired yet."
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
