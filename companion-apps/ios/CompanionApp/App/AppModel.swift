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
    /// Authoritative "is the rider currently following a route in this app
    /// session" flag. Set by `HomeViewModel.startSelectedRoute` and
    /// cleared by `stopActiveNavigation`. Defaults to false on app launch
    /// so a stale `activeSession.routeIdentifier` from disk can't trick
    /// the routing-activity coordinator into spinning up a Live Activity
    /// for a ride that isn't happening.
    @Published var isRoutingInProgress: Bool = false {
        didSet {
            locationService.setNavigationAccuracy(isRoutingInProgress)
            syncRoutingActivityServices()
        }
    }
    @Published var pairingState: PairingFlowState = .idle

    enum GpsSourceSelection: String, CaseIterable {
        case `internal`
        case phone
    }

    @Published var gpsSource: GpsSourceSelection = .internal
    @Published private(set) var isPhoneGpsForwarding: Bool = false
    private var phoneGpsForwarder: PhoneGpsForwarder?

    let diagnosticsStore = CompanionDiagnosticsStore()
    let persistence: CompanionPersistence
    let bleService: BleRouteSyncService
    let locationService: CoreLocationService
    private(set) var routingActivityCoordinator: RoutingActivityCoordinator
    private(set) var liveActivityCoordinator: LiveActivityCoordinator

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
        self.phoneGpsForwarder = PhoneGpsForwarder(
            bleClient: self.bleService.bluetoothClient,
            locationService: locationService
        )
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
        // Reflect forwarder state in published property.
        phoneGpsForwarder?.$isForwarding
            .receive(on: DispatchQueue.main)
            .assign(to: \.isPhoneGpsForwarding, on: self)
            .store(in: &cancellables)

        // Push the persisted language preference to the i18n runtime before
        // any view renders. Without this, T.string(...) defaults to .en
        // until the user opens routing (which is when
        // RoutingActivityCoordinator first calls setActiveLocale), so the
        // very first paint of every screen is English even when the user
        // picked Suomi. Read from the local rather than `self.settings`
        // because Swift won't let init access `self` until every stored
        // property is initialized.
        T.setActiveLocale(T.resolveLocale(loadedSettings.language))
        if let storedSession = persistence.loadLastSession() {
            activeSession = storedSession
            // The stored session reflects the LAST destination + provider
            // the user picked, which we want to remember. The actual
            // routing state (`routeIdentifier`/`routeRevision`) is
            // ephemeral — a fresh launch is by definition not mid-route.
            // Clearing here prevents the Live Activity from starting on
            // app launch off a stale session id when the user only meant
            // to open the app for planning.
            activeSession.routeIdentifier = nil
            activeSession.routeRevision = nil
            persistence.saveSession(activeSession)
        }
        selectedProviderID = activeSession.providerID
        pairedPeripheral = persistence.loadPairedPeripheral()
        if let initial = locationService.currentLocation ?? locationService.lastKnownLocation {
            routeRequest.origin = initial
        }
        bindBleState()
        bindLocationService()
        locationService.start()

        // App-launch auto-reconnect: if the user was previously paired,
        // try to connect once in the background. Failures are silent —
        // the home chip stays in `PairedDisconnected` and the user can
        // tap it to retry. We don't fall back to a service-UUID scan
        // here so a stranger's device doesn't silently become "the
        // paired one"; that path stays gated behind an explicit user
        // action (the chip's tap target or Settings → Connect).
        if let paired = pairedPeripheral {
            Task { [weak self] in
                pairingLog.notice("AppModel.init — auto-reconnect to paired peripheral [\(paired.identifier, privacy: .public)]")
                await self?.bleService.connectToPairedPeripheral(identifier: paired.identifier)
                await self?.refreshDiagnostics()
            }
        }
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

    /// Start or stop the phone GPS forwarder when the user toggles the
    /// GPS source picker in Device Settings. The firmware auto-detects
    /// phone GPS writes and switches source; when writes stop it falls
    /// back to Internal GPS after a 3-second timeout.
    func handleGpsSourceChange(to source: GpsSourceSelection) {
        switch source {
        case .internal:
            phoneGpsForwarder?.stop()
        case .phone:
            phoneGpsForwarder?.start()
        }
    }

    /// Step the device into pairing mode before the camera opens.
    ///
    /// The firmware defaults to showing the map; the QR overlay is
    /// only rendered after a companion writes the unencrypted
    /// `pairing_request` characteristic. The pairing flow runs this
    /// between the instructions step and the camera step so the device
    /// shows its QR by the time the user holds up the phone.
    ///
    /// Steps: scan → connect → write `pairing_request`. Throws so the
    /// pairing-flow view-model can surface a user-visible error before
    /// switching the screen to the camera.
    func prepareDeviceForPairing() async throws {
        pairingLog.notice("prepareDeviceForPairing — scan + connect + writePairingRequest")
        pairingState = .connecting
        do {
            _ = try await bleService.connectToAdvertisedPeripheral()
            try await bleService.writePairingRequest()
            pairingLog.notice("prepareDeviceForPairing OK — device should now show QR")
        } catch {
            pairingLog.error("prepareDeviceForPairing failed: \(error.localizedDescription, privacy: .public)")
            pairingState = .failed(error.localizedDescription)
            throw error
        }
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
        pairingLog.notice("completePairing — confirming (secret \(payload.ephemeralSecret.count, privacy: .public) B)")
        // We should already be connected from `prepareDeviceForPairing`.
        // If something dropped the connection in between (rare), fall
        // back to a fresh scan+connect — but the QR may no longer be
        // visible on the device by then.
        do {
            let info = try await bleService.connectToAdvertisedPeripheral()
            pairingLog.notice("completePairing — connection ready: \(info.name, privacy: .public) [\(info.identifier, privacy: .public)]")
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

    /// True when both endpoints of the current request fall inside Finland
    /// (Digitransit's nationwide coverage area).
    var isHslApplicableForRequest: Bool {
        AppModel.isInFinland(routeRequest.origin) && AppModel.isInFinland(routeRequest.destination)
    }

    /// True when HSL is both configured AND geographically usable for the current request.
    var isHslAvailable: Bool { isHslLiveConfigured && isHslApplicableForRequest }

    /// Source-mode tabs visible in the UI. With no Digitransit key, or when either endpoint
    /// is outside Finland, mixed/HSL collapse to OSM (the picker hides itself when there is
    /// only one option).
    var sourceModeOptions: [RouteSourceMode] {
        isHslAvailable ? RouteSourceMode.allCases : [.osm]
    }

    /// Approximate bounding box for mainland Finland (including Åland). Digitransit's
    /// `finland` router aggregates GTFS feeds nationwide.
    static func isInFinland(_ point: CoordinatePoint) -> Bool {
        (59.7...70.1).contains(point.latitude) && (19.0...31.7).contains(point.longitude)
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
        // Apply the language preference to the i18n runtime synchronously so
        // the next view repaint uses the new locale; otherwise the picker
        // change wouldn't visibly update strings until the user kicked
        // routing or relaunched.
        T.setActiveLocale(T.resolveLocale(settings.language))
        persistence.saveSettings(settings)
        normalizeSourceModeForHslAvailability()
        syncRoutingActivityServices()
    }

    /// Re-evaluates the routing activity coordinator's gating on every
    /// settings or active-session change. Per-tick cue dispatch happens
    /// from `HomeViewModel` where the per-tick guidance state lives.
    func syncRoutingActivityServices() {
        let pairedWithDevice = pairedPeripheral != nil
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

    /// Latest route package the user is actively riding. Source of truth
    /// lives on `HomeViewModel.guidanceRoute`; the live activity
    /// coordinator only needs the package's `routeIdentifier`,
    /// `summary`, and `maneuvers`. Set by `HomeViewModel` whenever its
    /// `guidanceRoute` changes.
    var activeGuidanceRoute: NormalizedRoutePackage?

    /// Spec line 130: when the user toggles "Allow GPS in background" on,
    /// escalate the CoreLocation authorization to Always. iOS only offers
    /// the prompt once; if it fails to escalate, [locationManualSettingsHint]
    /// becomes true so the UI can point the user to Settings.app.
    func requestAlwaysLocationAuthorization() {
        locationService.requestAlwaysAuthorization()
    }

    /// True when the user has asked for background GPS but iOS held us at
    /// "When-In-Use", so the user must change it manually in Settings.
    var locationManualSettingsHint: Bool {
        locationService.manualSettingsHint
    }

    /// Called by `ESP32MapCompanionApp` when the app enters the background.
    /// True when the app is currently in `.background` scene phase. Set
    /// by ESP32MapCompanionApp's scenePhase observer; consumed by the
    /// cue dispatch path to honour the `audioCuesOnlyInBackground`
    /// setting (spec line 144).
    @Published var isAppInBackground: Bool = false

    /// If no route is in progress we stop GPS to preserve battery — the
    /// "Allow GPS in background" permission is for routing only, not for
    /// keeping a planning-mode location feed alive while the user is away.
    func handleApplicationLifecycleEnteredBackground() {
        isAppInBackground = true
        if !isRoutingInProgress {
            locationService.stop()
        }
    }

    /// Resumes GPS when the app comes back to the foreground (so the
    /// where-to bar and the recenter button work again).
    func handleApplicationLifecycleEnteredForeground() {
        isAppInBackground = false
        locationService.start()
    }

    /// Test seam: rebuilds the routing-activity coordinator with the given
    /// fakes so unit tests can assert that cues are dispatched without
    /// invoking AVSpeechSynthesizer.
    func replaceRoutingActivityCoordinatorForTesting(speech: SpeechPort) {
        routingActivityCoordinator = RoutingActivityCoordinator(
            idleTimer: IdleTimerController(),
            speech: speech
        )
    }

    /// Test seam: swap in a fake driver so the coordinator can be exercised
    /// without touching ActivityKit.
    func replaceLiveActivityCoordinatorForTesting(driver: LiveActivityDriver) {
        liveActivityCoordinator = LiveActivityCoordinator(driver: driver)
    }

    /// Test seam: stamps the paired-peripheral state without going through
    /// the persistence + BLE pairing flow.
    func replacePairedPeripheralForTesting(_ record: PairedPeripheralRecord?) {
        pairedPeripheral = record
    }

    /// Test override for `isDeviceConnected`. Production reads the BLE
    /// session; tests stand in a known boolean so the audio-cue gating
    /// contract (cues silenced ONLY when actually connected, not just
    /// paired) can be exercised without a real BLE link.
    private var deviceConnectedTestOverride: Bool?

    func replaceDeviceConnectedForTesting(_ value: Bool?) {
        deviceConnectedTestOverride = value
    }

    /// True iff the companion is actively connected to the ESP32 device.
    /// Spec lines 7 / 131 — when the device is the on-screen UI, the
    /// phone goes silent (no cues) and the live activity is suppressed.
    /// A previously paired peripheral that is currently DISCONNECTED
    /// does not count: the rider is using the phone alone.
    var isDeviceConnectedForCueSuppression: Bool {
        if let override = deviceConnectedTestOverride { return override }
        return isDeviceConnected
    }

    /// When HSL becomes unusable (no key OR endpoints outside Finland), fall back any
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

    func planRoute(
        using sourceMode: RouteSourceMode,
        preferredTitle: String? = nil,
        revisionOverride: Int? = nil,
        rerouteContext: RerouteContext? = nil
    ) async {
        // If HSL isn't available for this trip (no key OR endpoints outside Finland),
        // collapse mixed/HSL down to OSM before planning so we don't race a useless provider.
        let effectiveMode: RouteSourceMode = (!isHslAvailable && sourceMode != .osm) ? .osm : sourceMode
        currentSourceMode = effectiveMode
        routeRequest.providerID = effectiveMode.primaryProviderID
        do {
            preview = try await buildPreview(
                for: routeRequest,
                sourceMode: effectiveMode,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
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

    /// Update preview selection without touching activeSession.destinationLabel.
    /// Used during alternatives exploration so the rider's typed destination is preserved.
    func selectAlternativePreviewOnly(_ alternativeID: UUID) {
        preview.selectedAlternativeID = alternativeID
        preview.routeIdentifier = preview.selectedAlternative?.normalizedPackage.routeIdentifier
        preview.routeRevision = preview.selectedAlternative?.normalizedPackage.revision
        if let providerID = preview.selectedAlternative?.normalizedPackage.provenance.providerID {
            selectedProviderID = providerID
        }
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

    func rerouteActiveSession(
        from riderLocation: CoordinatePoint,
        reason: String,
        rerouteContext: RerouteContext? = nil
    ) async {
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
        await planRoute(
            using: activeSession.sourceMode,
            preferredTitle: activeSession.destinationLabel,
            revisionOverride: (activeSession.routeRevision ?? 0) + 1,
            rerouteContext: rerouteContext
        )
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
                // Stop phone GPS forwarding when BLE disconnects so the
                // companion doesn't keep wasting power on writes that
                // can't reach the device.
                if state.connectionState == .disconnected {
                    self.phoneGpsForwarder?.stop()
                }
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
        revisionOverride: Int?,
        rerouteContext: RerouteContext? = nil
    ) async throws -> RoutePreviewModel {
        switch sourceMode {
        case .mixed:
            return try await buildMixedPreview(
                for: request,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
        case .hsl, .osm:
            guard let provider = providers[sourceMode.primaryProviderID] else {
                throw NSError(domain: "AppModel", code: 1, userInfo: [NSLocalizedDescriptionKey: "Missing provider for \(sourceMode.displayName)"])
            }
            var preview = try await preview(
                from: provider,
                request: request,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
            preview.alternatives = presentAlternatives(preview.alternatives, sourceMode: sourceMode)
            preview.selectedAlternativeID = preview.alternatives.first?.id
            preview.routeIdentifier = preview.alternatives.first?.normalizedPackage.routeIdentifier
            preview.routeRevision = preview.alternatives.first?.normalizedPackage.revision
            return preview
        }
    }

    private func preview(
        from provider: RoutingProvider,
        request: RoutePlanRequest,
        revisionOverride: Int?,
        rerouteContext: RerouteContext? = nil
    ) async throws -> RoutePreviewModel {
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
            return try await provider.replanRoute(
                using: session,
                riderLocation: request.origin,
                rerouteContext: rerouteContext
            )
        }
        return try await provider.planRoute(request)
    }

    private func buildMixedPreview(
        for request: RoutePlanRequest,
        revisionOverride: Int?,
        rerouteContext: RerouteContext? = nil
    ) async throws -> RoutePreviewModel {
        guard let osm = providers[.osm] else {
            throw NSError(domain: "AppModel", code: 2, userInfo: [NSLocalizedDescriptionKey: "Mixed mode providers are unavailable"])
        }
        // Skip the HSL race when HSL is unavailable (no key OR endpoints outside Finland).
        let includeHsl = isHslAvailable
        async let osmPreview = preview(
            from: osm,
            request: request,
            revisionOverride: revisionOverride,
            rerouteContext: rerouteContext
        )
        let previews: [RoutePreviewModel]
        if includeHsl, let hsl = providers[.hsl] {
            async let hslPreview = preview(
                from: hsl,
                request: request,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
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
        return T.string("planning.mixedRoutesFromHslAndOsm")
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

    /// Label every visible alternative with its underlying engine name.
    /// User-driven: "OSM Route 1 / OSM Route 2 / HSL Route 1" was noisy —
    /// the engine name carries the same information without the
    /// per-provider counter, and lets us drop the redundant "via …"
    /// subtitle entirely.
    ///
    ///   - OSM via BRouter `fastbike` → "BRouter fastbike"
    ///   - OSM via BRouter `trekking` → "BRouter trekking"
    ///   - OSM via OSRM bike          → "OSM Route"
    ///   - HSL Digitransit live / fastest     → "HSL Fastest"
    ///   - HSL Digitransit live / alternative → "HSL Route"
    private func presentAlternatives(_ alternatives: [RouteAlternative], sourceMode: RouteSourceMode) -> [RouteAlternative] {
        return alternatives.prefix(3).map { alternative in
            let label = Self.friendlyAlternativeLabel(for: alternative)
            return RouteAlternative(
                id: alternative.id,
                title: label.title,
                subtitle: label.subtitle,
                distanceMeters: alternative.distanceMeters,
                durationSeconds: alternative.durationSeconds,
                normalizedPackage: alternative.normalizedPackage
            )
        }
    }

    /// Pure helper used by `presentAlternatives` and pinned by
    /// `RouteAlternativeTitlesTests`. Maps a normalized route package's
    /// provider + sourceReference into the short engine-derived title
    /// the user wanted to see in the suggested-routes card.
    static func friendlyAlternativeLabel(for alternative: RouteAlternative) -> (title: String, subtitle: String) {
        let providerID = alternative.normalizedPackage.provenance.providerID
        let sourceRef = alternative.normalizedPackage.provenance.sourceReference?.lowercased() ?? ""
        switch providerID {
        case .osm:
            if sourceRef.contains("fastbike") { return ("BRouter fastbike", "") }
            if sourceRef.contains("trekking") { return ("BRouter trekking", "") }
            return ("OSM Route", "")
        case .hsl:
            if sourceRef.contains("fastest") { return ("HSL Fastest", "") }
            return ("HSL Route", "")
        case .gpxImport, .fitImport, .tcxImport:
            return (providerID.displayName, "")
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
            fallback: "No destination"
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
