package app.navon.bike.flows

import java.io.File
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.integration.cycling.BrouterProfile
import app.navon.bike.integration.cycling.mapBrouterToAlternative
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * L1 unit tests for the BRouter response parser.
 *
 * Uses the canonical Phase-0 fixture in
 * `parity-fixtures/data/cycling/brouter-fastbike-helsinki-kallio.json`
 * shared with web + iOS. Verifies the geometry, distance, duration, and
 * voicehints-derived maneuvers are parsed correctly. Network behavior is
 * not exercised here — that lives in the in-app integration smoke.
 */
class CyclingRoutingTest {

    private fun fixtureRoot(): File {
        // The test runs from companion-android/app; parity-fixtures is at repo-root/data/.
        return File(System.getProperty("user.dir") ?: ".")
            .parentFile
            ?.parentFile
            ?.parentFile
            ?.resolve("data/parity-fixtures/data/cycling")
            ?: error("data/parity-fixtures/data/cycling not found")
    }

    private fun loadFixture(name: String): JSONObject {
        val text = fixtureRoot().resolve(name).readText()
        return JSONObject(text)
    }

    @Test
    fun mapsBrouterFastbikeFixtureToAlternative() {
        val response = loadFixture("brouter-fastbike-helsinki-kallio.json")
        val feature = response.getJSONArray("features").getJSONObject(0)
        val request = RoutePlanRequest(
            origin = CoordinatePoint(60.1699, 24.9384),
            destination = CoordinatePoint(60.1854, 24.9522),
            providerId = RouteProviderId.OSM,
        )
        val alt = mapBrouterToAlternative(
            feature = feature,
            request = request,
            revision = 1,
            profile = BrouterProfile.FASTBIKE,
            title = "Bike-paths first",
        )
        assertNotNull("parser must produce an alternative for the fixture", alt)
        val pkg = alt!!.normalizedPackage
        assertEquals(RouteProviderId.OSM, pkg.provenance.providerId)
        assertTrue(
            "geometry must have many points (BRouter typically 100+ for a 3km route)",
            pkg.geometry.size > 50,
        )
        assertTrue(
            "distance must be roughly 2-4 km",
            pkg.summary.totalDistanceMeters in 1000.0..5000.0,
        )
        assertTrue(
            "duration must be at least 60s",
            pkg.summary.estimatedDurationSeconds >= 60,
        )
        // Maneuvers: depart + at least one turn + arrive.
        assertTrue(pkg.maneuvers.size >= 3)
        assertEquals(RouteManeuverType.DEPART, pkg.maneuvers.first().maneuverType)
        assertEquals(RouteManeuverType.ARRIVE, pkg.maneuvers.last().maneuverType)
        // voicehints in the fastbike fixture should produce at least one
        // turn that isn't depart/arrive/straight.
        assertTrue(
            "expected at least one non-trivial turn from voicehints",
            pkg.maneuvers.any {
                it.maneuverType !in setOf(
                    RouteManeuverType.DEPART,
                    RouteManeuverType.ARRIVE,
                    RouteManeuverType.STRAIGHT,
                )
            },
        )
    }

    @Test
    fun mapsBrouterTrekkingFixtureToAlternative() {
        val response = loadFixture("brouter-trekking-helsinki-kallio.json")
        val feature = response.getJSONArray("features").getJSONObject(0)
        val alt = mapBrouterToAlternative(
            feature = feature,
            request = RoutePlanRequest(
                origin = CoordinatePoint(60.1699, 24.9384),
                destination = CoordinatePoint(60.1854, 24.9522),
                providerId = RouteProviderId.OSM,
            ),
            revision = 1,
            profile = BrouterProfile.TREKKING,
            title = "Balanced cycling",
        )
        assertNotNull(alt)
        assertEquals("Balanced cycling", alt!!.title)
        assertTrue(alt.normalizedPackage.geometry.size > 50)
    }
}
