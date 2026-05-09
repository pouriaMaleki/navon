package me.fiksu.esp32map.companion.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.*
import me.fiksu.esp32map.companion.fakes.FakeLocationService
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Regression tests for guidance labels — activeNavigationTitle, guidanceSubtitleLine,
 * and the displayDestinationTitle fallback.
 *
 * Before the fix, displayDestinationTitle used "<providerName> route" as a fallback
 * when no real destination was known. This bled "OSM route" into the UI labels, since
 * "OSM route" was not in the placeholder set. These tests verify the corrected
 * behaviour: placeholder labels are filtered, and the fallback is "No destination".
 */
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class GuidanceLabelTest {

    private val origin = CoordinatePoint(60.1699, 24.9384)
    private val destination = CoordinatePoint(60.1921, 24.9458)

    private fun packageWithDestinationLabel(label: String?): NormalizedRoutePackage =
        NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "label-test",
            revision = 1,
            geometry = listOf(origin, destination),
            maneuvers = listOf(
                RouteManeuver("m1", RouteManeuverType.DEPART, origin, 0.0, 2500.0, null),
                RouteManeuver("m2", RouteManeuverType.ARRIVE, destination, 2500.0, null, null),
            ),
            summary = RouteSummary(2500.0, 600, null, label),
            provenance = RouteProvenance(RouteProviderId.OSM, null, 0L),
        )

    /** Returns (holder, state) with homeMode = PHONE_GUIDANCE for the given package. */
    private fun holderInGuidanceWith(pkg: NormalizedRoutePackage): Pair<HomeStateHolder, CompanionAppState> {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app, locationServiceOverride = FakeLocationService())
        state.preview = RoutePreviewModel(
            alternatives = listOf(RouteAlternative("a1", "Route 1", "", 2500, 600, pkg)),
            selectedAlternativeId = "a1",
            routeIdentifier = pkg.routeIdentifier,
            routeRevision = pkg.revision,
            planningNotice = null,
        )
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.startSelectedRoute()
        return holder to state
    }

    // ─── selectAlternativeForExploration — destination preservation ──────────

    @Test
    fun selectAlternativeForExploration_preservesDestinationLabel() {
        // Regression: selectAlternativeForExploration previously called
        // appState.selectAlternative which in turn called
        // applySelectedAlternativeToSession(preferredTitle = null), overwriting
        // activeSession.destinationLabel with the "No destination" fallback and
        // erasing the user-typed address. The fix uses selectAlternativePreviewOnly
        // which updates the preview selection without touching the session.
        val pkg1 = packageWithDestinationLabel("Selected destination")
        val pkg2 = packageWithDestinationLabel("Selected destination")
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app, locationServiceOverride = FakeLocationService())
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative("a1", "Route 1", "", 2500, 600, pkg1),
                RouteAlternative("a2", "Route 2", "", 3000, 720, pkg2),
            ),
            selectedAlternativeId = "a1",
            routeIdentifier = pkg1.routeIdentifier,
            routeRevision = pkg1.revision,
            planningNotice = null,
        )
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.startSelectedRoute()
        // Simulate the user-typed destination being set on the active session
        state.activeSession = state.activeSession.copy(destinationLabel = "Kallio")

        // Directly call the exploration selection (no need to enter exploration mode;
        // the invariant is that this call must never overwrite destinationLabel).
        holder.selectAlternativeForExploration("a2")

        assertEquals(
            "selectAlternativeForExploration must not overwrite activeSession.destinationLabel",
            "Kallio",
            state.activeSession.destinationLabel,
        )
    }

    // ─── activeNavigationTitle ────────────────────────────────────────────────

    @Test
    fun activeNavigationTitle_filtersSelectedDestination_andFallsToSessionLabel() {
        // "Selected destination" comes baked into the OSRM mapper — must not
        // surface as the guidance headline; fall to activeSession.destinationLabel.
        val (holder, state) = holderInGuidanceWith(packageWithDestinationLabel("Selected destination"))
        state.activeSession = state.activeSession.copy(destinationLabel = "Alppila")
        assertNotEquals(
            "activeNavigationTitle must not expose 'Selected destination' placeholder",
            "Selected destination",
            holder.activeNavigationTitle,
        )
        assertEquals(
            "activeNavigationTitle must fall back to the user-typed session label",
            "Alppila",
            holder.activeNavigationTitle,
        )
    }

    @Test
    fun activeNavigationTitle_filtersCurrentLocationPlaceholder() {
        val (holder, state) = holderInGuidanceWith(packageWithDestinationLabel("Current location"))
        state.activeSession = state.activeSession.copy(destinationLabel = "Kallio")
        assertEquals("Kallio", holder.activeNavigationTitle)
    }

    // ─── guidanceSubtitleLine ─────────────────────────────────────────────────

    @Test
    fun guidanceSubtitleLine_filtersSelectedDestination_showsOnlyRemaining() {
        // "Selected destination" must not appear in the subtitle like
        // "Selected destination • Riding on phone".
        val (holder, _) = holderInGuidanceWith(packageWithDestinationLabel("Selected destination"))
        assertFalse(
            "guidanceSubtitleLine must not contain 'Selected destination'",
            holder.guidanceSubtitleLine.contains("Selected destination", ignoreCase = true),
        )
    }

    @Test
    fun guidanceSubtitleLine_withRealDestination_includesItInSubtitle() {
        val (holder, _) = holderInGuidanceWith(packageWithDestinationLabel("Kamppi"))
        assertTrue(
            "guidanceSubtitleLine must include the real destination 'Kamppi'",
            holder.guidanceSubtitleLine.contains("Kamppi"),
        )
    }

    // ─── displayDestinationTitle fallback ─────────────────────────────────────

    @Test
    fun displayDestinationFallback_isNoDestination_notProviderName() {
        // Regression: before the fix, applySelectedAlternativeToSession used
        // "${providerId.displayName} route" (e.g. "OSM route") as the fallback
        // when no preferred title and no meaningful package label was available.
        // That bled "OSM route" into the subtitle and navigation title.
        // The fixed fallback is "No destination" — a known placeholder — so the
        // provider name never surfaces as if it were a destination address.
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app, locationServiceOverride = FakeLocationService())
        state.routeRequest = RoutePlanRequest(
            origin = origin,
            destination = destination,
            providerId = RouteProviderId.OSM,
        )
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative("a1", "OSM Route", "", 2500, 600, packageWithDestinationLabel("Selected destination"))
            ),
            selectedAlternativeId = "a1",
            routeIdentifier = "label-test",
            routeRevision = 1,
            planningNotice = null,
        )
        // selectAlternative calls applySelectedAlternativeToSession(preferredTitle = null),
        // exercising the fallback path (no user-typed address, package label is a placeholder).
        state.selectAlternative("a1")
        val label = state.activeSession.destinationLabel.lowercase()
        assertFalse(
            "displayDestinationTitle fallback must not inject provider name — got '${state.activeSession.destinationLabel}'",
            label.contains("osm") || label.contains("hsl") || label.contains("route"),
        )
    }
}
