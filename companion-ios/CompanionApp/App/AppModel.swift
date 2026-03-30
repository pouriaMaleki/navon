import Foundation
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    @Published var selectedProviderID: RouteProviderID = .hsl
    @Published var routeRequest = RoutePlanRequest(
        origin: CoordinatePoint(latitude: 60.1699, longitude: 24.9384),
        destination: CoordinatePoint(latitude: 60.1921, longitude: 24.9458),
        providerID: .hsl
    )
    @Published var preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil)
    @Published var activeSession = ActiveRouteSession(
        routeIdentifier: nil,
        routeRevision: nil,
        destinationLabel: "No destination",
        destinationCoordinate: nil,
        providerID: .hsl,
        lastRerouteReason: nil,
        lastRerouteTimestamp: nil
    )

    let diagnosticsStore = CompanionDiagnosticsStore()
    let persistence = CompanionPersistence()
    let bleService = BleRouteSyncService()

    private lazy var providers: [RouteProviderID: RoutingProvider] = [
        .hsl: HslRoutingAdapter()
    ]

    var providerOptions: [RouteProviderID] {
        RouteProviderID.allCases
    }

    var availableProvider: RoutingProvider? {
        providers[selectedProviderID]
    }

    func refreshDiagnostics() {
        diagnosticsStore.update(from: activeSession.routeIdentifier == nil ? nil : activeSession, syncState: bleService.sessionState)
    }

    func planRoute() async {
        routeRequest.providerID = selectedProviderID
        guard let provider = availableProvider else { return }
        do {
            preview = try await provider.planRoute(routeRequest)
            let selectedPackage = preview.selectedAlternative?.normalizedPackage
            activeSession.routeIdentifier = selectedPackage?.routeIdentifier ?? preview.routeIdentifier
            activeSession.routeRevision = selectedPackage?.revision ?? preview.routeRevision
            activeSession.destinationLabel = selectedPackage?.summary.destinationLabel ?? provider.providerID.displayName + " route"
            activeSession.destinationCoordinate = routeRequest.destination
            activeSession.providerID = provider.providerID
            persistence.saveRecentDestination(routeRequest.destination)
            refreshDiagnostics()
        } catch {
            preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil)
        }
    }

    func sendSelectedRoute() async {
        guard let provider = availableProvider else { return }
        do {
            let normalized = try provider.normalizePreview(preview, request: routeRequest)
            let shouldUpdate = bleService.sessionState.activeRouteIdentifier == normalized.routeIdentifier
                && bleService.sessionState.activeRouteRevision != nil
            if shouldUpdate {
                try await bleService.publishUpdate(normalized)
            } else {
                try await bleService.publishSet(normalized)
            }
            activeSession.routeIdentifier = normalized.routeIdentifier
            activeSession.routeRevision = normalized.revision
            activeSession.destinationLabel = normalized.summary.destinationLabel ?? activeSession.destinationLabel
            persistence.saveSession(activeSession)
            refreshDiagnostics()
        } catch {
            refreshDiagnostics()
        }
    }

    func clearActiveRoute() async {
        do {
            try await bleService.publishClear(routeIdentifier: activeSession.routeIdentifier)
            activeSession.routeIdentifier = nil
            activeSession.routeRevision = nil
            refreshDiagnostics()
        } catch {
            refreshDiagnostics()
        }
    }

    func connectToDevice() async {
        await bleService.scanForDevices()
        await bleService.connectToLastKnownDevice()
        refreshDiagnostics()
    }

    func handleDemoRerouteRequest() async {
        guard let provider = availableProvider else { return }
        let routeIdentifier = activeSession.routeIdentifier ?? "preview-route"
        let routeRevision = activeSession.routeRevision ?? 1
        let riderLocation = routeRequest.origin
        await bleService.receiveRerouteRequest(
            RouteRerouteRequestMessage(
                routeIdentifier: routeIdentifier,
                revision: routeRevision,
                riderLocation: riderLocation,
                reason: "User drifted off route"
            )
        )
        do {
            preview = try await provider.replanRoute(using: activeSession, riderLocation: riderLocation)
            activeSession.routeRevision = (activeSession.routeRevision ?? 0) + 1
            activeSession.lastRerouteReason = "Device requested reroute"
            activeSession.lastRerouteTimestamp = Date()
            try await sendSelectedRoute()
        } catch {
            activeSession.lastRerouteReason = "Reroute failed"
            activeSession.lastRerouteTimestamp = Date()
            refreshDiagnostics()
        }
    }
}
