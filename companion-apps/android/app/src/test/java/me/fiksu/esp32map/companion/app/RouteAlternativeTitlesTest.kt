package me.fiksu.esp32map.companion.app

import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RoutePackageVersion
import me.fiksu.esp32map.companion.domain.RouteProvenance
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteSummary
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * iOS-parity rename of suggested-route titles. The rows used to read
 * "OSM Route 1 / via …" with a redundant subtitle. The new scheme drops
 * the per-provider counter and uses the underlying engine name:
 *
 *   - OSM via BRouter `fastbike` → "BRouter fastbike"
 *   - OSM via BRouter `trekking` → "BRouter trekking"
 *   - OSM via OSRM bike          → "OSM Route"
 *   - HSL Digitransit live / fastest     → "HSL Fastest" (no subtitle)
 *   - HSL Digitransit live / alternative → "HSL Route"
 */
class RouteAlternativeTitlesTest {

    private fun alt(provider: RouteProviderId, sourceReference: String?): RouteAlternative {
        val start = CoordinatePoint(60.17, 24.94)
        val end = CoordinatePoint(60.18, 24.95)
        val pkg = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "rid-${sourceReference ?: "anon"}",
            revision = 1,
            geometry = listOf(start, end),
            maneuvers = emptyList(),
            summary = RouteSummary(
                totalDistanceMeters = 1000.0,
                estimatedDurationSeconds = 300,
                startLabel = null,
                destinationLabel = "Park",
            ),
            provenance = RouteProvenance(
                providerId = provider,
                sourceReference = sourceReference,
                generatedAtUnixMs = 0,
            ),
        )
        return RouteAlternative(
            id = "id-${sourceReference ?: "anon"}",
            title = "x",
            subtitle = "y",
            distanceMeters = 1000,
            durationSeconds = 300,
            normalizedPackage = pkg,
        )
    }

    @Test
    fun brouterFastbike_titleIsBRouterFastbike_withoutSubtitle() {
        val (title, subtitle) = CompanionAppState.friendlyAlternativeLabel(
            alt(RouteProviderId.OSM, "BRouter fastbike"),
        )
        assertEquals("BRouter fastbike", title)
        assertEquals("", subtitle)
    }

    @Test
    fun brouterTrekking_titleIsBRouterTrekking() {
        val (title, _) = CompanionAppState.friendlyAlternativeLabel(
            alt(RouteProviderId.OSM, "BRouter trekking"),
        )
        assertEquals("BRouter trekking", title)
    }

    @Test
    fun osrmBike_titleIsOsmRoute() {
        val (title, _) = CompanionAppState.friendlyAlternativeLabel(
            alt(RouteProviderId.OSM, "OSRM bike"),
        )
        assertEquals("OSM Route", title)
    }

    @Test
    fun hslFastest_titleIsHslFastest_withoutSubtitle() {
        val (title, subtitle) = CompanionAppState.friendlyAlternativeLabel(
            alt(RouteProviderId.HSL, "HSL Digitransit live / fastest"),
        )
        assertEquals("HSL Fastest", title)
        assertEquals("", subtitle)
    }

    @Test
    fun hslAlternative_titleIsHslRoute() {
        val (title, _) = CompanionAppState.friendlyAlternativeLabel(
            alt(RouteProviderId.HSL, "HSL Digitransit live / alternative"),
        )
        assertEquals("HSL Route", title)
    }
}
