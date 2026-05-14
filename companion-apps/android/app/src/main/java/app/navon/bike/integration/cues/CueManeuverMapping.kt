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
 * When building [CueManeuver] objects from route maneuvers, the caller
 * should pass the route geometry to [promotedKind] so that slightLeft/
 * slightRight with a genuine fork-angle (>= this threshold) get promoted
 * to [ManeuverKind.BEAR_LEFT]/[ManeuverKind.BEAR_RIGHT].
 * Below this threshold the path just gently curves and the audio should
 * stay silent (via [isMinorKeepFor]). Matches web's classifyTurn() boundary.
 */
const val MINOR_KEEP_PROMOTION_ANGLE_DEG = 25.0

private const val METERS_PER_DEGREE_LAT = 111_320.0
private const val ANGLE_LOOK_DISTANCE_M = 10.0

/**
 * Single source of truth for "given a [RouteManeuverType], what audio cue
 * kind do we emit?" Returns `null` for maneuver types that produce no cue.
 *
 * Callers that build [CueManeuver] objects from route maneuvers should use
 * [promotedKind] instead — it additionally checks the route-geometry turn
 * angle and promotes slight forks to bear kinds.
 */
object CueManeuverMapping {
    fun kindFor(type: RouteManeuverType): ManeuverKind? = when (type) {
        RouteManeuverType.LEFT, RouteManeuverType.SHARP_LEFT -> ManeuverKind.LEFT
        RouteManeuverType.RIGHT, RouteManeuverType.SHARP_RIGHT -> ManeuverKind.RIGHT
        RouteManeuverType.SLIGHT_LEFT -> ManeuverKind.LEFT
        RouteManeuverType.SLIGHT_RIGHT -> ManeuverKind.RIGHT
        RouteManeuverType.UTURN -> ManeuverKind.UTURN
        RouteManeuverType.ROUNDABOUT -> ManeuverKind.ROUNDABOUT
        RouteManeuverType.MERGE -> ManeuverKind.MERGE
        RouteManeuverType.RAMP -> ManeuverKind.RAMP
        RouteManeuverType.STRAIGHT,
        RouteManeuverType.DEPART,
        RouteManeuverType.ARRIVE,
        -> null
    }

    fun isMinorKeepFor(type: RouteManeuverType): Boolean = when (type) {
        RouteManeuverType.SLIGHT_LEFT, RouteManeuverType.SLIGHT_RIGHT -> true
        else -> false
    }

    /**
     * Promote a slightLeft/slightRight to [ManeuverKind.BEAR_LEFT] /
     * [ManeuverKind.BEAR_RIGHT] when the actual route-geometry turn angle
     * at the maneuver location is >= [MINOR_KEEP_PROMOTION_ANGLE_DEG].
     * Below that threshold the returned kind is the base LEFT/RIGHT and
     * the caller should keep [isMinorKeepFor] = true (suppressed).
     *
     * @param type        the original [RouteManeuverType]
     * @param geometry    the full route polyline
     * @param cumDist     cumulative distances along [geometry] (index → metres)
     * @param maneuverLoc the coordinate of this maneuver
     * @return the [ManeuverKind] to use for the [CueManeuver]
     */
    fun promotedKind(
        type: RouteManeuverType,
        geometry: List<CoordinatePoint>,
        cumDist: List<Double>,
        maneuverLoc: CoordinatePoint,
    ): ManeuverKind? {
        val base = kindFor(type) ?: return null
        if (!isMinorKeepFor(type)) return base
        if (geometry.size < 3 || cumDist.size < 3) return base

        val idx = closestPointIndex(geometry, maneuverLoc)
        if (idx <= 0 || idx >= geometry.size - 1) return base

        val behind = walkAlongPolyline(geometry, cumDist, idx, ANGLE_LOOK_DISTANCE_M, "backward")
        val ahead = walkAlongPolyline(geometry, cumDist, idx, ANGLE_LOOK_DISTANCE_M, "forward")
        val delta = turnDeltaDegrees(behind, geometry[idx], ahead)

        return if (abs(delta) >= MINOR_KEEP_PROMOTION_ANGLE_DEG) {
            if (base == ManeuverKind.LEFT) ManeuverKind.BEAR_LEFT else ManeuverKind.BEAR_RIGHT
        } else {
            base
        }
    }

    // ── geometry helpers ──────────────────────────────────────────

    private fun closestPointIndex(geom: List<CoordinatePoint>, target: CoordinatePoint): Int {
        var best = 0
        var bestDistSq = Double.MAX_VALUE
        for (i in geom.indices) {
            val dlat = geom[i].latitude - target.latitude
            val dlon = geom[i].longitude - target.longitude
            val distSq = dlat * dlat + dlon * dlon
            if (distSq < bestDistSq) { bestDistSq = distSq; best = i }
        }
        return best
    }

    private fun walkAlongPolyline(
        geom: List<CoordinatePoint>,
        cumDist: List<Double>,
        startIdx: Int,
        distanceM: Double,
        direction: String,
    ): CoordinatePoint {
        var remaining = distanceM
        var idx = startIdx
        while (remaining > 1e-6 && idx > 0 && idx < geom.size - 1) {
            val nextIdx = if (direction == "backward") idx - 1 else idx + 1
            if (nextIdx < 0 || nextIdx >= geom.size) break
            val segLen = if (direction == "backward") cumDist[idx] - cumDist[nextIdx]
                         else cumDist[nextIdx] - cumDist[idx]
            if (segLen <= 1e-9) { idx = nextIdx; continue }
            if (remaining >= segLen) {
                remaining -= segLen
                idx = nextIdx
            } else {
                val t = remaining / segLen
                val from = geom[idx]
                val to = geom[nextIdx]
                return CoordinatePoint(
                    latitude = from.latitude + (to.latitude - from.latitude) * t,
                    longitude = from.longitude + (to.longitude - from.longitude) * t,
                )
            }
        }
        return geom[idx]
    }

    private fun turnDeltaDegrees(
        previous: CoordinatePoint,
        current: CoordinatePoint,
        next: CoordinatePoint,
    ): Double {
        val incoming = bearingDegrees(previous, current)
        val outgoing = bearingDegrees(current, next)
        var delta = outgoing - incoming
        while (delta <= -180.0) delta += 360.0
        while (delta > 180.0) delta -= 360.0
        return delta
    }

    private fun bearingDegrees(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latM = (end.latitude - start.latitude) * METERS_PER_DEGREE_LAT
        val meanLat = ((start.latitude + end.latitude) / 2.0) * (Math.PI / 180.0)
        val lonM = (end.longitude - start.longitude) * cos(meanLat) * METERS_PER_DEGREE_LAT
        return Math.toDegrees(atan2(lonM, latM))
    }

    // ── maneuver collapse ──────────────────────────────────────────

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

    companion object {
        private const val COLLAPSE_DISTANCE_M = 5.0
        private const val COLLAPSE_ANGLE_DEG = 30.0
        private const val MANEUVER_LOOK_DIST_M = 10.0
    }

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
