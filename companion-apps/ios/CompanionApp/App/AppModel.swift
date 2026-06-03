import Foundation
import Combine

@MainActor
final class AppModel: ObservableObject {

    let persistence: CompanionPersistence
    let bleService: BleRouteSyncService
    let locationService: CoreLocationService
    let deviceManager: DeviceManager
    let sessionManager: SessionManager
    let diagnosticsStore = CompanionDiagnosticsStore()
    let routingDiagnosticsStore: RoutingDiagnosticsStore
    lazy var routeSyncService: RouteSyncService = RouteSyncService(
        bleService: self.bleService,
        sessionManager: sessionManager,
        providers: providers,
        onRefreshDiagnostics: { [weak self] in self?.refreshDiagnostics() }
    )
    lazy var routeHistoryService: RouteHistoryService = RouteHistoryService(
        persistence: persistence,
        deviceManager: deviceManager,
        sessionManager: sessionManager,
        routeSyncService: routeSyncService,
        settingsProvider: { [weak self] in self?.settings ?? CompanionSettings.defaults },
        previewProvider: { [weak self] in self?.preview ?? RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil) },
        onRefreshDiagnostics: { [weak self] in self?.refreshDiagnostics() },
        onSetCurrentSourceMode: { [weak self] in self?.currentSourceMode = $0 },
        onSetPreview: { [weak self] in self?.preview = $0 }
    )
    lazy var shareImportService = ShareImportService(appModel: self)
    private(set) var routingActivityCoordinator: RoutingActivityCoordinator
    private(set) var liveActivityCoordinator: LiveActivityCoordinator
    @Published var settings: CompanionSettings
    @Published var selectedProviderID: RouteProviderID = .hsl
    @Published var currentSourceMode: RouteSourceMode
    @Published var routeRequest = RoutePlanRequest(
        origin: CoordinatePoint(latitude: 60.1699, longitude: 24.9384),
        destination: CoordinatePoint(latitude: 60.1921, longitude: 24.9458),
        providerID: .hsl
    )
    @Published var preview = RoutePreviewModel(
        alternatives: [], selectedAlternativeID: nil,
        routeIdentifier: nil, routeRevision: nil, planningNotice: nil
    )
    @Published var homePreviewRequestID = UUID()
    @Published var homeStartRequestID = UUID()
    @Published var isRoutingInProgress: Bool = false {
        didSet {
            locationService.setNavigationAccuracy(isRoutingInProgress)
            syncRoutingActivityServices()
        }
    }
    @Published var isAppInBackground: Bool = false

    /// Latest route package the user is actively riding. Set by `HomeViewModel`
    /// whenever its `guidanceRoute` changes.
    var activeGuidanceRoute: NormalizedRoutePackage?
    private var cancellables = Set<AnyCancellable>()
    private var bleStateCoordinator: BleStateCoordinator?
    private var locationDiagnosticsCoordinator: LocationDiagnosticsCoordinator?

    private lazy var providers: [RouteProviderID: RoutingProvider] = [
        .hsl: HslRoutingAdapter(settingsProvider: { [unowned self] in self.settings }),
        .osm: OsmCyclingRoutingAdapter(),
        .gpxImport: GpxRoutingAdapter(),
        .fitImport: SampleRoutingAdapter(providerID: .fitImport),
        .tcxImport: SampleRoutingAdapter(providerID: .tcxImport),
    ]
    init(persistence: CompanionPersistence = CompanionPersistence(), bleService: BleRouteSyncService? = nil) {
        self.persistence = persistence
        self.routingDiagnosticsStore = RoutingDiagnosticsStore(persistence: persistence)
        self.bleService = bleService ?? BleRouteSyncService()
        let locationService = CoreLocationService(persistence: persistence)
        self.locationService = locationService
        self.deviceManager = DeviceManager(
            bleService: self.bleService,
            persistence: persistence,
            locationService: locationService
        )
        self.sessionManager = SessionManager(persistence: persistence)
        let loadedSettings = persistence.loadSettings()
        settings = loadedSettings
        let preferences = persistence.loadRoutePlannerPreferences()
        currentSourceMode = preferences.defaultSourceMode
        self.routingActivityCoordinator = RoutingActivityCoordinator(
            idleTimer: IdleTimerController(),
            speech: SpeechService()
        )
        self.liveActivityCoordinator = LiveActivityCoordinator(
            driver: ActivityKitLiveActivityDriver()
        )
        self.routingActivityCoordinator.onCueDispatched = { [weak self] cueType, messageText in
            self?.routingDiagnosticsStore.recordEvent(.audioCueDispatched(cueType: cueType, messageText: messageText))
        }

        T.setActiveLocale(T.resolveLocale(loadedSettings.language))
        selectedProviderID = sessionManager.session.providerID
        if let initial = locationService.currentLocation ?? locationService.lastKnownLocation {
            routeRequest.origin = initial
        }
        let bleCoordinator = BleStateCoordinator(
            bleService: self.bleService,
            deviceManager: deviceManager,
            onRefreshDiagnostics: { [weak self] in self?.refreshDiagnostics() },
            onRerouteRequest: { [weak self] location, reason in
                await self?.rerouteActiveSession(from: location, reason: reason)
            }
        )
        self.bleStateCoordinator = bleCoordinator
        bleCoordinator.forwardObjectWillChange(to: objectWillChange)
        let locCoordinator = LocationDiagnosticsCoordinator(
            locationService: locationService,
            routingDiagnosticsStore: routingDiagnosticsStore,
            onLocationUpdate: { [weak self] point in
                // Don't "optimize" this away: HslAvailabilityService reads
                // `routeRequest.origin` to gate the live-HSL source mode on
                // whether the rider is currently in Finland. The publish
                // here already coalesces with the surrounding locationService
                // publishes into one body re-render per GPS fix, so the cost
                // is negligible.
                self?.routeRequest.origin = point
            }
        )
        self.locationDiagnosticsCoordinator = locCoordinator
        locCoordinator.forwardObjectWillChange(to: objectWillChange)
        observeChildChanges()
        locationService.start()
    }
    func connectToDevice() async {
        await deviceManager.connectToDevice()
        refreshDiagnostics()
    }

    func resumePendingTransfer() async {
        await deviceManager.resumePendingTransfer()
        refreshDiagnostics()
    }

    func planRoute(
        using sourceMode: RouteSourceMode,
        preferredTitle: String? = nil,
        revisionOverride: Int? = nil,
        rerouteContext: RerouteContext? = nil
    ) async {
        let hslAvailable = HslAvailabilityService.isHslAvailable(settings: settings, request: routeRequest)
        let effectiveMode: RouteSourceMode = (!hslAvailable && sourceMode != .osm) ? .osm : sourceMode
        currentSourceMode = effectiveMode
        routeRequest.providerID = effectiveMode.primaryProviderID
        do {
            preview = try await RoutePlanningEngine.buildPreview(
                for: routeRequest,
                sourceMode: effectiveMode,
                providers: providers,
                isHslAvailable: hslAvailable,
                currentSourceMode: currentSourceMode,
                session: sessionManager.session,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
            if routingDiagnosticsStore.isRecording {
                let alts = preview.alternatives.map { alt in
                    RouteAltInfo(
                        providerName: alt.normalizedPackage.provenance.providerID.rawValue,
                        routeId: alt.normalizedPackage.routeIdentifier,
                        label: alt.title
                    )
                }
                routingDiagnosticsStore.recordEvent(.routeAlternativesSuggested(alternatives: alts))
            }
            persistence.saveRecentDestination(routeRequest.destination)
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

    func selectAlternativePreviewOnly(_ alternativeID: UUID) {
        preview.selectedAlternativeID = alternativeID
        preview.routeIdentifier = preview.selectedAlternative?.normalizedPackage.routeIdentifier
        preview.routeRevision = preview.selectedAlternative?.normalizedPackage.revision
        if let providerID = preview.selectedAlternative?.normalizedPackage.provenance.providerID {
            selectedProviderID = providerID
        }
        refreshDiagnostics()
    }

    func rerouteActiveSession(
        from riderLocation: CoordinatePoint,
        reason: String,
        rerouteContext: RerouteContext? = nil
    ) async {
        guard sessionManager.session.destinationCoordinate != nil else { return }
        let routeIdentifier = sessionManager.session.routeIdentifier ?? preview.routeIdentifier ?? "preview-route"
        await bleService.receiveRerouteRequest(
            RouteRerouteRequestMessage(
                routeIdentifier: routeIdentifier,
                riderLocation: riderLocation,
                reason: reason
            )
        )
        routeRequest = RoutePlanRequest(
            origin: riderLocation,
            destination: sessionManager.session.destinationCoordinate ?? routeRequest.destination,
            providerID: sessionManager.session.sourceMode.primaryProviderID
        )
        await planRoute(
            using: sessionManager.session.sourceMode,
            preferredTitle: sessionManager.session.destinationLabel,
            revisionOverride: (sessionManager.session.routeRevision ?? 0) + 1,
            rerouteContext: rerouteContext
        )
        sessionManager.session.lastRerouteReason = reason
        sessionManager.session.lastRerouteTimestamp = Date()
        let sendResult = await routeSyncService.sendSelectedRoute(
            preview: preview,
            routeRequest: routeRequest,
            sourceMode: currentSourceMode
        )
        routingDiagnosticsStore.recordEvent(.rerouteCompleted(result: sendResult ? "success" : "failed"))
    }

    func importGpxFile(from url: URL) async {
        do {
            guard let adapter = providers[.gpxImport] as? GpxRoutingAdapter else { return }
            let result = try FileImportService.importGpxFile(from: url, adapter: adapter)
            applyFileImportResult(result)
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
        guard let adapter = providers[providerID] as? SampleRoutingAdapter else { return }
        let result = try FileImportService.importSampleFile(
            from: url,
            providerID: providerID,
            adapter: adapter,
            origin: locationService.bestLocation,
            preferredTitle: preferredTitle
        )
        applyFileImportResult(result)
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
                origin: locationService.bestLocation,
                destination: item.destination ?? package.geometry.last ?? routeRequest.destination,
                providerID: package.provenance.providerID
            )
            let sessionSourceMode = package.provenance.providerID == .osm ? RouteSourceMode.osm : currentSourceMode
            applySelectedAlternativeToSession(sourceMode: sessionSourceMode, destination: routeRequest.destination, preferredTitle: item.title)
        } else if let destination = item.destination {
            routeRequest = RoutePlanRequest(origin: locationService.bestLocation, destination: destination, providerID: currentSourceMode.primaryProviderID)
            await planRoute(using: currentSourceMode, preferredTitle: item.title)
        }
    }

    func persistSettings() {
        T.setActiveLocale(T.resolveLocale(settings.language))
        persistence.saveSettings(settings)
        HslAvailabilityService.normalizeSourceModeForHslAvailability(
            currentSourceMode: &currentSourceMode,
            settings: settings,
            request: routeRequest,
            persistence: persistence
        )
        syncRoutingActivityServices()
    }

    func syncRoutingActivityServices() {
        let pairedWithDevice = deviceManager.pairedPeripheral != nil
        routingActivityCoordinator.onSettingsOrRoutingChange(
            settings: settings,
            isRouting: isRoutingInProgress,
            pairedWithDevice: pairedWithDevice
        )
        liveActivityCoordinator.onSettingsOrRoutingChange(
            settings: settings,
            isRouting: isRoutingInProgress,
            route: activeGuidanceRoute
        )
    }

    func handleApplicationLifecycleEnteredBackground() {
        isAppInBackground = true
        if !isRoutingInProgress {
            locationService.stop()
        }
    }

    func handleApplicationLifecycleEnteredForeground() {
        isAppInBackground = false
        locationService.start()
        syncRoutingActivityServices()
        if settings.routingDiagnosticsEnabled {
            routingDiagnosticsStore.startRecording()
        }
    }

    func refreshDiagnostics() {
        diagnosticsStore.update(
            from: sessionManager.session.routeIdentifier == nil ? nil : sessionManager.session,
            syncState: bleService.sessionState
        )
    }

    private func applySelectedAlternativeToSession(sourceMode: RouteSourceMode, destination: CoordinatePoint, preferredTitle: String?) {
        let selectedPackage = preview.selectedAlternative?.normalizedPackage
        let providerID = selectedPackage?.provenance.providerID ?? sourceMode.primaryProviderID
        selectedProviderID = providerID
        // Batch the six session-field updates into one struct re-assignment so
        // SessionManager fires `objectWillChange` once instead of six times.
        var session = sessionManager.session
        session.routeIdentifier = selectedPackage?.routeIdentifier ?? preview.routeIdentifier
        session.routeRevision = selectedPackage?.revision ?? preview.routeRevision
        session.destinationLabel = RoutePlanningEngine.displayDestinationTitle(
            selectedPackage: selectedPackage,
            preferredTitle: preferredTitle,
            fallback: "No destination"
        )
        session.destinationCoordinate = selectedPackage?.geometry.last ?? destination
        session.providerID = providerID
        session.sourceMode = sourceMode
        sessionManager.session = session
    }

    private func applyFileImportResult(_ result: FileImportResult) {
        selectedProviderID = result.providerID
        preview = result.preview
        if let destination = result.geometryDestination {
            routeRequest = RoutePlanRequest(
                origin: result.geometryOrigin ?? routeRequest.origin,
                destination: destination,
                providerID: result.providerID
            )
        }
        applySelectedAlternativeToSession(
            sourceMode: currentSourceMode,
            destination: routeRequest.destination,
            preferredTitle: result.suggestedTitle
        )
        refreshDiagnostics()
    }

    private func observeChildChanges() {
        deviceManager.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] in self?.objectWillChange.send() }
            .store(in: &cancellables)
        sessionManager.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] in self?.objectWillChange.send() }
            .store(in: &cancellables)
    }
}

#if DEBUG
extension AppModel {
    func replaceRoutingActivityCoordinatorForTesting(speech: SpeechPort) {
        routingActivityCoordinator = RoutingActivityCoordinator(
            idleTimer: IdleTimerController(),
            speech: speech
        )
    }

    func replaceLiveActivityCoordinatorForTesting(driver: LiveActivityDriver) {
        liveActivityCoordinator = LiveActivityCoordinator(driver: driver)
    }

    func replacePairedPeripheralForTesting(_ record: PairedPeripheralRecord?) {
        deviceManager.replacePairedPeripheralForTesting(record)
    }

    func replaceDeviceConnectedForTesting(_ value: Bool?) {
        deviceManager.replaceDeviceConnectedForTesting(value)
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
}
#endif
