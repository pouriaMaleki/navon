package me.fiksu.esp32map.companion.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.RoutePackageVersion
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.HomeMode
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RouteManeuver
import me.fiksu.esp32map.companion.domain.RouteManeuverType
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProvenance
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteSummary
import me.fiksu.esp32map.companion.fakes.FakeLocationService
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * L2 routing-session tests (plan flows #43, #44).
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class PlanningSessionTest {

    private fun straightLinePackage(): NormalizedRoutePackage {
        val origin = CoordinatePoint(60.1699, 24.9384)
        val destination = CoordinatePoint(60.1921, 24.9458)
        return NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "osm-straight",
            revision = 1,
            geometry = listOf(origin, destination),
            maneuvers = listOf(
                RouteManeuver(
                    id = "m1",
                    maneuverType = RouteManeuverType.DEPART,
                    location = origin,
                    distanceFromStartMeters = 0.0,
                    distanceToNextMeters = 2500.0,
                    instructionText = "Depart",
                ),
                RouteManeuver(
                    id = "m2",
                    maneuverType = RouteManeuverType.ARRIVE,
                    location = destination,
                    distanceFromStartMeters = 2500.0,
                    distanceToNextMeters = null,
                    instructionText = "Arrive",
                ),
            ),
            summary = RouteSummary(
                totalDistanceMeters = 2500.0,
                estimatedDurationSeconds = 600,
                startLabel = null,
                destinationLabel = null,
            ),
            provenance = RouteProvenance(
                providerId = RouteProviderId.OSM,
                sourceReference = null,
                generatedAtUnixMs = 0L,
            ),
        )
    }

    @Test
    fun startSelectedRoute_enters_phone_guidance_mode() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app, locationServiceOverride = FakeLocationService())
        val holder = HomeStateHolder(state, FakePlaceSearch())
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1",
                    title = "Route 1",
                    subtitle = "",
                    distanceMeters = 2500,
                    durationSeconds = 600,
                    normalizedPackage = straightLinePackage(),
                ),
            ),
            selectedAlternativeId = null,
            routeIdentifier = null,
            routeRevision = null,
            planningNotice = null,
        )
        holder.startSelectedRoute()
        assertEquals(HomeMode.PHONE_GUIDANCE, holder.homeMode)
    }
}
