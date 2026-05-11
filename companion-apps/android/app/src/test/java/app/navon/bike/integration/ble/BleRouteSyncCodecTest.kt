package app.navon.bike.integration.ble

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.domain.RoutePackageVersion
import app.navon.bike.domain.RouteProvenance
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RouteSetMessage
import app.navon.bike.domain.RouteSyncMessage
import app.navon.bike.domain.RouteSummary

class BleRouteSyncCodecTest {
    @Test
    fun roundTripsSetMessageThroughPacketCodec() {
        val route = sampleRoute()
        val packet = BleRouteSyncPacket.SyncMessage(RouteSyncMessage.Set(RouteSetMessage(route)))

        val decoded = BleRouteSyncCodec.decode(BleRouteSyncCodec.encode(packet)) as BleRouteSyncPacket.SyncMessage
        val decodedRoute = (decoded.message as RouteSyncMessage.Set).message.route

        assertEquals(route, decodedRoute)
    }

    @Test
    fun chunkEnvelopesCoverEntirePayloadAndReuseChecksum() {
        val message = RouteSyncMessage.Set(RouteSetMessage(sampleRoute()))

        val payload = BleRouteSyncCodec.canonicalPayloadBytes(message)
        val chunks = BleRouteSyncCodec.chunkEnvelopes(
            message = message,
            transferIdentifier = "transfer-1",
            chunkSizeBytes = 16,
        )

        assertEquals("transfer-1", chunks.first().transferIdentifier)
        assertEquals(BleRouteSyncCodec.checksumHex(payload), chunks.first().checksumHex)
        assertEquals(chunks.first().checksumHex, chunks.last().checksumHex)
        assertEquals(payload.size, chunks.sumOf { it.payloadFragment.size })
        assertEquals(chunks.size, chunks.first().totalChunks)
        assertEquals(chunks.indices.toList(), chunks.map { it.chunkIndex })
    }

    @Test
    fun clearMessageDropsCurrentSentinelWhenDecoded() {
        val message = RouteSyncMessage.Clear(app.navon.bike.domain.RouteClearMessage(null))

        val decoded = BleRouteSyncCodec.decodeSyncMessage(BleRouteSyncCodec.canonicalPayloadBytes(message))

        val clear = decoded as RouteSyncMessage.Clear
        assertNull(clear.message.routeIdentifier)
    }

    private fun sampleRoute(): NormalizedRoutePackage =
        NormalizedRoutePackage(
            version = RoutePackageVersion.CURRENT,
            routeIdentifier = "route-123",
            revision = 7,
            geometry = listOf(
                CoordinatePoint(60.1699, 24.9384),
                CoordinatePoint(60.1705, 24.9412),
                CoordinatePoint(60.1710, 24.9450),
            ),
            maneuvers = listOf(
                RouteManeuver(
                    id = "m1",
                    maneuverType = RouteManeuverType.DEPART,
                    location = CoordinatePoint(60.1699, 24.9384),
                    distanceFromStartMeters = 0.0,
                    distanceToNextMeters = 120.5,
                    instructionText = "Head north",
                ),
                RouteManeuver(
                    id = "m2",
                    maneuverType = RouteManeuverType.RIGHT,
                    location = CoordinatePoint(60.1705, 24.9412),
                    distanceFromStartMeters = 120.5,
                    distanceToNextMeters = null,
                    instructionText = "Turn right",
                ),
            ),
            summary = RouteSummary(
                totalDistanceMeters = 420.5,
                estimatedDurationSeconds = 180,
                startLabel = "Start",
                destinationLabel = "Finish",
            ),
            provenance = RouteProvenance(
                providerId = RouteProviderId.HSL,
                sourceReference = "fixture",
                generatedAtUnixMs = 1_712_345_678_000,
            ),
        )
}
