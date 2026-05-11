package app.navon.bike.flows

import app.navon.bike.domain.ActiveRouteSession
import app.navon.bike.domain.CompanionSettings
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RouteSourceMode
import app.navon.bike.domain.RerouteContext
import app.navon.bike.integration.hsl.HslRoutingAdapter
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class HslHeadingRerouteTest {
    private val origin = CoordinatePoint(60.1699, 24.9384)
    private val destination = CoordinatePoint(60.1921, 24.9458)

    private fun adapter(): HslRoutingAdapter = HslRoutingAdapter(
        settingsProvider = {
            CompanionSettings(
                preferLiveHslRouting = false,
                hslSubscriptionKey = "",
            )
        },
    )

    private fun session() = ActiveRouteSession(
        routeIdentifier = "r1",
        routeRevision = 1,
        destinationLabel = "Dest",
        destinationCoordinate = destination,
        providerId = RouteProviderId.HSL,
        sourceMode = RouteSourceMode.HSL,
    )

    @Test
    fun replanRoute_appliesHeadingBiasWhenSpeedIsHigh() = runTest {
        val preview = adapter().replanRoute(
            session = session(),
            riderLocation = origin,
            rerouteContext = RerouteContext(headingDegrees = 90.0, speedMps = 4.0),
        )
        assertNotEquals(origin, preview.alternatives.first().normalizedPackage.geometry.first())
    }

    @Test
    fun replanRoute_keepsLegacyOriginWhenSpeedIsLow() = runTest {
        val preview = adapter().replanRoute(
            session = session(),
            riderLocation = origin,
            rerouteContext = RerouteContext(headingDegrees = 90.0, speedMps = 0.5),
        )
        assertEquals(origin, preview.alternatives.first().normalizedPackage.geometry.first())
    }
}
