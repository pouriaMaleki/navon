package app.navon.bike.integration.hsl

import app.navon.bike.domain.CompanionSettings
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.RouteProviderId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Tests for HslRoutingAdapter pure functions.
 *
 * planRoute() tests live in HslLiveRoutingTest.kt — they require a running
 * server and are gated behind an opt-in flag.
 */
class HslSpeedOverrideTest {

    @Test
    fun overrideDurationSeconds_helperRoundsToTotalDistanceOverSpeed() {
        // 2500 m / (18 kph / 3.6) = 2500 / 5.0 = 500 s
        val s = HslRoutingAdapter.overrideDurationSeconds(
            totalDistanceMeters = 2500.0,
            cyclingSpeedKph = 18.0,
            fallbackSeconds = 999,
        )
        assertEquals(500, s)
    }

    @Test
    fun overrideDurationSeconds_returnsFallbackForNonPositiveSpeed() {
        val s = HslRoutingAdapter.overrideDurationSeconds(
            totalDistanceMeters = 2500.0,
            cyclingSpeedKph = 0.0,
            fallbackSeconds = 600,
        )
        assertEquals(600, s)
    }

    @Test
    fun overrideDurationSeconds_returnsFallbackForInfiniteSpeed() {
        val s = HslRoutingAdapter.overrideDurationSeconds(
            totalDistanceMeters = 2500.0,
            cyclingSpeedKph = Double.POSITIVE_INFINITY,
            fallbackSeconds = 600,
        )
        assertEquals(600, s)
    }
}
