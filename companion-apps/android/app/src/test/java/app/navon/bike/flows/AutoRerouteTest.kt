package app.navon.bike.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import app.navon.bike.app.CompanionAppState
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.HomeMode
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.RouteAlternative
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.domain.RoutePackageVersion
import app.navon.bike.domain.RoutePreviewModel
import app.navon.bike.domain.RouteProvenance
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RouteSummary
import app.navon.bike.fakes.FakeLocationService
import app.navon.bike.fakes.FakePlaceSearch
import app.navon.bike.feature.home.HomeStateHolder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class AutoRerouteTest {

    private fun offset(base: CoordinatePoint, eastM: Double, northM: Double): CoordinatePoint {
        val metersPerDegLat = 111_320.0
        val meanLat = base.latitude * Math.PI / 180.0
        return CoordinatePoint(
            latitude = base.latitude + northM / metersPerDegLat,
            longitude = base.longitude + eastM / (metersPerDegLat * kotlin.math.cos(meanLat)),
        )
    }

    private fun straightRoute(): NormalizedRoutePackage {
        val metersPerDegLat = 111_320.0
        val start = CoordinatePoint(60.17, 24.94)
        val end = CoordinatePoint(60.17 + 800.0 / metersPerDegLat, 24.94)
        return NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "straight",
            revision = 1,
            geometry = listOf(start, end),
            maneuvers = listOf(
                RouteManeuver("m1", RouteManeuverType.DEPART, start, 0.0, 800.0, null),
                RouteManeuver("m2", RouteManeuverType.ARRIVE, end, 800.0, null, null),
            ),
            summary = RouteSummary(800.0, 240, null, null),
            provenance = RouteProvenance(RouteProviderId.OSM, null, 0L),
        )
    }

    @Test
    fun sustainedOffRoute_triggersFollowUpAutoRerouteWithoutOnRouteReset() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app, locationServiceOverride = FakeLocationService())
        val rerouteAttempts = mutableListOf<CoordinatePoint>()
        val holder = HomeStateHolder(
            appState = state,
            placeSearchService = FakePlaceSearch(),
            autoRerouteDispatcher = { rider -> rerouteAttempts += rider },
            autoRerouteScope = backgroundScope,
        )
        val route = straightRoute()
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1",
                    title = "Straight",
                    subtitle = "",
                    distanceMeters = 800,
                    durationSeconds = 240,
                    normalizedPackage = route,
                ),
            ),
        )
        holder.startSelectedRoute()
        assertEquals(HomeMode.PHONE_GUIDANCE, holder.homeMode)

        val drifted = offset(route.geometry.first(), eastM = 50.0, northM = 0.0)
        holder.ingestRiderLocationFix(drifted, 0)
        holder.ingestRiderLocationFix(drifted, 1_500)
        holder.ingestRiderLocationFix(drifted, 3_000)
        advanceUntilIdle()
        assertTrue(holder.offRoute)
        assertEquals(1, rerouteAttempts.size)

        // Stay off-route and accumulate another dwell period.
        holder.ingestRiderLocationFix(drifted, 4_500)
        holder.ingestRiderLocationFix(drifted, 6_000)
        advanceUntilIdle()
        assertEquals(
            "follow-up off-route dwell should trigger another reroute attempt",
            2,
            rerouteAttempts.size,
        )
    }
}
