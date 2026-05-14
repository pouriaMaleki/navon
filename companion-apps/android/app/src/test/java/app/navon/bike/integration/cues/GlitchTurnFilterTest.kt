package app.navon.bike.integration.cues

import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import org.junit.Assert.assertEquals
import org.junit.Test

class GlitchTurnFilterTest {

    private val metersPerDegLat = 111_320.0

    private fun straightGeometry(lengthM: Double, stepM: Double = 5.0): List<CoordinatePoint> {
        val points = mutableListOf<CoordinatePoint>()
        var d = 0.0
        while (d <= lengthM) {
            points.add(CoordinatePoint(60.17 + d / metersPerDegLat, 24.94))
            d += stepM
        }
        return points
    }

    /** Geometry with a localized bend at [bendAtM] by [totalBendDeg] degrees. */
    private fun bentGeometry(
        lengthM: Double,
        bendAtM: Double,
        totalBendDeg: Double,
        stepM: Double = 5.0,
    ): List<CoordinatePoint> {
        val bendRad = totalBendDeg * Math.PI / 180.0
        val points = mutableListOf<CoordinatePoint>()
        var lat = 60.17
        var lon = 24.94
        points.add(CoordinatePoint(lat, lon))
        var d = stepM
        while (d <= lengthM) {
            val t = maxOf(0.0, minOf(1.0, (d - (bendAtM - 2.5)) / 5.0))
            val heading = bendRad * t
            val cosLat = kotlin.math.cos(lat * Math.PI / 180.0)
            lat += kotlin.math.cos(heading) * stepM / metersPerDegLat
            lon += kotlin.math.sin(heading) * stepM / (metersPerDegLat * cosLat)
            points.add(CoordinatePoint(lat, lon))
            d += stepM
        }
        return points
    }

    private fun m(
        id: String,
        type: RouteManeuverType,
        dist: Double,
        location: CoordinatePoint? = null,
    ): RouteManeuver {
        val loc = location ?: CoordinatePoint(60.17 + dist / metersPerDegLat, 24.94)
        return RouteManeuver(
            id = id,
            maneuverType = type,
            location = loc,
            distanceFromStartMeters = dist,
            distanceToNextMeters = null,
            instructionText = null,
        )
    }

    private fun ids(maneuvers: List<RouteManeuver>): List<String> = maneuvers.map { it.id }

    @Test
    fun empty_returnsUnchanged() {
        assertEquals(0, filterGlitchClusters(emptyList(), emptyList()).size)
    }

    @Test
    fun single_returnsUnchanged() {
        val maneuvers = listOf(m("m1", RouteManeuverType.LEFT, 100.0))
        val geom = straightGeometry(200.0)
        assertEquals(listOf("m1"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun insufficientGeometry_returnsUnchanged() {
        val maneuvers = listOf(m("m1", RouteManeuverType.LEFT, 100.0), m("m2", RouteManeuverType.LEFT, 107.0))
        val geom = listOf(CoordinatePoint(60.17, 24.94))
        assertEquals(listOf("m1", "m2"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun removesTwoCloseManeuversOnStraightPath() {
        val maneuvers = listOf(m("m1", RouteManeuverType.LEFT, 100.0), m("m2", RouteManeuverType.LEFT, 107.0))
        val geom = straightGeometry(200.0)
        assertEquals(0, filterGlitchClusters(maneuvers, geom).size)
    }

    @Test
    fun preservesTwoCloseManeuversWhenPathBends() {
        val geom = bentGeometry(200.0, 103.5, 12.0)
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0, location = pointAtDistance(geom, 100.0)),
            m("m2", RouteManeuverType.LEFT, 107.0, location = pointAtDistance(geom, 107.0)),
        )
        assertEquals(listOf("m1", "m2"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun removesThreeCloseManeuversOnStraightPath() {
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0),
            m("m2", RouteManeuverType.RIGHT, 105.0),
            m("m3", RouteManeuverType.LEFT, 112.0),
        )
        val geom = straightGeometry(200.0)
        assertEquals(0, filterGlitchClusters(maneuvers, geom).size)
    }

    @Test
    fun preservesThreeCloseManeuversOnCurvedPath() {
        val geom = bentGeometry(200.0, 108.5, 15.0)
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0, location = pointAtDistance(geom, 100.0)),
            m("m2", RouteManeuverType.RIGHT, 105.0, location = pointAtDistance(geom, 105.0)),
            m("m3", RouteManeuverType.LEFT, 112.0, location = pointAtDistance(geom, 112.0)),
        )
        assertEquals(listOf("m1", "m2", "m3"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun doesNotGroupManeuversMoreThan10mApart() {
        val maneuvers = listOf(m("m1", RouteManeuverType.LEFT, 100.0), m("m2", RouteManeuverType.LEFT, 115.0))
        val geom = straightGeometry(200.0)
        assertEquals(listOf("m1", "m2"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun twoSeparateClusters_handledIndependently() {
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0), m("m2", RouteManeuverType.RIGHT, 107.0),
            m("m3", RouteManeuverType.LEFT, 300.0), m("m4", RouteManeuverType.RIGHT, 306.0),
        )
        val geom = straightGeometry(500.0)
        assertEquals(0, filterGlitchClusters(maneuvers, geom).size)
    }

    @Test
    fun keepsNonClusteredManeuverAfterRemovedCluster() {
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0), m("m2", RouteManeuverType.RIGHT, 107.0),
            m("m3", RouteManeuverType.LEFT, 300.0),
        )
        val geom = straightGeometry(500.0)
        assertEquals(listOf("m3"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun preservesGlitchClusterThatTurnsSharplyEnough() {
        val geom = bentGeometry(200.0, 103.5, 10.5)
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0, location = pointAtDistance(geom, 100.0)),
            m("m2", RouteManeuverType.LEFT, 107.0, location = pointAtDistance(geom, 107.0)),
        )
        assertEquals(listOf("m1", "m2"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun clusterAtRouteStart() {
        val maneuvers = listOf(
            m("depart", RouteManeuverType.DEPART, 0.0),
            m("m1", RouteManeuverType.LEFT, 5.0),
        )
        val geom = straightGeometry(200.0)
        assertEquals(0, filterGlitchClusters(maneuvers, geom).size)
    }

    @Test
    fun clusterAtRouteEnd() {
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 180.0),
            m("m2", RouteManeuverType.RIGHT, 187.0),
            m("arrive", RouteManeuverType.ARRIVE, 200.0),
        )
        val geom = straightGeometry(200.0)
        assertEquals(listOf("arrive"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    @Test
    fun bendJustAboveThreshold_isPreserved() {
        val geom = bentGeometry(200.0, 103.5, 10.2)
        val maneuvers = listOf(
            m("m1", RouteManeuverType.LEFT, 100.0, location = pointAtDistance(geom, 100.0)),
            m("m2", RouteManeuverType.LEFT, 107.0, location = pointAtDistance(geom, 107.0)),
        )
        assertEquals(listOf("m1", "m2"), ids(filterGlitchClusters(maneuvers, geom)))
    }

    // ── helper ──────────────────────────────────────────────────────

    private fun pointAtDistance(geometry: List<CoordinatePoint>, targetDistM: Double): CoordinatePoint {
        val cum = cumulativeDistances(geometry)
        for (i in cum.indices) {
            if (cum[i] >= targetDistM - 1e-3) return geometry[i]
        }
        return geometry.last()
    }

    private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
        if (geometry.isEmpty()) return emptyList()
        val cum = mutableListOf(0.0)
        for (i in 1 until geometry.size) {
            cum.add(cum[i - 1] + haversineDistance(geometry[i - 1], geometry[i]))
        }
        return cum
    }

    private fun haversineDistance(a: CoordinatePoint, b: CoordinatePoint): Double {
        val dlat = (b.latitude - a.latitude) * metersPerDegLat
        val meanLat = ((a.latitude + b.latitude) / 2.0) * (Math.PI / 180.0)
        val dlon = (b.longitude - a.longitude) * kotlin.math.cos(meanLat) * metersPerDegLat
        return kotlin.math.sqrt(dlat * dlat + dlon * dlon)
    }
}
