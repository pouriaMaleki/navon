package app.navon.bike.integration.gpx

import java.io.ByteArrayInputStream
import java.util.UUID
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sqrt
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
import org.xmlpull.v1.XmlPullParser
import org.xmlpull.v1.XmlPullParserFactory

class GpxRoutingAdapter : RoutingProvider {
    override val providerId: RouteProviderId = RouteProviderId.GPX_IMPORT
    override val isAvailableInV1: Boolean = true

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        throw IllegalStateException("Select a GPX file instead of using coordinate planning.")
    }

    override suspend fun replanRoute(
        _session: ActiveRouteSession,
        _riderLocation: CoordinatePoint,
        _rerouteContext: RerouteContext?,
    ): RoutePreviewModel {
        println("[reroute_heading] provider=gpxImport reason=provider_noop")
        throw IllegalStateException("Reroute is not supported for imported GPX routes yet.")
    }

    override fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
        return preview.selectedAlternative?.normalizedPackage
            ?: error("No imported GPX route is available")
    }

    fun importBytes(fileName: String, data: ByteArray, revision: Int = 1): RoutePreviewModel {
        val parsed = parseGpx(data)
        val routeName = parsed.routeName ?: fileName.removeSuffix(".gpx")
        val geometry = parsed.points.map { it.point }
        val cumulative = cumulativeDistances(geometry)
        val totalDistance = cumulative.lastOrNull() ?: 0.0
        val routePackage = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = slugify(routeName),
            revision = revision,
            geometry = geometry,
            maneuvers = buildManeuvers(parsed.points, cumulative, parsed.preferPointLabels),
            summary = RouteSummary(
                totalDistanceMeters = totalDistance,
                estimatedDurationSeconds = max((totalDistance / 5.0).toInt(), 60),
                startLabel = parsed.points.firstOrNull()?.label,
                destinationLabel = parsed.points.lastOrNull()?.label ?: routeName,
            ),
            provenance = RouteProvenance(
                providerId = RouteProviderId.GPX_IMPORT,
                sourceReference = fileName,
                generatedAtUnixMs = System.currentTimeMillis(),
            ),
        )
        val alternative = RouteAlternative(
            id = UUID.randomUUID().toString(),
            title = routeName,
            subtitle = "Imported GPX route",
            distanceMeters = totalDistance.toInt(),
            durationSeconds = routePackage.summary.estimatedDurationSeconds,
            normalizedPackage = routePackage,
        )
        return RoutePreviewModel(
            alternatives = listOf(alternative),
            selectedAlternativeId = alternative.id,
            routeIdentifier = routePackage.routeIdentifier,
            routeRevision = routePackage.revision,
            planningNotice = "Imported $fileName",
        )
    }

    private fun buildManeuvers(
        points: List<GpxPoint>,
        cumulative: List<Double>,
        preferPointLabels: Boolean,
    ): List<RouteManeuver> {
        if (points.size < 2) return emptyList()
        val maneuvers = mutableListOf<RouteManeuver>()
        maneuvers += RouteManeuver(
            id = "depart",
            maneuverType = RouteManeuverType.DEPART,
            location = points.first().point,
            distanceFromStartMeters = 0.0,
            distanceToNextMeters = cumulative.drop(1).firstOrNull(),
            instructionText = "Start riding",
        )
        for (index in 1 until points.lastIndex) {
            val pointLabel = points[index].label
            val turn = classifyTurn(points[index - 1].point, points[index].point, points[index + 1].point)
            if (turn == null && !(preferPointLabels && pointLabel != null)) continue
            val distanceToNext = if (index + 1 < cumulative.size) cumulative[index + 1] - cumulative[index] else null
            maneuvers += RouteManeuver(
                id = "step-$index",
                maneuverType = turn?.first ?: RouteManeuverType.STRAIGHT,
                location = points[index].point,
                distanceFromStartMeters = cumulative[index],
                distanceToNextMeters = distanceToNext,
                instructionText = pointLabel ?: turn?.second,
            )
        }
        maneuvers += RouteManeuver(
            id = "arrive",
            maneuverType = RouteManeuverType.ARRIVE,
            location = points.last().point,
            distanceFromStartMeters = cumulative.lastOrNull() ?: 0.0,
            distanceToNextMeters = null,
            instructionText = "Arrive at destination",
        )
        return maneuvers
    }

    private fun parseGpx(data: ByteArray): ParsedGpxRoute {
        val parser = XmlPullParserFactory.newInstance().newPullParser().apply {
            setInput(ByteArrayInputStream(data), Charsets.UTF_8.name())
        }

        val routePoints = mutableListOf<GpxPoint>()
        val trackPoints = mutableListOf<GpxPoint>()
        var metadataName: String? = null
        var routeName: String? = null
        var trackName: String? = null
        var currentPoint: GpxPoint? = null
        var currentPointKind: String? = null
        var currentTextTarget: TextTarget? = null
        var inMetadata = false
        var inRoute = false
        var inTrack = false
        var pointName: String? = null
        var pointDesc: String? = null
        var pointComment: String? = null

        while (parser.eventType != XmlPullParser.END_DOCUMENT) {
            when (parser.eventType) {
                XmlPullParser.START_TAG -> when (parser.name) {
                    "metadata" -> inMetadata = true
                    "rte" -> inRoute = true
                    "trk" -> inTrack = true
                    "rtept", "trkpt" -> {
                        val lat = parser.getAttributeValue(null, "lat")?.toDoubleOrNull()
                        val lon = parser.getAttributeValue(null, "lon")?.toDoubleOrNull()
                        if (lat != null && lon != null) {
                            currentPoint = GpxPoint(CoordinatePoint(lat, lon), null)
                            currentPointKind = parser.name
                            pointName = null
                            pointDesc = null
                            pointComment = null
                        }
                    }
                    "name" -> {
                        currentTextTarget = when {
                            currentPoint != null -> TextTarget.POINT_NAME
                            inMetadata -> TextTarget.METADATA_NAME
                            inRoute -> TextTarget.ROUTE_NAME
                            inTrack -> TextTarget.TRACK_NAME
                            else -> null
                        }
                    }
                    "desc" -> if (currentPoint != null) currentTextTarget = TextTarget.POINT_DESC
                    "cmt" -> if (currentPoint != null) currentTextTarget = TextTarget.POINT_COMMENT
                }
                XmlPullParser.TEXT -> {
                    val text = parser.text?.trim().orEmpty()
                    if (text.isNotEmpty()) {
                        when (currentTextTarget) {
                            TextTarget.METADATA_NAME -> metadataName = text
                            TextTarget.ROUTE_NAME -> routeName = text
                            TextTarget.TRACK_NAME -> trackName = text
                            TextTarget.POINT_NAME -> pointName = text
                            TextTarget.POINT_DESC -> pointDesc = text
                            TextTarget.POINT_COMMENT -> pointComment = text
                            null -> Unit
                        }
                    }
                }
                XmlPullParser.END_TAG -> when (parser.name) {
                    "metadata" -> inMetadata = false
                    "rte" -> inRoute = false
                    "trk" -> inTrack = false
                    "rtept", "trkpt" -> {
                        currentPoint?.let { point ->
                            val labelled = point.copy(label = pointName ?: pointDesc ?: pointComment)
                            if (currentPointKind == "rtept") routePoints += labelled else trackPoints += labelled
                        }
                        currentPoint = null
                        currentPointKind = null
                        pointName = null
                        pointDesc = null
                        pointComment = null
                    }
                }
            }
            if (parser.eventType == XmlPullParser.END_TAG || parser.eventType == XmlPullParser.TEXT) {
                currentTextTarget = null
            }
            parser.next()
        }

        val chosen = if (routePoints.size >= 2) dedupe(routePoints) else dedupe(trackPoints)
        require(chosen.size >= 2) { "GPX did not contain a usable route or track" }
        return ParsedGpxRoute(
            routeName = metadataName ?: routeName ?: trackName,
            points = chosen,
            preferPointLabels = routePoints.size >= 2,
        )
    }

    private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
        val cumulative = mutableListOf(0.0)
        geometry.zipWithNext().forEach { (start, end) ->
            cumulative += cumulative.last() + approximateDistanceMeters(start, end)
        }
        return cumulative
    }

    private fun dedupe(points: List<GpxPoint>): List<GpxPoint> {
        val output = mutableListOf<GpxPoint>()
        for (point in points) {
            if (output.lastOrNull()?.point != point.point) output += point
        }
        return output
    }

    private fun classifyTurn(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint): Pair<RouteManeuverType, String>? {
        val delta = turnDeltaDegrees(previous, current, next)
        val magnitude = abs(delta)
        if (magnitude < 25.0) return null
        if (magnitude >= 170.0) return RouteManeuverType.UTURN to "Make a U-turn"
        if (magnitude >= 110.0) return if (delta > 0) RouteManeuverType.SHARP_RIGHT to "Turn sharply right" else RouteManeuverType.SHARP_LEFT to "Turn sharply left"
        if (magnitude >= 50.0) return if (delta > 0) RouteManeuverType.RIGHT to "Turn right" else RouteManeuverType.LEFT to "Turn left"
        return if (delta > 0) RouteManeuverType.SLIGHT_RIGHT to "Slight right" else RouteManeuverType.SLIGHT_LEFT to "Slight left"
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

    private fun approximateDistanceMeters(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latMeters = (end.latitude - start.latitude) * 111_320.0
        val lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * PI / 180.0) * 111_320.0
        return sqrt(latMeters * latMeters + lonMeters * lonMeters)
    }

    private fun slugify(value: String): String {
        val output = StringBuilder()
        var previousDash = false
        for (character in value.lowercase()) {
            val next = if (character.isLetterOrDigit()) character else '-'
            if (next == '-') {
                if (!previousDash) {
                    output.append(next)
                    previousDash = true
                }
            } else {
                output.append(next)
                previousDash = false
            }
        }
        val trimmed = output.toString().trim('-')
        return if (trimmed.isBlank()) "gpx-import" else trimmed
    }

    private data class ParsedGpxRoute(
        val routeName: String?,
        val points: List<GpxPoint>,
        val preferPointLabels: Boolean,
    )

    private data class GpxPoint(
        val point: CoordinatePoint,
        val label: String?,
    )

    private enum class TextTarget {
        METADATA_NAME,
        ROUTE_NAME,
        TRACK_NAME,
        POINT_NAME,
        POINT_DESC,
        POINT_COMMENT,
    }
}
