package app.navon.bike.integration.hsl

import kotlinx.coroutines.runBlocking
import app.navon.bike.domain.CompanionSettings
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.RouteProviderId
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Verifies that HslRoutingAdapter throws meaningful errors instead of silently
 * failing when the upstream is unreachable.
 */
class HslErrorHandlingTest {

    private val origin = CoordinatePoint(60.1699, 24.9384)
    private val destination = CoordinatePoint(60.1921, 24.9458)

    /** Point to a port that nothing listens on → connection refused fast. */
    private fun deadAdapter() = HslRoutingAdapter(
        settingsProvider = {
            CompanionSettings(
                hslEndpointUrl = "http://127.0.0.1:1/api/hsl/routing",
                cyclingSpeedKph = 18.0,
            )
        },
    )

    @Test
    fun planRoute_throwsWhenServerUnreachable() = runBlocking {
        var caught: Throwable? = null
        try {
            deadAdapter().planRoute(
                RoutePlanRequest(origin, destination, RouteProviderId.HSL)
            )
        } catch (e: Exception) {
            caught = e
        }
        assertNotNull("expected planRoute to throw when server is unreachable", caught)
    }

    @Test
    fun planRoute_throwsWhenInvalidUrl() = runBlocking {
        val adapter = HslRoutingAdapter(
            settingsProvider = {
                CompanionSettings(hslEndpointUrl = "not-a-valid-url")
            },
        )
        var caught: Throwable? = null
        try {
            adapter.planRoute(RoutePlanRequest(origin, destination, RouteProviderId.HSL))
        } catch (e: Exception) {
            caught = e
        }
        assertNotNull("expected planRoute to throw for invalid URL", caught)
    }
}
