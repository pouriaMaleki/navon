import Foundation

protocol RoutingProvider {
    var providerID: RouteProviderID { get }
    var isAvailableInV1: Bool { get }
    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel
    func replanRoute(using session: ActiveRouteSession, riderLocation: CoordinatePoint) async throws -> RoutePreviewModel
    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage
}

protocol RouteSyncTransport {
    func scanForDevices() async
    func connectToLastKnownDevice() async
    func publishSet(_ route: NormalizedRoutePackage) async throws
    func publishUpdate(_ route: NormalizedRoutePackage) async throws
    func publishClear(routeIdentifier: String?) async throws
    func receiveStatus(_ message: RouteStatusMessage) async
    func receiveRerouteRequest(_ message: RouteRerouteRequestMessage) async
}

protocol RouteSessionStore {
    func loadRecentDestinations() -> [CoordinatePoint]
    func saveRecentDestination(_ point: CoordinatePoint)
    func loadLastSession() -> ActiveRouteSession?
    func saveSession(_ session: ActiveRouteSession)
}
