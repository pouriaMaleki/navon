import Foundation

/// Publishes routes to and clears routes from the device via BLE, updating shared session state.
@MainActor
final class RouteSyncService {
    private let bleService: BleRouteSyncService
    private let sessionManager: SessionManager
    private let providers: [RouteProviderID: RoutingProvider]
    private let onRefreshDiagnostics: () -> Void

    init(
        bleService: BleRouteSyncService,
        sessionManager: SessionManager,
        providers: [RouteProviderID: RoutingProvider],
        onRefreshDiagnostics: @escaping () -> Void
    ) {
        self.bleService = bleService
        self.sessionManager = sessionManager
        self.providers = providers
        self.onRefreshDiagnostics = onRefreshDiagnostics
    }

    @discardableResult
    func sendSelectedRoute(
        preview: RoutePreviewModel,
        routeRequest: RoutePlanRequest,
        sourceMode: RouteSourceMode
    ) async -> Bool {
        guard let selected = preview.selectedAlternative else { return false }
        let providerID = selected.normalizedPackage.provenance.providerID
        guard let provider = providers[providerID] else { return false }
        do {
            let normalized = try provider.normalizePreview(preview, request: routeRequest)
            let shouldUpdate = bleService.sessionState.activeRouteIdentifier == normalized.routeIdentifier
                && bleService.sessionState.activeRouteRevision != nil
            if shouldUpdate {
                try await bleService.publishUpdate(normalized)
            } else {
                try await bleService.publishSet(normalized)
            }
            sessionManager.session.routeIdentifier = normalized.routeIdentifier
            sessionManager.session.routeRevision = normalized.revision
            sessionManager.session.destinationLabel = normalized.summary.destinationLabel ?? sessionManager.session.destinationLabel
            sessionManager.session.destinationCoordinate = normalized.geometry.last ?? sessionManager.session.destinationCoordinate
            sessionManager.session.providerID = providerID
            sessionManager.session.sourceMode = sourceMode
            onRefreshDiagnostics()
            return true
        } catch {
            onRefreshDiagnostics()
            return false
        }
    }

    @discardableResult
    func clearActiveRoute() async -> Bool {
        do {
            try await bleService.publishClear(routeIdentifier: sessionManager.session.routeIdentifier)
            sessionManager.session.routeIdentifier = nil
            sessionManager.session.routeRevision = nil
            onRefreshDiagnostics()
            return true
        } catch {
            onRefreshDiagnostics()
            return false
        }
    }
}
