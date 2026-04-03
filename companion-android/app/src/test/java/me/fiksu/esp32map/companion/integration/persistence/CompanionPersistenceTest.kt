package me.fiksu.esp32map.companion.integration.persistence

import org.junit.Assert.assertEquals
import org.junit.Test
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RouteProviderId

class CompanionPersistenceTest {
    @Test
    fun recentDestinationsAreStoredNewestFirstAndTrimmedToTen() {
        val persistence = CompanionPersistence()

        repeat(12) { index ->
            persistence.saveRecentDestination(CoordinatePoint(60.0 + index, 24.0 + index))
        }

        val recent = persistence.loadRecentDestinations()
        assertEquals(10, recent.size)
        assertEquals(CoordinatePoint(71.0, 35.0), recent.first())
        assertEquals(CoordinatePoint(62.0, 26.0), recent.last())
    }

    @Test
    fun sessionAndSettingsRoundTrip() {
        val persistence = CompanionPersistence()
        val session = ActiveRouteSession(
            routeIdentifier = "route-7",
            routeRevision = 3,
            destinationLabel = "Finish",
            providerId = RouteProviderId.GPX_IMPORT,
        )
        val settings = CompanionSettings(
            preferLiveHslRouting = true,
            hslSubscriptionKey = "local-only",
        )

        persistence.saveSession(session)
        persistence.saveSettings(settings)

        assertEquals(session, persistence.loadLastSession())
        assertEquals(settings, persistence.loadSettings())
    }
}
