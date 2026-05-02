package me.fiksu.esp32map.companion.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.*
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Regression tests for the "explore alternatives from active guidance" flow
 * (the "split" button during PHONE_GUIDANCE).
 *
 * Spec: pressing the split icon must NOT drop homeMode to PLANNING. Instead:
 *  - homeMode stays PHONE_GUIDANCE so guidance keeps running.
 *  - isExploringAlternativesFromGuidance is true while the panel is open.
 *  - cancelAlternativesExploration() dismisses the panel without touching routing.
 *  - startSelectedRoute() commits a new route and clears the flag.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class ExploreAlternativesFromGuidanceTest {

    // ─── Shared harness ────────────────────────────────────────────────────────

    private fun makeHolder(): HomeStateHolder {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        return HomeStateHolder(state, FakePlaceSearch())
    }

    /**
     * A minimal NormalizedRoutePackage that satisfies startSelectedRoute()
     * (needs a non-null routeIdentifier and at least one alternative).
     */
    private fun minimalPackage(): NormalizedRoutePackage {
        val origin = CoordinatePoint(60.1699, 24.9384)
        val destination = CoordinatePoint(60.1921, 24.9458)
        return NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "alt-explore-test",
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

    /**
     * Returns a holder in PHONE_GUIDANCE with a seeded preview so
     * `destinationCoordinate` is non-null and `exploreAlternateRoutes()`
     * passes its guard.
     */
    private fun holderInPhoneGuidance(): HomeStateHolder {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1",
                    title = "Route 1",
                    subtitle = "",
                    distanceMeters = 2500,
                    durationSeconds = 600,
                    normalizedPackage = minimalPackage(),
                )
            ),
            selectedAlternativeId = null,
            routeIdentifier = null,
            routeRevision = null,
            planningNotice = null,
        )
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        return holder
    }

    // ─── Test 1 ────────────────────────────────────────────────────────────────

    /**
     * isExploringAlternativesFromGuidance defaults to false.
     */
    @Test
    fun exploreAlternateRoutes_isExploringFlagDefaultsFalse() = runTest {
        val holder = makeHolder()
        assertFalse(
            "isExploringAlternativesFromGuidance must default to false",
            holder.isExploringAlternativesFromGuidance,
        )
    }

    // ─── Test 2 ────────────────────────────────────────────────────────────────

    /**
     * Pressing the split button during PHONE_GUIDANCE must NOT leave routing —
     * homeMode stays PHONE_GUIDANCE after exploreAlternateRoutes().
     */
    @Test
    fun exploreAlternateRoutes_keepsPhoneGuidanceMode() = runTest {
        val holder = holderInPhoneGuidance()
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)
        assertEquals(
            "exploreAlternateRoutes must keep homeMode in PHONE_GUIDANCE",
            HomeMode.PHONE_GUIDANCE,
            holder.homeMode,
        )
    }

    // ─── Test 3 ────────────────────────────────────────────────────────────────

    /**
     * After exploreAlternateRoutes() is called, the alternatives panel flag
     * is set to true so the UI can show the alternatives panel.
     */
    @Test
    fun exploreAlternateRoutes_setsExploringFlag() = runTest {
        val holder = holderInPhoneGuidance()
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)
        assertTrue(
            "exploreAlternateRoutes must set isExploringAlternativesFromGuidance to true",
            holder.isExploringAlternativesFromGuidance,
        )
    }

    // ─── Test 4 ────────────────────────────────────────────────────────────────

    /**
     * cancelAlternativesExploration() clears the flag and leaves routing
     * intact (homeMode stays PHONE_GUIDANCE).
     */
    @Test
    fun cancelAlternativesExploration_clearsFlag() = runTest {
        val holder = holderInPhoneGuidance()
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)
        holder.cancelAlternativesExploration()
        assertFalse(
            "cancelAlternativesExploration must clear isExploringAlternativesFromGuidance",
            holder.isExploringAlternativesFromGuidance,
        )
        assertEquals(
            "cancelAlternativesExploration must keep homeMode in PHONE_GUIDANCE",
            HomeMode.PHONE_GUIDANCE,
            holder.homeMode,
        )
    }

    // ─── Test 5 ────────────────────────────────────────────────────────────────

    /**
     * startSelectedRoute() clears isExploringAlternativesFromGuidance so
     * the alternatives panel is dismissed when a new route is confirmed.
     */
    @Test
    fun startSelectedRoute_clearsExploringFlag() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))

        // Seed a preview so startSelectedRoute() can proceed past the early
        // return guard (it requires a non-null routeIdentifier).
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1",
                    title = "Route 1",
                    subtitle = "",
                    distanceMeters = 2500,
                    durationSeconds = 600,
                    normalizedPackage = minimalPackage(),
                ),
            ),
            selectedAlternativeId = null,
            routeIdentifier = null,
            routeRevision = null,
            planningNotice = null,
        )

        // Enter guidance and then start exploring alternatives.
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        holder.exploreAlternateRoutes(scope)
        assertTrue(
            "pre-condition: isExploringAlternativesFromGuidance must be true before startSelectedRoute",
            holder.isExploringAlternativesFromGuidance,
        )

        // Confirming a (re)route must clear the flag.
        holder.startSelectedRoute()
        assertFalse(
            "startSelectedRoute must clear isExploringAlternativesFromGuidance",
            holder.isExploringAlternativesFromGuidance,
        )
    }
}
