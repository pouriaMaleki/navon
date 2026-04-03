import Foundation
import SwiftUI
import UniformTypeIdentifiers

@MainActor
final class AppModel: ObservableObject {
    @Published var selectedProviderID: RouteProviderID = .hsl
    @Published var settings: CompanionSettings
    @Published var simulatedRiderLocation = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
    @Published var routeRequest = RoutePlanRequest(
        origin: CoordinatePoint(latitude: 60.1699, longitude: 24.9384),
        destination: CoordinatePoint(latitude: 60.1921, longitude: 24.9458),
        providerID: .hsl
    )
    @Published var preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
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
        .hsl: HslRoutingAdapter(settingsProvider: { [unowned self] in self.settings }),
        .osm: SampleRoutingAdapter(providerID: .osm),
        .googleIngest: SampleRoutingAdapter(providerID: .googleIngest),
        .gpxImport: GpxRoutingAdapter(),
        .fitImport: SampleRoutingAdapter(providerID: .fitImport),
        .tcxImport: SampleRoutingAdapter(providerID: .tcxImport),
        .garminApi: SampleRoutingAdapter(providerID: .garminApi),
        .garminFile: SampleRoutingAdapter(providerID: .garminFile)
    ]

    init() {
        settings = persistence.loadSettings()
    }

    var providerOptions: [RouteProviderID] {
        RouteProviderID.allCases
    }

    var availableProvider: RoutingProvider? {
        providers[selectedProviderID]
    }

    var selectedProviderCanPlan: Bool {
        selectedProviderID != .gpxImport && availableProvider != nil
    }

    func refreshDiagnostics() {
        diagnosticsStore.update(from: activeSession.routeIdentifier == nil ? nil : activeSession, syncState: bleService.sessionState)
    }

    func persistSettings() {
        persistence.saveSettings(settings)
    }

    func importGpxFile(from url: URL) async {
        do {
            let accessGranted = url.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    url.stopAccessingSecurityScopedResource()
                }
            }
            let data = try Data(contentsOf: url)
            guard let adapter = providers[.gpxImport] as? GpxRoutingAdapter else { return }
            let preview = try adapter.importFile(named: url.lastPathComponent, data: data)
            self.selectedProviderID = .gpxImport
            self.preview = preview
            if let selected = preview.selectedAlternative?.normalizedPackage {
                routeRequest = RoutePlanRequest(
                    origin: selected.geometry.first ?? routeRequest.origin,
                    destination: selected.geometry.last ?? routeRequest.destination,
                    providerID: .gpxImport
                )
                simulatedRiderLocation = selected.geometry.first ?? simulatedRiderLocation
            }
            applySelectedAlternativeToSession(providerID: .gpxImport, destination: routeRequest.destination)
            refreshDiagnostics()
        } catch {
            preview = RoutePreviewModel(
                alternatives: [],
                selectedAlternativeID: nil,
                routeIdentifier: nil,
                routeRevision: nil,
                planningNotice: "GPX import failed: \(error.localizedDescription)"
            )
        }
    }

    func planRoute() async {
        routeRequest.providerID = selectedProviderID
        guard let provider = availableProvider else { return }
        do {
            preview = try await provider.planRoute(routeRequest)
            applySelectedAlternativeToSession(providerID: provider.providerID, destination: routeRequest.destination)
            simulatedRiderLocation = routeRequest.origin
            persistence.saveRecentDestination(routeRequest.destination)
            refreshDiagnostics()
        } catch {
            preview = RoutePreviewModel(
                alternatives: [],
                selectedAlternativeID: nil,
                routeIdentifier: nil,
                routeRevision: nil,
                planningNotice: "Planning failed: \(error.localizedDescription)"
            )
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

    func selectAlternative(_ alternativeID: UUID) {
        preview.selectedAlternativeID = alternativeID
        preview.routeIdentifier = preview.selectedAlternative?.normalizedPackage.routeIdentifier
        preview.routeRevision = preview.selectedAlternative?.normalizedPackage.revision
        applySelectedAlternativeToSession(providerID: selectedProviderID, destination: routeRequest.destination)
        refreshDiagnostics()
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

    func resumePendingTransfer() async {
        do {
            try await bleService.resumePendingTransfer()
            refreshDiagnostics()
        } catch {
            refreshDiagnostics()
        }
    }

    func armRetryableInterruptionOnNextTransfer() {
        bleService.armRetryableInterruptionOnNextTransfer()
        refreshDiagnostics()
    }

    func armWriteFailureOnNextTransfer() {
        bleService.armFaultInjection(.writeFailure)
        refreshDiagnostics()
    }

    func armDisconnectAfterNextChunkWrite() {
        bleService.armFaultInjection(.disconnectAfterChunkWrite)
        refreshDiagnostics()
    }

    func armDropNextInboundStatus() {
        bleService.armFaultInjection(.dropNextInboundStatus)
        refreshDiagnostics()
    }

    func connectToDevice() async {
        await bleService.scanForDevices()
        await bleService.connectToLastKnownDevice()
        refreshDiagnostics()
    }

    func handleRerouteRequest() async {
        guard let provider = availableProvider else { return }
        let routeIdentifier = activeSession.routeIdentifier ?? "preview-route"
        let riderLocation = simulatedRiderLocation
        await bleService.receiveRerouteRequest(
            RouteRerouteRequestMessage(
                routeIdentifier: routeIdentifier,
                riderLocation: riderLocation,
                reason: "User drifted off route"
            )
        )
        do {
            preview = try await provider.replanRoute(using: activeSession, riderLocation: riderLocation)
            applySelectedAlternativeToSession(providerID: provider.providerID, destination: activeSession.destinationCoordinate ?? routeRequest.destination)
            activeSession.routeRevision = preview.selectedAlternative?.normalizedPackage.revision ?? ((activeSession.routeRevision ?? 0) + 1)
            activeSession.lastRerouteReason = "Device requested reroute"
            activeSession.lastRerouteTimestamp = Date()
            try await sendSelectedRoute()
        } catch {
            activeSession.lastRerouteReason = "Reroute failed"
            activeSession.lastRerouteTimestamp = Date()
            refreshDiagnostics()
        }
    }

    private func applySelectedAlternativeToSession(providerID: RouteProviderID, destination: CoordinatePoint) {
        let selectedPackage = preview.selectedAlternative?.normalizedPackage
        activeSession.routeIdentifier = selectedPackage?.routeIdentifier ?? preview.routeIdentifier
        activeSession.routeRevision = selectedPackage?.revision ?? preview.routeRevision
        activeSession.destinationLabel = selectedPackage?.summary.destinationLabel ?? providerID.displayName + " route"
        activeSession.destinationCoordinate = selectedPackage?.geometry.last ?? destination
        activeSession.providerID = providerID
    }
}
