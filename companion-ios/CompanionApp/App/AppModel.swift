import Foundation
import SwiftUI
import UniformTypeIdentifiers
import Combine
import os.log

/// Pairing-flow log channel. Filter Console.app with
/// `subsystem:me.fiksu.esp32map.companion.ios category:pairing` to follow
/// each step (begin → scan → connect → write → persist).
let pairingLog = Logger(subsystem: "me.fiksu.esp32map.companion.ios", category: "pairing")

@MainActor
final class AppModel: ObservableObject {
    static let defaultRiderFallback = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)

    @Published var selectedProviderID: RouteProviderID = .hsl
    @Published var currentSourceMode: RouteSourceMode
    @Published var settings: CompanionSettings
    @Published var routeRequest = RoutePlanRequest(
        origin: CoordinatePoint(latitude: 60.1699, longitude: 24.9384),
        destination: CoordinatePoint(latitude: 60.1921, longitude: 24.9458),
        providerID: .hsl
    )

    /// Best estimate of the rider's current position. Sourced from CoreLocation; falls back
    /// to the last persisted fix and finally to a static default so the planner stays usable
    /// when permission is denied.
    var riderLocation: CoordinatePoint {
        locationService.currentLocation
            ?? locationService.lastKnownLocation
            ?? Self.defaultRiderFallback
    }
    @Published var preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
    @Published var activeSession = ActiveRouteSession(
        routeIdentifier: nil,
        routeRevision: nil,
        destinationLabel: "No destination",
        destinationCoordinate: nil,
        providerID: .hsl,
        sourceMode: .mixed,
        lastRerouteReason: nil,
        lastRerouteTimestamp: nil
    )
    @Published var importActivityStatus: String?
    @Published var homePreviewRequestID = UUID()
    @Published var homeStartRequestID = UUID()
    @Published private(set) var persistenceRevision = 0
    @Published private(set) var pairedPeripheral: PairedPeripheralRecord?
    @Published var pairingState: PairingFlowState = .idle

    let diagnosticsStore = CompanionDiagnosticsStore()
    let persistence: CompanionPersistence
    let bleService: BleRouteSyncService
    let locationService: CoreLocationService

    private var cancellables = Set<AnyCancellable>()
    private var lastHandledRerouteSignature: String?

    private lazy var providers: [RouteProviderID: RoutingProvider] = [
        .hsl: HslRoutingAdapter(settingsProvider: { [unowned self] in self.settings }),
        .osm: OsmCyclingRoutingAdapter(),
        .gpxImport: GpxRoutingAdapter(),
        .fitImport: SampleRoutingAdapter(providerID: .fitImport),
        .tcxImport: SampleRoutingAdapter(providerID: .tcxImport),
    ]

    init(persistence: CompanionPersistence = CompanionPersistence(), bleService: BleRouteSyncService? = nil) {
        // All stored properties must be initialized before any self-method call.
        self.persistence = persistence
        self.bleService = bleService ?? BleRouteSyncService()
        let locationService = CoreLocationService(persistence: persistence)
        self.locationService = locationService

        settings = persistence.loadSettings()
        let preferences = persistence.loadRoutePlannerPreferences()
        currentSourceMode = preferences.defaultSourceMode
        if let storedSession = persistence.loadLastSession() {
            activeSession = storedSession
        }
        selectedProviderID = activeSession.providerID
        pairedPeripheral = persistence.loadPairedPeripheral()
        if let initial = locationService.currentLocation ?? locationService.lastKnownLocation {
            routeRequest.origin = initial
        }
        bindBleState()
        bindLocationService()
        locationService.start()
    }

    /// Drop the bonded peripheral. Clears in-memory + persistence and the
    /// pairing UI state machine so a future reconnect requires a fresh QR
    /// scan. Single-bond is enforced here and on the firmware side.
    func forgetPairedDevice() {
        pairedPeripheral = nil
        persistence.clearPairedPeripheral()
        pairingState = .idle
    }

    /// Entry point invoked by the home-screen pair chip / Settings CTA.
    /// Surfaces the instructions step before the camera sheet opens so the
    /// user knows what to do with the QR.
    func beginPairingFlow() {
        pairingLog.notice("beginPairingFlow tapped — pairingState → .instructions")
        pairingState = .instructions
    }

    /// Drive the pairing flow's BLE half: scan for the route-sync service,
    /// connect to whichever peripheral advertises it, write the OOB
    /// confirmation secret, and persist the bond on success. On any failure
    /// we leave persistence untouched so the user can retry without the app
    /// holding a half-state record.
    ///
    /// The persisted identifier comes from CoreBluetooth at connect time —
    /// the QR carries `id_android` (BD_ADDR) which iOS cannot use directly,
    /// so iOS captures `peripheral.identifier.uuidString` instead.
    ///
    /// Auto-dismiss timing: 1.5 s after `.succeeded` we drop back to `.idle`.
    /// Tests can drive this synchronously by waiting on `pairingState`.
    func completePairing(payload: PairingQrPayload) async {
        pairingLog.notice("completePairing — connecting (secret \(payload.ephemeralSecret.count, privacy: .public) B)")
        pairingState = .connecting
        do {
            let info = try await bleService.connectToAdvertisedPeripheral()
            pairingLog.notice("completePairing — connected to \(info.name, privacy: .public) [\(info.identifier, privacy: .public)]")
            pairingState = .confirming
            try await bleService.writePairingConfirm(secret: payload.ephemeralSecret)
            pairingLog.notice("completePairing — pairing-confirm write OK")
            let record = PairedPeripheralRecord(
                identifier: info.identifier,
                friendlyName: info.name,
                pairedAt: Date()
            )
            persistence.savePairedPeripheral(record)
            pairedPeripheral = record
            pairingState = .succeeded
            try? await Task.sleep(nanoseconds: 1_500_000_000)
            // Only auto-dismiss if no later action moved the state already
            // (e.g. user cancelled, error fired). Avoids stomping on it.
            if pairingState == .succeeded {
                pairingState = .idle
            }
        } catch {
            pairingLog.error("completePairing failed: \(error.localizedDescription, privacy: .public)")
            pairingState = .failed(error.localizedDescription)
        }
    }

    var providerOptions: [RouteProviderID] {
        RouteProviderID.allCases
    }

    /// True only when the user has enabled live HSL routing AND configured a Digitransit key.
    /// Mirrors `companion-web` `SettingsStore.isHslLiveConfigured`.
    var isHslLiveConfigured: Bool {
        settings.preferLiveHslRouting
            && !settings.hslSubscriptionKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// True when both endpoints of the current request fall inside the Uusimaa region of
    /// Finland (HSL Digitransit's coverage area).
    var isHslApplicableForRequest: Bool {
        AppModel.isInUusimaa(routeRequest.origin) && AppModel.isInUusimaa(routeRequest.destination)
    }

    /// True when HSL is both configured AND geographically usable for the current request.
    var isHslAvailable: Bool { isHslLiveConfigured && isHslApplicableForRequest }

    /// Source-mode tabs visible in the UI. With no Digitransit key, or when either endpoint
    /// is outside Uusimaa, mixed/HSL collapse to OSM (the picker hides itself when there is
    /// only one option).
    var sourceModeOptions: [RouteSourceMode] {
        isHslAvailable ? RouteSourceMode.allCases : [.osm]
    }

    /// Approximate bounding box for the Uusimaa region of Finland (Helsinki, Espoo,
    /// Vantaa, Porvoo, Hanko, Loviisa, etc.).
    static func isInUusimaa(_ point: CoordinatePoint) -> Bool {
        (59.8...60.8).contains(point.latitude) && (23.3...26.7).contains(point.longitude)
    }

    var isDeviceConnected: Bool {
        bleService.sessionState.connectionState == .connected
    }

    func provider(for providerID: RouteProviderID) -> RoutingProvider? {
        providers[providerID]
    }

    func refreshDiagnostics() {
        diagnosticsStore.update(from: activeSession.routeIdentifier == nil ? nil : activeSession, syncState: bleService.sessionState)
    }

    func notePersistenceChanged() {
        persistenceRevision &+= 1
    }

    func persistSettings() {
        persistence.saveSettings(settings)
        normalizeSourceModeForHslAvailability()
    }

    /// When HSL becomes unusable (no key OR endpoints outside Uusimaa), fall back any
    /// HSL-only or Mixed active selections to OSM. Persisted defaults are also normalised
    /// when the underlying *configuration* (the key) is gone, so a relaunch is consistent.
    func normalizeSourceModeForHslAvailability() {
        if !isHslAvailable && currentSourceMode != .osm {
            currentSourceMode = .osm
        }
        guard !isHslLiveConfigured else { return }
        var preferences = persistence.loadRoutePlannerPreferences()
        if preferences.defaultSourceMode != .osm {
            preferences = RoutePlannerPreferences(
                defaultSourceMode: .osm,
                suggestionMode: preferences.suggestionMode,
                startBehavior: preferences.startBehavior
            )
            persistence.saveRoutePlannerPreferences(preferences)
        }
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
            selectedProviderID = .gpxImport
            self.preview = preview
            if let selected = preview.selectedAlternative?.normalizedPackage {
                routeRequest = RoutePlanRequest(
                    origin: selected.geometry.first ?? routeRequest.origin,
                    destination: selected.geometry.last ?? routeRequest.destination,
                    providerID: .gpxImport
                )
            }
            applySelectedAlternativeToSession(sourceMode: currentSourceMode, destination: routeRequest.destination, preferredTitle: url.deletingPathExtension().lastPathComponent)
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

    func importSampleFile(from url: URL, providerID: RouteProviderID, preferredTitle: String? = nil) async throws {
        let accessGranted = url.startAccessingSecurityScopedResource()
        defer {
            if accessGranted {
                url.stopAccessingSecurityScopedResource()
            }
        }

        let data = try Data(contentsOf: url)
        guard let adapter = providers[providerID] as? SampleRoutingAdapter else { return }
        let preview = try adapter.importFile(
            named: url.lastPathComponent,
            data: data,
            origin: riderLocation
        )
        selectedProviderID = providerID
        self.preview = preview
        if let selected = preview.selectedAlternative?.normalizedPackage {
            routeRequest = RoutePlanRequest(
                origin: selected.geometry.first ?? routeRequest.origin,
                destination: selected.geometry.last ?? routeRequest.destination,
                providerID: providerID
            )
        }
        applySelectedAlternativeToSession(
            sourceMode: currentSourceMode,
            destination: routeRequest.destination,
            preferredTitle: preferredTitle ?? url.deletingPathExtension().lastPathComponent
        )
        refreshDiagnostics()
    }

    func planRoute() async {
        await planRoute(using: currentSourceMode)
    }

    func planRoute(using sourceMode: RouteSourceMode, preferredTitle: String? = nil, revisionOverride: Int? = nil) async {
        // If HSL isn't available for this trip (no key OR endpoints outside Uusimaa),
        // collapse mixed/HSL down to OSM before planning so we don't race a useless provider.
        let effectiveMode: RouteSourceMode = (!isHslAvailable && sourceMode != .osm) ? .osm : sourceMode
        currentSourceMode = effectiveMode
        routeRequest.providerID = effectiveMode.primaryProviderID
        do {
            preview = try await buildPreview(for: routeRequest, sourceMode: effectiveMode, revisionOverride: revisionOverride)
            persistence.saveRecentDestination(routeRequest.destination)
            notePersistenceChanged()
            applySelectedAlternativeToSession(sourceMode: effectiveMode, destination: routeRequest.destination, preferredTitle: preferredTitle)
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

    @discardableResult
    func sendSelectedRoute() async -> Bool {
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
            activeSession.routeIdentifier = normalized.routeIdentifier
            activeSession.routeRevision = normalized.revision
            activeSession.destinationLabel = normalized.summary.destinationLabel ?? activeSession.destinationLabel
            activeSession.destinationCoordinate = normalized.geometry.last ?? activeSession.destinationCoordinate
            activeSession.providerID = providerID
            activeSession.sourceMode = currentSourceMode
            persistence.saveSession(activeSession)
            refreshDiagnostics()
            return true
        } catch {
            refreshDiagnostics()
            return false
        }
    }

    @discardableResult
    func clearActiveRoute() async -> Bool {
        do {
            try await bleService.publishClear(routeIdentifier: activeSession.routeIdentifier)
            activeSession.routeIdentifier = nil
            activeSession.routeRevision = nil
            persistence.saveSession(activeSession)
            refreshDiagnostics()
            return true
        } catch {
            refreshDiagnostics()
            return false
        }
    }

    func selectAlternative(_ alternativeID: UUID) {
        preview.selectedAlternativeID = alternativeID
        preview.routeIdentifier = preview.selectedAlternative?.normalizedPackage.routeIdentifier
        preview.routeRevision = preview.selectedAlternative?.normalizedPackage.revision
        if let providerID = preview.selectedAlternative?.normalizedPackage.provenance.providerID {
            selectedProviderID = providerID
        }
        applySelectedAlternativeToSession(sourceMode: currentSourceMode, destination: routeRequest.destination, preferredTitle: nil)
        refreshDiagnostics()
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
        if let paired = pairedPeripheral {
            await bleService.connectToPairedPeripheral(identifier: paired.identifier)
            if bleService.sessionState.connectionState == .connected {
                refreshDiagnostics()
                return
            }
        }
        await bleService.scanForDevices()
        await bleService.connectToLastKnownDevice()
        refreshDiagnostics()
    }

    func applyRouteHistoryPreview(_ item: RouteHistoryItem) async {
        if let package = item.routePackage {
            let alternative = RouteAlternative(
                id: UUID(),
                title: item.title,
                subtitle: item.subtitle,
                distanceMeters: Int(package.summary.totalDistanceMeters.rounded()),
                durationSeconds: package.summary.estimatedDurationSeconds,
                normalizedPackage: package
            )
            preview = RoutePreviewModel(
                alternatives: [alternative],
                selectedAlternativeID: alternative.id,
                routeIdentifier: package.routeIdentifier,
                routeRevision: package.revision,
                planningNotice: item.sourceLabel
            )
            selectedProviderID = package.provenance.providerID
            if package.provenance.providerID == .osm {
                currentSourceMode = .osm
            } else if package.provenance.providerID == .hsl {
                currentSourceMode = .hsl
            }
            routeRequest = RoutePlanRequest(
                origin: riderLocation,
                destination: item.destination ?? package.geometry.last ?? routeRequest.destination,
                providerID: package.provenance.providerID
            )
            let sessionSourceMode = package.provenance.providerID == .osm ? RouteSourceMode.osm : currentSourceMode
            applySelectedAlternativeToSession(sourceMode: sessionSourceMode, destination: routeRequest.destination, preferredTitle: item.title)
        } else if let destination = item.destination {
            routeRequest = RoutePlanRequest(origin: riderLocation, destination: destination, providerID: currentSourceMode.primaryProviderID)
            await planRoute(using: currentSourceMode, preferredTitle: item.title)
        }
    }

    func handleRerouteRequest() async {
        await rerouteActiveSession(from: riderLocation, reason: "Device requested reroute")
    }

    func rerouteActiveSession(from riderLocation: CoordinatePoint, reason: String) async {
        guard activeSession.destinationCoordinate != nil else { return }
        let routeIdentifier = activeSession.routeIdentifier ?? preview.routeIdentifier ?? "preview-route"
        await bleService.receiveRerouteRequest(
            RouteRerouteRequestMessage(
                routeIdentifier: routeIdentifier,
                riderLocation: riderLocation,
                reason: reason
            )
        )

        routeRequest = RoutePlanRequest(
            origin: riderLocation,
            destination: activeSession.destinationCoordinate ?? routeRequest.destination,
            providerID: activeSession.sourceMode.primaryProviderID
        )
        await planRoute(using: activeSession.sourceMode, preferredTitle: activeSession.destinationLabel, revisionOverride: (activeSession.routeRevision ?? 0) + 1)
        activeSession.lastRerouteReason = reason
        activeSession.lastRerouteTimestamp = Date()
        _ = await sendSelectedRoute()
    }

    private func bindLocationService() {
        // Forward location updates to AppModel so SwiftUI views observing AppModel re-render
        // when riderLocation changes, and refresh the planning origin if the user has not
        // typed a destination yet.
        locationService.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] in
                guard let self else { return }
                self.objectWillChange.send()
            }
            .store(in: &cancellables)

        locationService.$currentLocation
            .receive(on: RunLoop.main)
            .sink { [weak self] point in
                guard let self, let point else { return }
                self.routeRequest.origin = point
            }
            .store(in: &cancellables)
    }

    private func bindBleState() {
        // SwiftUI observes AppModel via @EnvironmentObject. `bleService` is a
        // nested ObservableObject held in a plain `let`, so its @Published
        // updates don't reach views that read `appModel.bleService.*`. Forward
        // its `objectWillChange` so the device-settings UI re-renders when the
        // BLE session state moves between scanning / connecting / connected /
        // disconnected — without this, "Reconnect" appears to do nothing.
        bleService.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] in
                self?.objectWillChange.send()
            }
            .store(in: &cancellables)
        bleService.$sessionState
            .receive(on: RunLoop.main)
            .sink { [weak self] state in
                guard let self else { return }
                self.refreshDiagnostics()
                guard case let .rerouteRequest(message)? = state.lastInboundMessage else { return }
                let signature = "\(message.routeIdentifier)-\(message.riderLocation.latitude)-\(message.riderLocation.longitude)-\(message.reason)"
                guard self.lastHandledRerouteSignature != signature else { return }
                self.lastHandledRerouteSignature = signature
                Task { @MainActor in
                    await self.rerouteActiveSession(from: message.riderLocation, reason: message.reason)
                }
            }
            .store(in: &cancellables)
    }

    private func buildPreview(
        for request: RoutePlanRequest,
        sourceMode: RouteSourceMode,
        revisionOverride: Int?
    ) async throws -> RoutePreviewModel {
        switch sourceMode {
        case .mixed:
            return try await buildMixedPreview(for: request, revisionOverride: revisionOverride)
        case .hsl, .osm:
            guard let provider = providers[sourceMode.primaryProviderID] else {
                throw NSError(domain: "AppModel", code: 1, userInfo: [NSLocalizedDescriptionKey: "Missing provider for \(sourceMode.displayName)"])
            }
            var preview = try await preview(from: provider, request: request, revisionOverride: revisionOverride)
            preview.alternatives = presentAlternatives(preview.alternatives, sourceMode: sourceMode)
            preview.selectedAlternativeID = preview.alternatives.first?.id
            preview.routeIdentifier = preview.alternatives.first?.normalizedPackage.routeIdentifier
            preview.routeRevision = preview.alternatives.first?.normalizedPackage.revision
            return preview
        }
    }

    private func preview(from provider: RoutingProvider, request: RoutePlanRequest, revisionOverride: Int?) async throws -> RoutePreviewModel {
        if let revisionOverride {
            let session = ActiveRouteSession(
                routeIdentifier: activeSession.routeIdentifier,
                routeRevision: revisionOverride - 1,
                destinationLabel: activeSession.destinationLabel,
                destinationCoordinate: request.destination,
                providerID: provider.providerID,
                sourceMode: currentSourceMode,
                lastRerouteReason: activeSession.lastRerouteReason,
                lastRerouteTimestamp: activeSession.lastRerouteTimestamp
            )
            return try await provider.replanRoute(using: session, riderLocation: request.origin)
        }
        return try await provider.planRoute(request)
    }

    private func buildMixedPreview(for request: RoutePlanRequest, revisionOverride: Int?) async throws -> RoutePreviewModel {
        guard let osm = providers[.osm] else {
            throw NSError(domain: "AppModel", code: 2, userInfo: [NSLocalizedDescriptionKey: "Mixed mode providers are unavailable"])
        }
        // Skip the HSL race when HSL is unavailable (no key OR endpoints outside Uusimaa).
        let includeHsl = isHslAvailable
        async let osmPreview = preview(from: osm, request: request, revisionOverride: revisionOverride)
        let previews: [RoutePreviewModel]
        if includeHsl, let hsl = providers[.hsl] {
            async let hslPreview = preview(from: hsl, request: request, revisionOverride: revisionOverride)
            previews = try await [hslPreview, osmPreview]
        } else {
            previews = try await [osmPreview]
        }
        let effectivePreviews = preferredMixedPreviews(from: previews)
        let merged = mergeMixedAlternatives(effectivePreviews.flatMap(\.alternatives))
        return RoutePreviewModel(
            alternatives: merged,
            selectedAlternativeID: merged.first?.id,
            routeIdentifier: merged.first?.normalizedPackage.routeIdentifier,
            routeRevision: merged.first?.normalizedPackage.revision,
            planningNotice: mixedPlanningNotice(from: previews, effectivePreviews: effectivePreviews)
        )
    }

    private func preferredMixedPreviews(from previews: [RoutePreviewModel]) -> [RoutePreviewModel] {
        let livePreviews = previews.filter { !isSamplePreview($0) && !$0.alternatives.isEmpty }
        if !livePreviews.isEmpty {
            return livePreviews
        }
        return previews.filter { !$0.alternatives.isEmpty }
    }

    private func isSamplePreview(_ preview: RoutePreviewModel) -> Bool {
        guard let notice = preview.planningNotice?.lowercased() else { return false }
        return notice.contains("sample")
    }

    private func mixedPlanningNotice(from previews: [RoutePreviewModel], effectivePreviews: [RoutePreviewModel]) -> String {
        if effectivePreviews.count == 1, let notice = effectivePreviews.first?.planningNotice, !notice.isEmpty {
            return notice
        }
        if effectivePreviews.count < previews.count {
            return "Showing live routes while sample fallback providers are hidden."
        }
        return "Mixed routes from HSL and OSM"
    }

    private func mergeMixedAlternatives(_ alternatives: [RouteAlternative]) -> [RouteAlternative] {
        guard !alternatives.isEmpty else { return [] }
        var remaining = alternatives.sorted {
            if $0.durationSeconds == $1.durationSeconds {
                return $0.distanceMeters < $1.distanceMeters
            }
            return $0.durationSeconds < $1.durationSeconds
        }

        var chosen: [RouteAlternative] = []
        if let fastest = remaining.first {
            chosen.append(fastest)
            remaining.removeAll { $0.normalizedPackage.routeIdentifier == fastest.normalizedPackage.routeIdentifier }
        }

        if let quieter = remaining.first(where: { $0.normalizedPackage.provenance.providerID == .osm }) ?? remaining.first {
            chosen.append(quieter)
            remaining.removeAll { $0.normalizedPackage.routeIdentifier == quieter.normalizedPackage.routeIdentifier }
        }

        if let simpler = remaining.min(by: { $0.normalizedPackage.maneuverCount < $1.normalizedPackage.maneuverCount }) {
            chosen.append(simpler)
            remaining.removeAll { $0.normalizedPackage.routeIdentifier == simpler.normalizedPackage.routeIdentifier }
        }

        while chosen.count < 3, let next = remaining.first {
            chosen.append(next)
            remaining.removeFirst()
        }

        return presentAlternatives(chosen, sourceMode: .mixed)
    }

    /// Label every visible alternative as "<Provider> Route N", where N is
    /// a per-provider counter (so OSM Route 1, OSM Route 2, HSL Route 1,
    /// …). Replaces the prior "Fastest / Quieter / Simpler" scheme which
    /// implied semantics the routing backends don't deliver — the order
    /// is just whatever the provider returned.
    private func presentAlternatives(_ alternatives: [RouteAlternative], sourceMode: RouteSourceMode) -> [RouteAlternative] {
        var counters: [RouteProviderID: Int] = [:]
        return alternatives.prefix(3).map { alternative in
            let providerID = alternative.normalizedPackage.provenance.providerID
            let providerLabel = providerID.displayName
            let next = (counters[providerID] ?? 0) + 1
            counters[providerID] = next
            return RouteAlternative(
                id: alternative.id,
                title: "\(providerLabel) Route \(next)",
                subtitle: alternative.normalizedPackage.provenance.sourceReference ?? "via \(providerLabel)",
                distanceMeters: alternative.distanceMeters,
                durationSeconds: alternative.durationSeconds,
                normalizedPackage: alternative.normalizedPackage
            )
        }
    }

    private func applySelectedAlternativeToSession(sourceMode: RouteSourceMode, destination: CoordinatePoint, preferredTitle: String?) {
        let selectedPackage = preview.selectedAlternative?.normalizedPackage
        let providerID = selectedPackage?.provenance.providerID ?? sourceMode.primaryProviderID
        selectedProviderID = providerID
        activeSession.routeIdentifier = selectedPackage?.routeIdentifier ?? preview.routeIdentifier
        activeSession.routeRevision = selectedPackage?.revision ?? preview.routeRevision
        activeSession.destinationLabel = displayDestinationTitle(
            selectedPackage: selectedPackage,
            preferredTitle: preferredTitle,
            fallback: providerID.displayName + " route"
        )
        activeSession.destinationCoordinate = selectedPackage?.geometry.last ?? destination
        activeSession.providerID = providerID
        activeSession.sourceMode = sourceMode
    }

    private func displayDestinationTitle(selectedPackage: NormalizedRoutePackage?, preferredTitle: String?, fallback: String) -> String {
        if let preferredTitle {
            let trimmed = preferredTitle.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                return trimmed
            }
        }
        if let packageTitle = selectedPackage?.summary.destinationLabel,
           !isGenericDestinationTitle(packageTitle, providerID: selectedPackage?.provenance.providerID) {
            return packageTitle
        }
        return fallback
    }

    private func isGenericDestinationTitle(_ title: String, providerID: RouteProviderID?) -> Bool {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return true }
        let lowercased = trimmed.lowercased()
        if lowercased == "selected destination" || lowercased == "route" || lowercased == "recent destination" || lowercased == "dropped pin" {
            return true
        }
        if let providerID, lowercased == "\(providerID.displayName.lowercased()) sample destination" {
            return true
        }
        return false
    }
}
