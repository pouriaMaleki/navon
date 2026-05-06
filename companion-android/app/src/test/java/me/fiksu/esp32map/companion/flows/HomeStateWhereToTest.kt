package me.fiksu.esp32map.companion.flows

import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.DestinationSearchResult
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.fakes.FakeLocationService
import me.fiksu.esp32map.companion.fakes.FakePlaceSearch
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import android.app.Application

/**
 * L2 tests — where-to dropdown surface (plan flows #20, #25, #35-37).
 *
 * Drives `HomeStateHolder` with FakePlaceSearch and a real `CompanionAppState`
 * built under Robolectric. This is heavier than we'd like: `CompanionAppState`
 * wires `BleRouteSyncService`, `AndroidLocationService`, and
 * `CompanionPersistence` inside its constructor, so a regression in any of
 * those init paths breaks every where-to test.
 *
 * Proper narrowing requires extracting an interface (e.g. `HomeStateContext`)
 * that exposes only the fields `HomeStateHolder` reads. That is a source-code
 * refactor and is tracked as a follow-up. Until then, Robolectric is the
 * cheapest way to exercise the happy path without pulling the rest of the app.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, application = Application::class)
class HomeStateWhereToTest {

    private fun newHolder(): Pair<HomeStateHolder, FakePlaceSearch> {
        val app = ApplicationProvider.getApplicationContext<Application>()
        val state = CompanionAppState(app, locationServiceOverride = FakeLocationService())
        val search = FakePlaceSearch()
        return HomeStateHolder(state, search) to search
    }

    @Test
    fun blank_query_clears_suggestions() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val scope = TestScope(dispatcher)
        val (holder, search) = newHolder()
        search.nextResults = listOf(
            DestinationSearchResult(
                id = "r1",
                title = "Cathedral",
                subtitle = "",
                coordinate = CoordinatePoint(60.17, 24.95),
            )
        )
        holder.updateQuery("cathedral", scope)
        scope.advanceUntilIdle()
        assertFalse(holder.suggestions.isEmpty())
        holder.updateQuery("", scope)
        assertTrue(holder.suggestions.isEmpty())
    }

    @Test
    fun url_query_flips_isResolvingUrl_true() = runTest {
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        val (holder, _) = newHolder()
        holder.updateQuery("https://www.google.com/maps/@60.16,24.95,15z", scope)
        assertTrue(
            "url paste must flip isResolvingUrl synchronously",
            holder.isResolvingUrl,
        )
    }

    @Test
    fun closeSearch_cancels_url_resolve() = runTest {
        val scope = TestScope(StandardTestDispatcher(testScheduler))
        val (holder, _) = newHolder()
        holder.updateQuery("https://www.google.com/maps/@60.16,24.95,15z", scope)
        assertTrue(holder.isResolvingUrl)
        holder.closeSearch()
        assertFalse(holder.isResolvingUrl)
        assertNull(holder.urlResolveError)
    }

    @Test
    fun manual_keyboard_delete_clears_suggestions() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val scope = TestScope(dispatcher)
        val (holder, search) = newHolder()
        search.nextResults = listOf(
            DestinationSearchResult(
                id = "r1", title = "Station", subtitle = "",
                coordinate = CoordinatePoint(60.17, 24.95),
            )
        )
        holder.updateQuery("station", scope)
        scope.advanceUntilIdle()
        assertFalse(holder.suggestions.isEmpty())
        for (prefix in listOf("statio", "stati", "stat", "sta", "st", "s", "")) {
            holder.updateQuery(prefix, scope)
        }
        assertTrue(holder.suggestions.isEmpty())
    }

    @Test
    fun loadMoreRecentsIfNeeded_isNoOp_forNonLastItem() = runTest {
        // Spec lines 71-73: pagination is gated on reaching the end of the
        // visible slice. An id that isn't the last visible recent must not
        // grow the visible count.
        val (holder, _) = newHolder()
        val before = holder.visibleRecentCount
        holder.loadMoreRecentsIfNeeded(
            me.fiksu.esp32map.companion.domain.RouteHistoryItem(
                id = "not-the-last-id",
                title = "x",
                subtitle = "",
                source = me.fiksu.esp32map.companion.domain.RouteHistorySource.RECENT_DESTINATION,
                sourceLabel = "Recent",
                createdAtLabel = "now",
                destination = CoordinatePoint(60.17, 24.95),
                routePackage = null,
                occurrenceCount = null,
            )
        )
        assertEquals(before, holder.visibleRecentCount)
    }

    @Test
    fun initialVisibleRecentCount_isBounded() = runTest {
        // Spec lines 72-73: initial slice must be a small page.
        val (holder, _) = newHolder()
        assertTrue(
            "initial recents slice must be bounded so large histories don't render everything up-front",
            holder.visibleRecentCount < 30,
        )
    }
}
