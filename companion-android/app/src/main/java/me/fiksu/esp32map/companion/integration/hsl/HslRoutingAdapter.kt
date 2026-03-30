package me.fiksu.esp32map.companion.integration.hsl

import java.util.UUID
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RoutingProvider

class HslRoutingAdapter : RoutingProvider {
    override val providerId: RouteProviderId = RouteProviderId.HSL
    override val isAvailableInV1: Boolean = true

    override suspend fun planRoute(request: RoutePlanRequest): RoutePreviewModel {
        val alternatives = listOf(
            RouteAlternative(
                id = UUID.randomUUID().toString(),
                title = "Fastest bike route",
                subtitle = "HSL Digitransit",
                distanceMeters = 5400,
                durationSeconds = 1320,
            ),
            RouteAlternative(
                id = UUID.randomUUID().toString(),
                title = "Quieter streets",
                subtitle = "HSL Digitransit",
                distanceMeters = 5900,
                durationSeconds = 1440,
            ),
        )
        return RoutePreviewModel(
            alternatives = alternatives,
            selectedAlternativeId = alternatives.firstOrNull()?.id,
            routeIdentifier = "hsl-demo-route",
            routeRevision = 1,
        )
    }

    override suspend fun replanRoute(session: ActiveRouteSession, riderLocation: CoordinatePoint): RoutePreviewModel {
        return planRoute(
            RoutePlanRequest(
                origin = riderLocation,
                destination = riderLocation,
                providerId = session.providerId,
            ),
        )
    }

    override fun normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
        val selected = preview.alternatives.firstOrNull { it.id == preview.selectedAlternativeId } ?: preview.alternatives.firstOrNull()
        return NormalizedRoutePackage(
            routeIdentifier = preview.routeIdentifier ?: "hsl-preview-route",
            revision = preview.routeRevision ?: 1,
            providerId = request.providerId,
            summary = selected?.title ?: "HSL route",
            geometryPointCount = 42,
            maneuverCount = 8,
        )
    }
}
