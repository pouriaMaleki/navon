package app.navon.bike.integration.cues

import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RouteManeuver
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sqrt

private const val GLITCH_CLUSTER_MAX_GAP_M = 10.0
private const val GLITCH_ANGLE_THRESHOLD_DEG = 10.0
private const val GLITCH_ANGLE_LOOK_DISTANCE_M = 10.0
private const val METERS_PER_DEGREE_LAT = 111_320.0

/**
 * Remove clusters of consecutive maneuvers that are map glitches:
 * >=2 maneuvers each within 10m of the previous, where the net path
 * direction change from before the cluster to after it is < 10 degrees.
 */
fun filterGlitchClusters(
    maneuvers: List<RouteManeuver>,
    geometry: List<CoordinatePoint>,
): List<RouteManeuver> {
    if (maneuvers.size < 2 || geometry.size < 2) return maneuvers
    val cumDist = cumulativeDistances(geometry)
    if (cumDist.isEmpty()) return maneuvers

    val result = maneuvers.toMutableList()
    var i = 0
    while (i < result.size) {
        if (i + 1 >= result.size) break
        val gap = result[i + 1].distanceFromStartMeters - result[i].distanceFromStartMeters
        if (gap > GLITCH_CLUSTER_MAX_GAP_M) { i++; continue }

        var clusterEnd = i + 1
        while (clusterEnd + 1 < result.size) {
            val nextGap = result[clusterEnd + 1].distanceFromStartMeters - result[clusterEnd].distanceFromStartMeters
            if (nextGap > GLITCH_CLUSTER_MAX_GAP_M) break
            clusterEnd++
        }

        val clusterSize = clusterEnd - i + 1
        if (clusterSize < 2) { i = clusterEnd + 1; continue }

        val firstIdx = closestPointIndex(geometry, result[i].location)
        val lastIdx = closestPointIndex(geometry, result[clusterEnd].location)
        if (firstIdx >= 0 && firstIdx < geometry.size && lastIdx >= 0 && lastIdx < geometry.size) {
            val entryApproach = walkAlongPolyline(geometry, cumDist, firstIdx, GLITCH_ANGLE_LOOK_DISTANCE_M, "backward")
            val exitDepart = walkAlongPolyline(geometry, cumDist, lastIdx, GLITCH_ANGLE_LOOK_DISTANCE_M, "forward")
            val entryBearing = bearingDegrees(entryApproach, geometry[firstIdx])
            val exitBearing = bearingDegrees(geometry[lastIdx], exitDepart)
            var delta = abs(exitBearing - entryBearing)
            if (delta > 180.0) delta = 360.0 - delta

            if (delta < GLITCH_ANGLE_THRESHOLD_DEG) {
                for (j in clusterEnd downTo i) {
                    result.removeAt(j)
                }
                continue
            }
        }
        i = clusterEnd + 1
    }
    return result
}

// ── Private geometry helpers ──────────────────────────────────────

private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
    if (geometry.isEmpty()) return emptyList()
    val cum = mutableListOf(0.0)
    for (i in 1 until geometry.size) {
        cum.add(cum[i - 1] + haversineDistance(geometry[i - 1], geometry[i]))
    }
    return cum
}

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
            return CoordinatePoint(
                latitude = geom[idx].latitude + (geom[nextIdx].latitude - geom[idx].latitude) * t,
                longitude = geom[idx].longitude + (geom[nextIdx].longitude - geom[idx].longitude) * t,
            )
        }
    }
    return geom[idx]
}

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
