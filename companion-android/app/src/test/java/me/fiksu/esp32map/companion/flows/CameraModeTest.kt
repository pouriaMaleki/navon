package me.fiksu.esp32map.companion.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.HomeCompassMode
import me.fiksu.esp32map.companion.domain.HomeMode
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.assertEquals
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
}
