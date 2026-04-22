package me.fiksu.esp32map.companion.flows

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * L2 URL paste tests (plan flows #29-31).
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class HomeStateUrlPasteTest {

    @Test
    fun url_query_starts_url_resolve_and_clears_search_suggestions() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.updateQuery("https://www.google.com/maps/@60.16,24.95,15z", scope)
        assertTrue(holder.isResolvingUrl)
        assertTrue(holder.suggestions.isEmpty())
    }

    @Test
    fun nonUrl_query_does_not_set_isResolvingUrl() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.updateQuery("helsinki central", scope)
        assertFalse(holder.isResolvingUrl)
    }

    @Test
    fun closeSearch_clears_url_state() = runTest {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app)
        val holder = HomeStateHolder(state, FakePlaceSearch())
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        holder.updateQuery("https://maps.app.goo.gl/test", scope)
        holder.closeSearch()
        assertFalse(holder.isResolvingUrl)
        assertNull(holder.urlResolveError)
    }
}
