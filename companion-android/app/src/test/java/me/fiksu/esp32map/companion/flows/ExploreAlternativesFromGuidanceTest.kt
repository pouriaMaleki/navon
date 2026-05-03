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

    // ─── Test 6: compassMode ───────────────────────────────────────────────────

    /**
     * exploreAlternateRoutes() must switch compassMode to NORTH_LOCKED so
     * the camera shows the full route overview while the rider browses.
     */
    @Test
    fun exploreAlternateRoutes_setsCompassToNorthLocked() = runTest {
        val holder = holderInPhoneGuidance()
        assertEquals("precondition", HomeCompassMode.AUTO_FOLLOW, holder.compassMode)
        val scope = TestScope(StandardTestDispatcher(testScheduler))

        holder.exploreAlternateRoutes(scope)

        assertEquals(
            "entering alternatives must switch compassMode to NORTH_LOCKED",
            HomeCompassMode.NORTH_LOCKED,
            holder.compassMode,
        )
    }

    /**
     * cancelAlternativesExploration() must restore compassMode to AUTO_FOLLOW
     * so the camera follows the rider again.
     */
    @Test
    fun cancelAlternativesExploration_restoresCompassToAutoFollow() = runTest {
        val holder = holderInPhoneGuidance()
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)
        assertEquals("precondition", HomeCompassMode.NORTH_LOCKED, holder.compassMode)

        holder.cancelAlternativesExploration()

        assertEquals(
            "cancelling must restore compassMode to AUTO_FOLLOW",
            HomeCompassMode.AUTO_FOLLOW,
            holder.compassMode,
        )
    }

    // ─── Test 7: selectedAlternativeIdForDisplay ───────────────────────────────

    /**
     * During exploration, selectedAlternativeIdForDisplay must be null so no
     * alternative row shows a checkmark — the "Continue" button marks the active route.
     */
    @Test
    fun selectedAlternativeIdForDisplay_isNullOnEnterExploration() = runTest {
        val holder = holderInPhoneGuidance()
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)

        assertNull(
            "selectedAlternativeIdForDisplay must be null on enter — no double checkmark",
            holder.selectedAlternativeIdForDisplay,
        )
    }

    /**
     * After the user calls selectAlternative during exploration, that alternative
     * gets a checkmark in the card.
     */
    @Test
    fun selectedAlternativeIdForDisplay_showsCheckmarkAfterTap() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1", title = "Route 1", subtitle = "",
                    distanceMeters = 2500, durationSeconds = 600,
                    normalizedPackage = minimalPackage(),
                ),
                RouteAlternative(
                    id = "a2", title = "Route 2", subtitle = "",
                    distanceMeters = 3000, durationSeconds = 700,
                    normalizedPackage = minimalPackage(),
                ),
            ),
            selectedAlternativeId = "a1",
            routeIdentifier = null,
            routeRevision = null,
            planningNotice = null,
        )
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)
        assertNull("pre-condition: null on enter", holder.selectedAlternativeIdForDisplay)

        holder.selectAlternativeForExploration("a2")

        assertEquals(
            "tapping an alternative during exploration must show its checkmark",
            "a2",
            holder.selectedAlternativeIdForDisplay,
        )
    }

    /**
     * Outside exploration, selectedAlternativeIdForDisplay returns the
     * planning-selected alternative ID.
     */
    @Test
    fun selectedAlternativeIdForDisplay_returnsSelectedIdOutsideExploration() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1", title = "Route 1", subtitle = "",
                    distanceMeters = 2500, durationSeconds = 600,
                    normalizedPackage = minimalPackage(),
                )
            ),
            selectedAlternativeId = "a1",
            routeIdentifier = null,
            routeRevision = null,
            planningNotice = null,
        )
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.homeMode = HomeMode.PHONE_GUIDANCE

        assertEquals(
            "outside exploration selectedAlternativeIdForDisplay must match the selected ID",
            "a1",
            holder.selectedAlternativeIdForDisplay,
        )
    }

    // ─── Test 8: guidanceAlternatives ─────────────────────────────────────────

    /**
     * guidanceAlternatives returns non-empty alternatives while exploring.
     */
    @Test
    fun guidanceAlternatives_returnsAlternativesDuringExploration() = runTest {
        val holder = holderInPhoneGuidance()
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)

        assertTrue(
            "guidanceAlternatives must return alternatives during exploration",
            holder.guidanceAlternatives.isNotEmpty(),
        )
    }

    /**
     * guidanceAlternatives is empty outside of exploration.
     */
    @Test
    fun guidanceAlternatives_isEmptyOutsideExploration() = runTest {
        val holder = holderInPhoneGuidance()

        assertTrue(
            "guidanceAlternatives must be empty when not exploring",
            holder.guidanceAlternatives.isEmpty(),
        )
    }

    // ─── Test 9: guidanceRoute stability ──────────────────────────────────────

    /**
     * guidanceRoute must stay frozen to the active route when exploration loads
     * new alternatives — prevents progress tracking using the wrong geometry.
     */
    @Test
    fun guidanceRoute_staysStableDuringExploration() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "orig",
                    title = "Original Route",
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
        holder.startSelectedRoute()
        val identifierBefore = holder.guidanceRoute?.routeIdentifier
        assertEquals("precondition: active route must be set", "alt-explore-test", identifierBefore)

        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.exploreAlternateRoutes(scope)

        // Replace the planning preview with a new route (simulating async re-plan result)
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "new",
                    title = "New Route",
                    subtitle = "",
                    distanceMeters = 3500,
                    durationSeconds = 800,
                    normalizedPackage = NormalizedRoutePackage(
                        version = RoutePackageVersion.CURRENT,
                        routeIdentifier = "new-plan-route",
                        revision = 1,
                        geometry = listOf(
                            CoordinatePoint(60.1699, 24.9384),
                            CoordinatePoint(60.2000, 24.9500),
                        ),
                        maneuvers = minimalPackage().maneuvers,
                        summary = minimalPackage().summary,
                        provenance = minimalPackage().provenance,
                    ),
                )
            ),
            selectedAlternativeId = null,
            routeIdentifier = null,
            routeRevision = null,
            planningNotice = null,
        )

        assertEquals(
            "guidanceRoute must be frozen to the active ride during exploration",
            identifierBefore,
            holder.guidanceRoute?.routeIdentifier,
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
