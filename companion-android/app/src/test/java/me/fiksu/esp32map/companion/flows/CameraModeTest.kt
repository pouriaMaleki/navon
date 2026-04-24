package me.fiksu.esp32map.companion.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.HomeCompassMode
import me.fiksu.esp32map.companion.domain.HomeMode
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RouteManeuver
import me.fiksu.esp32map.companion.domain.RouteManeuverType
import me.fiksu.esp32map.companion.domain.RoutePackageVersion
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProvenance
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteSummary
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * L2 compass / camera-mode tests (plan flows #44, #45, #52).
 * Spec source: docs/ux-specs.md lines 95-97 (compass double-tap locks
 * north-up while routing).
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class CameraModeTest {

    @Test
    fun handleCompassDoubleTap_locks_north_up_while_routing() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        // handleCompassDoubleTap is a no-op outside phone guidance (see
        // HomeStateHolder.kt:365). We must put the holder into PHONE_GUIDANCE
        // to exercise the actual spec flow.
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        holder.handleCompassDoubleTap()
        assertEquals(HomeCompassMode.NORTH_LOCKED, holder.compassMode)
    }

    @Test
    fun handleCompassDoubleTap_outsideGuidance_isNoOp() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        // homeMode defaults to PLANNING — handler must no-op.
        holder.handleCompassDoubleTap()
        assertEquals(HomeCompassMode.AUTO_FOLLOW, holder.compassMode)
    }

    @Test
    fun handleCompassTap_outsideGuidance_isNoOp() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.handleCompassTap(scope)
        assertEquals(HomeCompassMode.AUTO_FOLLOW, holder.compassMode)
    }

    @Test
    fun handleCompassTap_duringRouting_entersNorthPreview() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        holder.handleCompassTap(scope)
        assertEquals(HomeCompassMode.NORTH_PREVIEW, holder.compassMode)
    }

    // ─── Follow rider during routing (spec line 84) ─────────────────────────

    @Test
    fun notifyRiderLocationUpdated_duringRouting_bumpsFollowTick() = runTest {
        // Spec line 84: during routing, the camera follows the rider on every
        // GPS update. HomeStateHolder must expose `mapFollowRiderTick` and
        // bump it when `notifyRiderLocationUpdated()` is called in guidance.
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        val before = holder.mapFollowRiderTick
        holder.notifyRiderLocationUpdated()
        assertEquals(
            "notifyRiderLocationUpdated during routing must bump mapFollowRiderTick",
            before + 1,
            holder.mapFollowRiderTick,
        )
    }

    @Test
    fun notifyRiderLocationUpdated_outsideRouting_isNoOp() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        // homeMode defaults to PLANNING.
        val before = holder.mapFollowRiderTick
        holder.notifyRiderLocationUpdated()
        assertEquals(before, holder.mapFollowRiderTick)
    }

    // ─── Auto-recenter after user map interaction (spec line 104) ───────────

    @Test
    fun noteUserMapInteraction_duringRouting_schedulesRecenter() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        val before = holder.mapRecenterRequestTick
        holder.noteUserMapInteraction(scope)
        // Before the timeout: no bump.
        scope.advanceTimeBy(500)
        scope.runCurrent()
        assertEquals(
            "recenter must not fire before the pinned timeout",
            before,
            holder.mapRecenterRequestTick,
        )
        // Past the 1300 ms window: one bump.
        scope.advanceTimeBy(1000)
        scope.runCurrent()
        assertEquals(
            "recenter must fire once the pinned inactivity timeout elapses",
            before + 1,
            holder.mapRecenterRequestTick,
        )
    }

    @Test
    fun noteUserMapInteraction_outsideRouting_isNoOp() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        // homeMode defaults to PLANNING.
        val before = holder.mapRecenterRequestTick
        holder.noteUserMapInteraction(scope)
        scope.advanceTimeBy(2000)
        scope.runCurrent()
        assertEquals(before, holder.mapRecenterRequestTick)
    }

    // ─── Routing camera bearing (spec line 101) ─────────────────────────────

    private fun lShapeRoute(): NormalizedRoutePackage {
        val metersPerDegreeLat = 111_320.0
        val start = CoordinatePoint(60.17, 24.94)
        val mid = CoordinatePoint(60.17 + 400.0 / metersPerDegreeLat, 24.94)
        val cosLat = kotlin.math.cos(60.17 * Math.PI / 180.0)
        val end = CoordinatePoint(
            mid.latitude,
            mid.longitude + 400.0 / (metersPerDegreeLat * cosLat),
        )
        return NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "bearing-test",
            revision = 1,
            geometry = listOf(start, mid, end),
            maneuvers = listOf(
                RouteManeuver("m1", RouteManeuverType.DEPART, start, 0.0, 400.0, null),
                RouteManeuver("m2", RouteManeuverType.RIGHT, mid, 400.0, 400.0, null),
                RouteManeuver("m3", RouteManeuverType.ARRIVE, end, 800.0, null, null),
            ),
            summary = RouteSummary(800.0, 240, null, null),
            provenance = RouteProvenance(RouteProviderId.OSM, null, 0L),
        )
    }

    @Test
    fun routingBearingDegrees_atRouteStart_pointsAlongFirstLeg() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val pkg = lShapeRoute()
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1", title = "R", subtitle = "",
                    distanceMeters = 800, durationSeconds = 240,
                    normalizedPackage = pkg,
                ),
            ),
            selectedAlternativeID = null, routeIdentifier = null,
            routeRevision = null, planningNotice = null,
        )
        holder.startSelectedRoute()
        val bearing = holder.routingBearingDegrees(pkg.geometry[0])
        val delta = ((bearing + 540.0) % 360.0) - 180.0
        assertTrue(
            "first leg bearing should be ~0° (north), got $bearing",
            kotlin.math.abs(delta) < 5.0,
        )
    }

    @Test
    fun routingBearingDegrees_shiftsToNextLegOnceProgressPassesCorner() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val pkg = lShapeRoute()
        state.preview = RoutePreviewModel(
            alternatives = listOf(
                RouteAlternative(
                    id = "a1", title = "R", subtitle = "",
                    distanceMeters = 800, durationSeconds = 240,
                    normalizedPackage = pkg,
                ),
            ),
            selectedAlternativeID = null, routeIdentifier = null,
            routeRevision = null, planningNotice = null,
        )
        holder.startSelectedRoute()
        val bearing = holder.routingBearingDegrees(pkg.geometry[1])
        assertTrue(
            "second-leg bearing should be ~90° (east), got $bearing",
            kotlin.math.abs(bearing - 90.0) < 5.0,
        )
    }

    // ─── Compass lock holds the overview (web-parity regression) ────────────

    /**
     * Spec lines 95-96 + web-parity regression (the '🧭 reverts after 1.3 s'
     * bug). Once the compass is locked, neither GPS ticks nor the inactivity
     * recenter must flip the mode off .NORTH_LOCKED. Android achieves this
     * at the view layer — the MainActivity LaunchedEffect dispatches to
     * `fitCameraToRoute` whenever `compassMode` is .NORTH_LOCKED /
     * .NORTH_PREVIEW regardless of which tick bumped. We protect that
     * contract at the state-holder layer by asserting the mode does not
     * drift from .NORTH_LOCKED in response to GPS / inactivity events.
     */
    @Test
    fun compassLock_survivesRiderLocationUpdates() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        holder.compassMode = HomeCompassMode.NORTH_LOCKED
        holder.notifyRiderLocationUpdated()
        holder.notifyRiderLocationUpdated()
        assertEquals(
            "GPS ticks must not flip compassMode off NORTH_LOCKED",
            HomeCompassMode.NORTH_LOCKED,
            holder.compassMode,
        )
    }

    @Test
    fun compassLock_survivesInactivityTimeoutRecenter() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.homeMode = HomeMode.PHONE_GUIDANCE
        holder.compassMode = HomeCompassMode.NORTH_LOCKED
        holder.noteUserMapInteraction(scope)
        scope.advanceTimeBy(1500)
        scope.runCurrent()
        assertEquals(
            "inactivity-timeout recenter must not flip compassMode off NORTH_LOCKED",
            HomeCompassMode.NORTH_LOCKED,
            holder.compassMode,
        )
    }

    // ─── GPS-trail heading overrides route bearing (spec line 110) ──────────
    //
    // Spec line 110 (authoritative): "camera rotates so that riding direction
    // is towards top of the screen this overrides the camera of routing. Most
    // important camera behaviour is this. (it needs to determine the
    // direction by last few GPS locations it receives)".

    private fun offset(base: CoordinatePoint, eastM: Double, northM: Double): CoordinatePoint {
        val metersPerDegreeLat = 111_320.0
        val meanLat = base.latitude * Math.PI / 180.0
        return CoordinatePoint(
            base.latitude + northM / metersPerDegreeLat,
            base.longitude + eastM / (metersPerDegreeLat * kotlin.math.cos(meanLat)),
        )
    }

    @Test
    fun headingTrail_jitterBelowFloor_yieldsNoHeading() {
        val trail = me.fiksu.esp32map.companion.integration.location.HeadingTrail(
            maxAgeMs = 5_000, maxFixes = 10,
            minDisplacementM = 3.0, smoothingAlpha = 0.25,
        )
        val base = CoordinatePoint(60.17, 24.94)
        trail.recordFix(base, 0L)
        trail.recordFix(offset(base, eastM = 0.5, northM = 0.0), 100L)
        assertTrue(
            "tiny GPS jitter below the displacement floor must not produce a heading",
            trail.travelHeadingDegrees == null,
        )
    }

    @Test
    fun headingTrail_eastLeg_producesEastBearing() {
        val trail = me.fiksu.esp32map.companion.integration.location.HeadingTrail(
            maxAgeMs = 5_000, maxFixes = 10,
            minDisplacementM = 3.0, smoothingAlpha = 0.25,
        )
        val base = CoordinatePoint(60.17, 24.94)
        for (i in 0 until 5) {
            trail.recordFix(offset(base, eastM = i * 1.2, northM = 0.0), (i * 200).toLong())
        }
        val heading = trail.travelHeadingDegrees
        assertTrue("east-only motion should give ≈90°, got $heading",
            heading != null && kotlin.math.abs(heading - 90.0) < 5.0)
    }

    @Test
    fun headingTrail_smoothsLateralJitter() {
        val trail = me.fiksu.esp32map.companion.integration.location.HeadingTrail(
            maxAgeMs = 5_000, maxFixes = 10,
            minDisplacementM = 3.0, smoothingAlpha = 0.25,
        )
        val base = CoordinatePoint(60.17, 24.94)
        for (i in 0 until 20) {
            val east = i * 2.5
            val noise = ((i % 2) * 2.0 - 1.0) * 1.5
            trail.recordFix(offset(base, eastM = east, northM = noise), (i * 200).toLong())
        }
        val heading = trail.travelHeadingDegrees ?: 0.0
        assertTrue(
            "smoothed trail heading must stay tight to east under lateral jitter, got $heading",
            kotlin.math.abs(heading - 90.0) < 8.0,
        )
    }

    @Test
    fun movingWithRoute_cameraBearingTracksTrailHeading_notRouteSegment() = runTest {
        // Route goes NORTH; rider actually moves EAST. Camera must follow EAST.
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val start = CoordinatePoint(60.17, 24.94)
        val northEnd = CoordinatePoint(60.175, 24.94)
        val pkg = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "north-route",
            revision = 1,
            geometry = listOf(start, northEnd),
            maneuvers = listOf(
                RouteManeuver("m1", RouteManeuverType.DEPART, start, 0.0, 500.0, null),
                RouteManeuver("m2", RouteManeuverType.ARRIVE, northEnd, 500.0, null, null),
            ),
            summary = RouteSummary(500.0, 120, null, null),
            provenance = RouteProvenance(RouteProviderId.OSM, null, 0L),
        )
        state.preview = RoutePreviewModel(
            alternatives = listOf(RouteAlternative("a1", "R", "", 500, 120, pkg)),
            selectedAlternativeID = null, routeIdentifier = null,
            routeRevision = null, planningNotice = null,
        )
        holder.startSelectedRoute()
        for (i in 0 until 8) {
            holder.ingestRiderLocationFix(offset(start, eastM = i * 2.5, northM = 0.0),
                (i * 200).toLong())
        }
        val heading = holder.cameraHeadingDegrees(offset(start, eastM = 17.5, northM = 0.0))
        assertTrue(
            "moving east with a north-going route → camera must follow east (spec 110), got $heading",
            kotlin.math.abs(heading - 90.0) < 8.0,
        )
    }

    @Test
    fun movingWithoutRoute_cameraBearingTracksTrailHeading() {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val start = CoordinatePoint(60.17, 24.94)
        for (i in 0 until 8) {
            holder.ingestRiderLocationFix(offset(start, eastM = i * 2.5, northM = 0.0),
                (i * 200).toLong())
        }
        val heading = holder.cameraHeadingDegrees(offset(start, eastM = 17.5, northM = 0.0))
        assertTrue(
            "moving east without a route — camera must rotate to travel direction (spec 110), got $heading",
            kotlin.math.abs(heading - 90.0) < 8.0,
        )
    }

    @Test
    fun movingInPlanning_travelHeadingPopulated_evenWithoutRoute() = runTest {
        // Spec lines 108-118: "when moving (with or without a route)" the
        // camera enters riding mode. The trail must be fed in any mode and
        // expose travelHeadingDegrees so the Compose camera dispatch can
        // rotate. Existing tests only covered phoneGuidance.
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        // No startSelectedRoute — homeMode stays PLANNING.
        val start = CoordinatePoint(60.17, 24.94)
        for (i in 0 until 8) {
            holder.ingestRiderLocationFix(offset(start, eastM = i * 2.5, northM = 0.0),
                (i * 200).toLong())
        }
        assertEquals(HomeMode.PLANNING, holder.homeMode)
        val heading = holder.travelHeadingDegrees
        assertTrue(
            "moving in planning mode (no route) — travelHeadingDegrees must be defined and ≈90° east, got $heading",
            heading != null && kotlin.math.abs(heading - 90.0) < 8.0,
        )
    }

    @Test
    fun stationaryOnRoute_cameraBearingFallsBackToRouteSegment() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val start = CoordinatePoint(60.17, 24.94)
        val northEnd = CoordinatePoint(60.175, 24.94)
        val pkg = NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "north-route",
            revision = 1,
            geometry = listOf(start, northEnd),
            maneuvers = listOf(
                RouteManeuver("m1", RouteManeuverType.DEPART, start, 0.0, 500.0, null),
                RouteManeuver("m2", RouteManeuverType.ARRIVE, northEnd, 500.0, null, null),
            ),
            summary = RouteSummary(500.0, 120, null, null),
            provenance = RouteProvenance(RouteProviderId.OSM, null, 0L),
        )
        state.preview = RoutePreviewModel(
            alternatives = listOf(RouteAlternative("a1", "R", "", 500, 120, pkg)),
            selectedAlternativeID = null, routeIdentifier = null,
            routeRevision = null, planningNotice = null,
        )
        holder.startSelectedRoute()
        for (i in 0 until 5) holder.ingestRiderLocationFix(start, (i * 200).toLong())
        val heading = holder.cameraHeadingDegrees(start)
        val normalized = ((heading + 540.0) % 360.0) - 180.0
        assertTrue(
            "stationary on a north route — camera falls back to route bearing (north ≈ 0°), got $heading",
            kotlin.math.abs(normalized) < 5.0,
        )
    }
}
