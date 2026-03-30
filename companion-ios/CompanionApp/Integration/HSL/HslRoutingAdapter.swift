import Foundation

struct HslRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID = .hsl
    let isAvailableInV1: Bool = true

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        let alternatives = [
            RouteAlternative(
                id: UUID(),
                title: "Fastest bike route",
                subtitle: "HSL Digitransit",
                distanceMeters: 5400,
                durationSeconds: 1320
            ),
            RouteAlternative(
                id: UUID(),
                title: "Quieter streets",
                subtitle: "HSL Digitransit",
                distanceMeters: 5900,
                durationSeconds: 1440
            )
        ]
        return RoutePreviewModel(
            alternatives: alternatives,
            selectedAlternativeID: alternatives.first?.id,
            routeIdentifier: "hsl-demo-route",
            routeRevision: 1
        )
    }

    func replanRoute(using session: ActiveRouteSession, riderLocation: CoordinatePoint) async throws -> RoutePreviewModel {
        let fallbackRequest = RoutePlanRequest(
            origin: riderLocation,
            destination: riderLocation,
            providerID: session.providerID
        )
        return try await planRoute(fallbackRequest)
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        let selected = preview.alternatives.first { $0.id == preview.selectedAlternativeID } ?? preview.alternatives.first
        return NormalizedRoutePackage(
            routeIdentifier: preview.routeIdentifier ?? "hsl-preview-route",
            revision: preview.routeRevision ?? 1,
            providerID: request.providerID,
            summary: selected?.title ?? "HSL route",
            geometryPointCount: 42,
            maneuverCount: 8
        )
    }
}
