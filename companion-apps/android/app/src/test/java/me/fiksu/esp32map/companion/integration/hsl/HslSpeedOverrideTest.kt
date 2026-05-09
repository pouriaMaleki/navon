package me.fiksu.esp32map.companion.integration.hsl

import kotlinx.coroutines.runBlocking
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RouteProviderId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Mirrors the web `hslSpeed.test.ts` and the iOS `HslSpeedOverrideTests`:
 * the cycling-speed setting overrides the HSL ETA so listed times match
 * real-world riding rather than Digitransit's conservative defaults.
 *
 * Why existing tests didn't cover this: Android HSL tests were limited
 * to source-mode gating; nothing exercised the duration-override pipeline.
 */
class HslSpeedOverrideTest {

    private val origin = CoordinatePoint(60.1699, 24.9384)
    private val destination = CoordinatePoint(60.1921, 24.9458)

    private fun adapter(speedKph: Double): HslRoutingAdapter {
        return HslRoutingAdapter {
            CompanionSettings(
                preferLiveHslRouting = false,
                hslSubscriptionKey = "",
                hslEndpointUrl = CompanionSettings().hslEndpointUrl,
                cyclingSpeedKph = speedKph,
            )
        }
    }

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
    fun planRoute_appliesCyclingSpeedToEachAlternative() = runBlocking {
        val adapter = adapter(speedKph = 18.0)
        val preview = adapter.planRoute(
            RoutePlanRequest(origin = origin, destination = destination, providerId = RouteProviderId.HSL)
        )
        assertTrue("expected at least one alternative", preview.alternatives.isNotEmpty())
        for (alt in preview.alternatives) {
            val distance = alt.normalizedPackage.summary.totalDistanceMeters
            val expected = HslRoutingAdapter.overrideDurationSeconds(
                totalDistanceMeters = distance,
                cyclingSpeedKph = 18.0,
                fallbackSeconds = alt.normalizedPackage.summary.estimatedDurationSeconds,
            )
            assertEquals(
                "summary ETA reflects override",
                expected,
                alt.normalizedPackage.summary.estimatedDurationSeconds
            )
            assertEquals(
                "alternative durationSeconds reflects override",
                expected,
                alt.durationSeconds
            )
        }
    }

    @Test
    fun planRoute_higherSpeedYieldsLowerEtaThanLowerSpeed() = runBlocking {
        val slow = adapter(speedKph = 12.0)
        val fast = adapter(speedKph = 25.0)
        val req = RoutePlanRequest(origin = origin, destination = destination, providerId = RouteProviderId.HSL)
        val slowSec = slow.planRoute(req).alternatives[0].normalizedPackage.summary.estimatedDurationSeconds
        val fastSec = fast.planRoute(req).alternatives[0].normalizedPackage.summary.estimatedDurationSeconds
        assertTrue("higher speed should yield lower ETA: slow=$slowSec fast=$fastSec", fastSec < slowSec)
    }
}
