package app.navon.bike.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import app.navon.bike.app.CompanionAppState
import app.navon.bike.domain.RoutePackageVersion
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.HomeMode
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.RouteAlternative
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.domain.RoutePreviewModel
import app.navon.bike.domain.RouteProvenance
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RouteSummary
import app.navon.bike.fakes.FakeLocationService
import app.navon.bike.fakes.FakePlaceSearch
import app.navon.bike.feature.home.HomeStateHolder
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
