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
    func resumePendingTransfer() async throws
    func armRetryableInterruptionOnNextTransfer()
    func armFaultInjection(_ mode: RouteSyncFaultInjectionMode)
    func receiveStatus(_ message: RouteStatusMessage) async
    func receiveRerouteRequest(_ message: RouteRerouteRequestMessage) async
}

protocol RouteSessionStore {
    func loadRecentDestinations() -> [CoordinatePoint]
    func saveRecentDestination(_ point: CoordinatePoint)
    func loadLastSession() -> ActiveRouteSession?
    func saveSession(_ session: ActiveRouteSession)
}

enum LocationErrorKind: String, Equatable {
    case denied
    case unavailable
    case timeout
    case unsupported
}

protocol LocationService: AnyObject {
    var currentLocation: CoordinatePoint? { get }
    var lastKnownLocation: CoordinatePoint? { get }
    var isLocating: Bool { get }
    var lastError: LocationErrorKind? { get }
    /// Begin watching the device's foreground location. Idempotent.
    func start()
    /// Pause watching. Idempotent.
    func stop()
}
