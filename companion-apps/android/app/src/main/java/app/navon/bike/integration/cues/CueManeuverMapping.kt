package app.navon.bike.integration.cues

import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sqrt

/**
 * Single source of truth for "given a [RouteManeuverType], what audio cue
 * kind do we emit?" Returns `null` for maneuver types that produce no cue.
 */
object CueManeuverMapping {
    fun kindFor(type: RouteManeuverType): ManeuverKind? = when (type) {
        RouteManeuverType.LEFT, RouteManeuverType.SHARP_LEFT -> ManeuverKind.LEFT
        RouteManeuverType.RIGHT, RouteManeuverType.SHARP_RIGHT -> ManeuverKind.RIGHT
        RouteManeuverType.SLIGHT_LEFT -> ManeuverKind.SLIGHT_LEFT
        RouteManeuverType.SLIGHT_RIGHT -> ManeuverKind.SLIGHT_RIGHT
        RouteManeuverType.UTURN -> ManeuverKind.UTURN
        RouteManeuverType.ROUNDABOUT -> ManeuverKind.ROUNDABOUT
        RouteManeuverType.MERGE -> ManeuverKind.MERGE
        RouteManeuverType.RAMP -> ManeuverKind.RAMP
        RouteManeuverType.STRAIGHT,
        RouteManeuverType.DEPART,
        RouteManeuverType.ARRIVE,
        -> null
    }

    // ── geometry helpers for collapseCloseManeuvers ───────────────

    private const val METERS_PER_DEGREE_LAT = 111_320.0

    private fun bearingDegrees(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latM = (end.latitude - start.latitude) * METERS_PER_DEGREE_LAT
        val meanLat = ((start.latitude + end.latitude) / 2.0) * (Math.PI / 180.0)
        val lonM = (end.longitude - start.longitude) * cos(meanLat) * METERS_PER_DEGREE_LAT
        return Math.toDegrees(atan2(lonM, latM))
    }

    private fun haversineDistance(a: CoordinatePoint, b: CoordinatePoint): Double {
        val dlat = (b.latitude - a.latitude) * METERS_PER_DEGREE_LAT
        val meanLat = ((a.latitude + b.latitude) / 2.0) * (Math.PI / 180.0)
        val dlon = (b.longitude - a.longitude) * cos(meanLat) * METERS_PER_DEGREE_LAT
        return sqrt(dlat * dlat + dlon * dlon)
    }

    private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
        if (geometry.size < 2) return if (geometry.isEmpty()) emptyList() else listOf(0.0)
        val cum = mutableListOf(0.0)
        for (i in 1 until geometry.size) {
            cum.add(cum[i - 1] + haversineDistance(geometry[i - 1], geometry[i]))
        }
        return cum
    }

    private fun coordAtDistance(
        geometry: List<CoordinatePoint>,
        dist: Double,
        cumul: List<Double>,
    ): CoordinatePoint {
        if (dist <= 0.0) return geometry[0]
        val total = cumul.last()
        if (dist >= total) return geometry.last()
        for (i in 1 until cumul.size) {
            if (cumul[i] >= dist) {
                val segLen = cumul[i] - cumul[i - 1]
                val t = if (segLen > 1e-9) (dist - cumul[i - 1]) / segLen else 0.0
                return CoordinatePoint(
                    latitude = geometry[i - 1].latitude + (geometry[i].latitude - geometry[i - 1].latitude) * t,
                    longitude = geometry[i - 1].longitude + (geometry[i].longitude - geometry[i - 1].longitude) * t,
                )
            }
        }
        return geometry.last()
    }

    private const val COLLAPSE_DISTANCE_M = 5.0
    private const val COLLAPSE_ANGLE_DEG = 30.0
    private const val MANEUVER_LOOK_DIST_M = 10.0

    /**
     * Collapse back-to-back maneuvers that are very close (<5m) when the net
     * direction change through them is >30°. Shared pedestrian path entries/exits
     * create multiple annotations but only the final real turn matters.
     */
    fun collapseCloseManeuvers(
        maneuvers: List<RouteManeuver>,
        geometry: List<CoordinatePoint>,
    ): List<RouteManeuver> {
        if (maneuvers.size < 2) return maneuvers.toList()
        if (geometry.size < 2) return maneuvers.toList()

        val cumul = cumulativeDistances(geometry)
        val totalDist = cumul.last()

        fun netAngleDeg(firstDist: Double, lastDist: Double): Double {
            val approachFrom = coordAtDistance(geometry, maxOf(0.0, firstDist - MANEUVER_LOOK_DIST_M), cumul)
            val approachPt = coordAtDistance(geometry, firstDist, cumul)
            val exitPt = coordAtDistance(geometry, lastDist, cumul)
            val exitTo = coordAtDistance(geometry, minOf(totalDist, lastDist + MANEUVER_LOOK_DIST_M), cumul)
            val inBearing = bearingDegrees(approachFrom, approachPt)
            val outBearing = bearingDegrees(exitPt, exitTo)
            var delta = outBearing - inBearing
            while (delta <= -180.0) delta += 360.0
            while (delta > 180.0) delta -= 360.0
            return abs(delta)
        }

        val result = mutableListOf<RouteManeuver>()
        var i = 0
        while (i < maneuvers.size) {
            var j = i + 1
            while (j < maneuvers.size &&
                maneuvers[j].distanceFromStartMeters - maneuvers[j - 1].distanceFromStartMeters < COLLAPSE_DISTANCE_M
            ) {
                j++
            }

            if (j - i > 1) {
                val firstDist = maneuvers[i].distanceFromStartMeters
                val lastDist = maneuvers[j - 1].distanceFromStartMeters
                if (netAngleDeg(firstDist, lastDist) > COLLAPSE_ANGLE_DEG) {
                    result.add(maneuvers[j - 1])
                } else {
                    for (k in i until j) result.add(maneuvers[k])
                }
            } else {
                result.add(maneuvers[i])
            }
            i = j
        }

        return result
    }
}
