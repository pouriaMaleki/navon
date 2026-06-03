import CoreLocation
import Foundation
import MapKit

@MainActor
final class HomeViewModel: ObservableObject {
    private static let arrivalNoticeAutoDismissDelay: TimeInterval = 60
    static var arrivalNoticeAutoDismissDelayForTesting: TimeInterval?

    private let appModel: AppModel
    private let placeSearchService: PlaceSearchService
    let searchController: SearchController
    private let switchablePlanningProviders: Set<RouteProviderID> = [.hsl, .osm]
    private let rerouteThrottle = RerouteThrottle()
    let offRouteTracker: OffRouteTracker
    private let headingTrail = HeadingTrail(
        maxAgeMs: 3_000, maxFixes: 6,
        minDisplacementM: 4.0, smoothingAlpha: 0.45
    )
    private let mapInteractionRecenterDelay: TimeInterval = 1.3

    @Published var activeRouteIdentifier: String?
    @Published var homeMode: HomeMode = .planning
    @Published var compassMode: HomeCompassMode = .autoFollow
    @Published private(set) var planningStatus: String?
    @Published private(set) var mapRecenterRequestID: Int = 0
    @Published private(set) var mapFollowRiderTick: Int = 0
    @Published private(set) var isUserInteractingWithMap: Bool = false
    @Published var showConnectionPopover: Bool = false
    @Published var arrivalNotice: String?
    @Published private(set) var offRouteDistanceM: Double = 0
    @Published private(set) var offRoute: Bool = false
    @Published private(set) var rerouteRequested: Bool = false
    @Published private(set) var reroutingAttemptTimestampsMs: [Double] = []
    @Published private(set) var reroutingDelayedUntilMs: Double?
    @Published private(set) var progressDistanceM: Double = 0
    @Published var isExploringAlternativesFromGuidance: Bool = false
    @Published private(set) var explorationSelectedID: UUID?

    private var routeTotalDistanceM: Double = 0
    private var mapInteractionRecenterTask: Task<Void, Never>?
    private var arrivalNoticeAutoDismissTask: Task<Void, Never>?
    private var activeRoutePackage: NormalizedRoutePackage?

    var headingTrailForTesting: HeadingTrail { headingTrail }

    init(appModel: AppModel, placeSearchService: PlaceSearchService = MapKitPlaceSearchService()) {
        self.appModel = appModel
        self.placeSearchService = placeSearchService
        self.searchController = SearchController(appModel: appModel, placeSearchService: placeSearchService)
        let tracker = OffRouteTracker(rerouteThrottle: rerouteThrottle)
        self.offRouteTracker = tracker
        tracker.onDiagnosticsEvent = { [weak appModel] event in
            appModel?.routingDiagnosticsStore.recordEvent(event)
        }
        tracker.onRerouteNeeded = { [weak self] rider, context in
            await self?.appModel.rerouteActiveSession(from: rider, reason: "Off-route", rerouteContext: context)
        }
        tracker.onArrivalDetected = { [weak self] in
            self?.declareArrival()
        }
        tracker.onProgressTick = { [weak self] in
            guard let self else { return }
            if let line = self.nextInstructionLine {
                self.appModel.routingDiagnosticsStore.recordEvent(.nextTurnAlerted(
                    instructionText: line,
                    distanceRemainingM: self.guidanceRoute?.maneuvers.first { m in
                        m.maneuverType != .depart && m.maneuverType != .arrive &&
                        m.distanceFromStartMeters > self.progressDistanceM
                    }.map { $0.distanceFromStartMeters - self.progressDistanceM } ?? 0
                ))
            }
        }
        searchController.onUrlResolved = { [weak self] title, point in
            guard let self else { return }
            self.planningStatus = "Planning route to \(title)…"
            defer { self.planningStatus = nil }
            await self.appModel.planRoute(using: self.sourceMode, preferredTitle: title)
            self.appModel.routeHistoryService.recordRecentDestination(title: title, coordinate: point)
            self.appModel.routeHistoryService.recordPlannedPreview(source: .plannedRoute, sourceLabel: self.sourceMode.displayName)
        }
    }

    var deviceChipState: DeviceChipState? {
        let connection = appModel.bleService.sessionState.connectionState
        guard let record = appModel.deviceManager.pairedPeripheral else {
            return nil
        }
        switch connection {
        case .scanning, .connecting:
            return .connecting(name: record.friendlyName)
        case .connected:
            return .connected(name: record.friendlyName)
        case .disconnected:
            return .pairedDisconnected(name: record.friendlyName)
        }
    }

    var sourceMode: RouteSourceMode {
        get { appModel.currentSourceMode }
        set {
            appModel.currentSourceMode = newValue
            appModel.routeHistoryService.routePlannerPreferences = RoutePlannerPreferences(
                defaultSourceMode: newValue,
                suggestionMode: appModel.routeHistoryService.routePlannerPreferences.suggestionMode,
                startBehavior: appModel.routeHistoryService.routePlannerPreferences.startBehavior
            )
        }
    }

    var recentItems: [RouteHistoryItem] {
        Array(appModel.routeHistoryService.routeHistoryItems.prefix(searchController.visibleRecentCount))
    }

    var visibleSuggestions: [DestinationSearchResult] {
        Array(searchController.suggestions.prefix(searchController.visibleSuggestionCount))
    }

    var previewAlternatives: [RouteAlternative] {
        let limit = appModel.routeHistoryService.routePlannerPreferences.suggestionMode == .bestOnly ? 1 : 3
        return Array(appModel.preview.alternatives.prefix(limit))
    }

    var selectedPreview: RouteAlternative? {
        appModel.preview.selectedAlternative
    }

    var guidanceRoute: NormalizedRoutePackage? {
        switch homeMode {
        case .phoneGuidance:
            if isExploringAlternativesFromGuidance { return activeRoutePackage }
            return selectedPreview?.normalizedPackage
        case .deviceOverview, .sendingToDevice:
            return selectedPreview?.normalizedPackage
        case .planning:
            return nil
        }
    }

    var previewRoute: NormalizedRoutePackage? {
        selectedPreview?.normalizedPackage
    }

    var destinationCoordinate: CoordinatePoint? {
        guidanceRoute?.geometry.last ?? previewRoute?.geometry.last
    }

    var originCoordinate: CoordinatePoint? {
        guidanceRoute?.geometry.first ?? previewRoute?.geometry.first
    }

    var displayedRouteCoordinates: [CoordinatePoint] {
        guidanceRoute?.geometry ?? previewRoute?.geometry ?? []
    }

    var isPreviewLockedToImportedRoute: Bool {
        guard let providerID = previewRoute?.provenance.providerID else { return false }
        return !switchablePlanningProviders.contains(providerID)
    }

    var ridingCameraDistanceM: Double {
        appModel.settings.ridingCameraDistanceM ?? CameraMath.defaultRidingCameraDistanceM
    }

    var travelHeadingDegrees: Double? {
        headingTrail.travelHeadingDegrees
    }

    private var autoReroutePending: Bool { offRouteTracker.autoReroutePending }
    private(set) var pendingAutoRerouteTask: Task<Void, Never>? {
        get { offRouteTracker.pendingAutoRerouteTask }
        set {}
    }

    var shouldShowSearchPanel: Bool {
        HomeDisplay.shouldShowSearchPanel(
            homeMode: homeMode, isSearchOpen: searchController.isSearchOpen,
            isResolvingUrl: searchController.isResolvingUrl, urlResolveError: searchController.urlResolveError,
            query: searchController.query, hasRecentItems: !recentItems.isEmpty,
            hasVisibleSuggestions: !visibleSuggestions.isEmpty
        )
    }

    var shouldShowSourceControl: Bool {
        HomeDisplay.shouldShowSourceControl(
            homeMode: homeMode, hasPreviewAlternatives: !previewAlternatives.isEmpty,
            isPreviewLockedToImportedRoute: isPreviewLockedToImportedRoute,
            sourceModeOptionsCount: HslAvailabilityService.sourceModeOptions(settings: appModel.settings, request: appModel.routeRequest).count
        )
    }

    var routeSuggestionsTitle: String {
        HomeDisplay.routeSuggestionsTitle(isPreviewLockedToImportedRoute: isPreviewLockedToImportedRoute)
    }

    var isShowingActiveNavigation: Bool {
        HomeDisplay.isShowingActiveNavigation(homeMode: homeMode)
    }

    var startButtonTitle: String {
        HomeDisplay.startButtonTitle(homeMode: homeMode, isDeviceConnected: appModel.deviceManager.isDeviceConnected)
    }

    var activeNavigationTitle: String {
        HomeDisplay.activeNavigationTitle(
            guidanceRouteDestinationLabel: guidanceRoute?.summary.destinationLabel,
            sessionDestinationLabel: appModel.sessionManager.session.destinationLabel
        )
    }

    var activeNavigationSubtitle: String {
        HomeDisplay.activeNavigationSubtitle(
            homeMode: homeMode, remainingDistanceM: remainingDistanceM,
            remainingDurationSeconds: remainingDurationSeconds,
            guidanceRouteTotalDistanceMeters: guidanceRoute?.summary.totalDistanceMeters ?? 0,
            guidanceRouteEstimatedDurationSeconds: guidanceRoute?.summary.estimatedDurationSeconds ?? 0,
            lastSyncResult: appModel.bleService.sessionState.lastSyncResult,
            selectedPreviewSummaryLine: selectedPreview?.normalizedPackage.summaryLine ?? ""
        )
    }

    var guidanceSubtitleLine: String {
        HomeDisplay.guidanceSubtitleLine(
            guidanceRouteDestinationLabel: guidanceRoute?.summary.destinationLabel,
            sessionDestinationLabel: appModel.sessionManager.session.destinationLabel,
            activeNavigationSubtitle: activeNavigationSubtitle
        )
    }

    var remainingDistanceM: Double {
        HomeDisplay.remainingDistanceM(routeTotalDistanceM: routeTotalDistanceM, progressDistanceM: progressDistanceM)
    }

    var remainingDurationSeconds: Double {
        HomeDisplay.remainingDurationSeconds(
            guidanceRouteTotalDistanceMeters: guidanceRoute?.summary.totalDistanceMeters ?? 0,
            guidanceRouteEstimatedDurationSeconds: guidanceRoute?.summary.estimatedDurationSeconds ?? 0,
            routeTotalDistanceM: routeTotalDistanceM, progressDistanceM: progressDistanceM
        )
    }

    var routeOverviewGeometry: [CoordinatePoint] {
        HomeDisplay.routeOverviewGeometry(guidanceRouteGeometry: guidanceRoute?.geometry, progressDistanceM: progressDistanceM)
    }

    var offRouteLabel: String? {
        HomeDisplay.offRouteLabel(rerouteRequested: rerouteRequested, offRoute: offRoute)
    }

    var nextInstructionLine: String? {
        HomeDisplay.nextInstructionLine(guidanceRoute: guidanceRoute, progressDistanceM: progressDistanceM)
    }

    var compassSymbolName: String {
        HomeDisplay.compassSymbolName(compassMode: compassMode)
    }

    typealias RoutingTopLayout = HomeDisplay.RoutingTopLayout
    typealias TopRightIcon = HomeDisplay.TopRightIcon
    typealias TopLeftIcon = HomeDisplay.TopLeftIcon

    var routingTopLayout: RoutingTopLayout? {
        HomeDisplay.routingTopLayout(
            homeMode: homeMode, nextInstructionLine: nextInstructionLine,
            activeNavigationTitle: activeNavigationTitle, offRouteLabel: offRouteLabel,
            remainingDistanceM: remainingDistanceM, remainingDurationSeconds: remainingDurationSeconds,
            guidanceRoute: guidanceRoute,
            sessionDestinationLabel: appModel.sessionManager.session.destinationLabel
        )
    }

    var topRightIconStack: [TopRightIcon] {
        HomeDisplay.topRightIconStack(isPaired: appModel.deviceManager.pairedPeripheral != nil)
    }

    var topLeftIconStack: [TopLeftIcon] {
        HomeDisplay.topLeftIconStack(homeMode: homeMode)
    }

    var selectedAlternativeIDForDisplay: UUID? {
        HomeDisplay.selectedAlternativeIDForDisplay(
            isExploringAlternativesFromGuidance: isExploringAlternativesFromGuidance,
            explorationSelectedID: explorationSelectedID,
            previewSelectedAlternativeID: appModel.preview.selectedAlternativeID
        )
    }

    var guidanceAlternatives: [RouteAlternative] {
        HomeDisplay.guidanceAlternatives(
            isExploringAlternativesFromGuidance: isExploringAlternativesFromGuidance,
            previewAlternatives: appModel.preview.alternatives
        )
    }

    enum RidingZoomDirection { case zoomIn, zoomOut }

    func bumpRidingZoom(direction: RidingZoomDirection) {
        let factor = direction == .zoomIn ? (1.0 / CameraMath.ridingZoomStepFactor) : CameraMath.ridingZoomStepFactor
        let raw = ridingCameraDistanceM * factor
        let next = min(CameraMath.maxRidingCameraDistanceM, max(CameraMath.minRidingCameraDistanceM, raw))
        appModel.settings.ridingCameraDistanceM = next
        appModel.persistSettings()
    }

    func selectSuggestion(_ suggestion: DestinationSearchResult) {
        searchController.selectSuggestion(suggestion)
        searchController.closeSearch()
        Task {
            planningStatus = "Planning route to \(suggestion.title)…"
            defer { planningStatus = nil }
            appModel.routeRequest = RoutePlanRequest(
                origin: appModel.locationService.bestLocation,
                destination: suggestion.coordinate,
                providerID: sourceMode.primaryProviderID
            )
            await appModel.planRoute(using: sourceMode, preferredTitle: suggestion.title)
            appModel.routeHistoryService.recordRecentDestination(title: suggestion.title, coordinate: suggestion.coordinate)
            appModel.routeHistoryService.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceMode.displayName)
            searchController.query = suggestion.title
            homeMode = .planning
            if appModel.routeHistoryService.routePlannerPreferences.startBehavior == .automatic {
                await startSelectedRoute()
            }
        }
    }

    func selectRecent(_ item: RouteHistoryItem) {
        searchController.selectRecent(item)
        searchController.closeSearch()
        Task {
            planningStatus = "Opening \(item.title)…"
            defer { planningStatus = nil }
            await appModel.applyRouteHistoryPreview(item)
            searchController.query = item.title
            homeMode = .planning
            if appModel.routeHistoryService.routePlannerPreferences.startBehavior == .automatic {
                await startSelectedRoute()
            }
        }
    }

    func setDestinationFromMap(_ coordinate: CoordinatePoint) {
        planningStatus = "Resolving dropped pin…"
        Task {
            defer {
                if planningStatus == "Resolving dropped pin…" {
                    planningStatus = nil
                }
            }
            let resolved = await placeSearchService.resolveDestination(
                at: coordinate,
                fallbackTitle: "Dropped pin",
                preserveFallbackTitle: false
            )
            let manualDrop = resolved ?? DestinationSearchResult(
                id: "long-press-\(coordinate.latitude)-\(coordinate.longitude)",
                title: "Dropped pin",
                subtitle: "Map destination",
                coordinate: coordinate
            )
            selectSuggestion(manualDrop)
        }
    }

    func revealImportedPreview() async {
        searchController.cancelActiveSearch()
        activeRouteIdentifier = nil
        homeMode = .planning
        compassMode = .autoFollow
        searchController.suggestions = []
        searchController.closeSearch()

        if let pending = appModel.routeHistoryService.pendingHomeImportPresentation {
            planningStatus = "Opening imported destination…"
            defer { planningStatus = nil }
            if let item = appModel.routeHistoryService.routeHistoryItems.first(where: { $0.id == pending.routeHistoryItemID }) {
                await appModel.applyRouteHistoryPreview(item)
            } else if let destination = pending.destination {
                appModel.routeRequest = RoutePlanRequest(
                    origin: appModel.locationService.bestLocation,
                    destination: destination,
                    providerID: sourceMode.primaryProviderID
                )
                await appModel.planRoute(using: sourceMode, preferredTitle: pending.title)
            }
            searchController.query = pending.title
            appModel.routeHistoryService.clearPendingHomeImportPresentation()
            return
        }
    }

    func setSourceMode(_ mode: RouteSourceMode) {
        guard shouldShowSourceControl, sourceMode != mode else { return }
        sourceMode = mode
        guard let destination = destinationCoordinate else { return }
        Task {
            planningStatus = "Refreshing route options…"
            defer { planningStatus = nil }
            appModel.routeRequest = RoutePlanRequest(
                origin: appModel.locationService.bestLocation,
                destination: destination,
                providerID: mode.primaryProviderID
            )
            await appModel.planRoute(using: mode, preferredTitle: searchController.query.isEmpty ? activeNavigationTitle : searchController.query)
            appModel.routeHistoryService.recordPlannedPreview(source: .plannedRoute, sourceLabel: mode.displayName)
        }
    }

    func selectAlternative(_ alternativeID: UUID) {
        if isExploringAlternativesFromGuidance {
            explorationSelectedID = alternativeID
            appModel.selectAlternativePreviewOnly(alternativeID)
        } else {
            appModel.selectAlternative(alternativeID)
        }
    }

    func startSelectedRoute() async {
        searchController.closeSearch()
        guard let selectedPreview else { return }
        activeRouteIdentifier = selectedPreview.normalizedPackage.routeIdentifier
        cancelArrivalNoticeAutoDismiss()
        arrivalNotice = nil
        offRouteTracker.reset()
        if appModel.deviceManager.isDeviceConnected {
            homeMode = .sendingToDevice
            let success = await appModel.routeSyncService.sendSelectedRoute(
                preview: appModel.preview,
                routeRequest: appModel.routeRequest,
                sourceMode: sourceMode
            )
            homeMode = success ? .deviceOverview : .planning
        } else {
            let pkg = selectedPreview.normalizedPackage
            // Batch the session-field updates into one struct re-assignment so
            // SessionManager fires `objectWillChange` once instead of 5-6 times.
            var session = appModel.sessionManager.session
            session.routeIdentifier = pkg.routeIdentifier
            session.routeRevision = pkg.revision
            session.destinationCoordinate = pkg.geometry.last ?? session.destinationCoordinate
            session.providerID = pkg.provenance.providerID
            session.sourceMode = sourceMode
            if let label = pkg.summary.destinationLabel {
                let placeholderTitles: Set<String> = [
                    "", "No destination", "Selected destination", "Current location",
                ]
                let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
                let sessionIsPlaceholder = placeholderTitles.contains(
                    session.destinationLabel.trimmingCharacters(in: .whitespacesAndNewlines)
                )
                let labelIsMeaningful = !placeholderTitles.contains(trimmed)
                if labelIsMeaningful && (sessionIsPlaceholder || session.destinationLabel.isEmpty) {
                    session.destinationLabel = trimmed
                }
            }
            appModel.sessionManager.session = session
            homeMode = .phoneGuidance
            compassMode = .autoFollow
            isExploringAlternativesFromGuidance = false
            explorationSelectedID = nil
            activeRoutePackage = selectedPreview.normalizedPackage
            offRouteTracker.setRouteTotalDistance(PolylineGeo.polylineLengthMeters(selectedPreview.normalizedPackage.geometry))
            syncOffRouteState()
            appModel.activeGuidanceRoute = guidanceRoute
            appModel.isRoutingInProgress = true
            dispatchCueTick()
        }
        appModel.routingDiagnosticsStore.recordEvent(.routeStarted)
        appModel.routingDiagnosticsStore.recordEvent(.routeSelected(
            alternativeId: selectedPreview.id.uuidString,
            providerName: selectedPreview.normalizedPackage.provenance.providerID.rawValue,
            routeId: selectedPreview.normalizedPackage.routeIdentifier,
            label: selectedPreview.title
        ))
        let geom = selectedPreview.normalizedPackage.geometry
        if !geom.isEmpty {
            appModel.routingDiagnosticsStore.recordRouteGeometry(
                routeId: selectedPreview.normalizedPackage.routeIdentifier,
                providerName: selectedPreview.normalizedPackage.provenance.providerID.rawValue,
                geometry: geom
            )
        }
    }

    func stopActiveNavigation(afterArrival: Bool = false) {
        appModel.routingDiagnosticsStore.recordEvent(.routeStopped(reason: afterArrival ? "arrival" : "manual"))
        if appModel.routingDiagnosticsStore.isRecording {
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
                self?.appModel.routingDiagnosticsStore.stopRecording()
            }
        }
        Task {
            var shouldClearPlanningStatus = false
            let destination = destinationCoordinate
            let sourceToReuse = appModel.sessionManager.session.sourceMode
            let shouldPreserveCurrentPreview = isPreviewLockedToImportedRoute
            if homeMode == .deviceOverview || homeMode == .sendingToDevice {
                _ = await appModel.routeSyncService.clearActiveRoute()
            }
            compassMode = .autoFollow
            activeRouteIdentifier = nil
            homeMode = .planning
            offRouteTracker.cancelPendingReroute()
            offRouteTracker.reset()
            syncOffRouteState()
            var session = appModel.sessionManager.session
            session.routeIdentifier = nil
            session.routeRevision = nil
            appModel.sessionManager.session = session
            appModel.isRoutingInProgress = false
            dispatchCueTick()
            if afterArrival {
                searchController.query = ""
                appModel.preview = RoutePreviewModel(
                    alternatives: [],
                    selectedAlternativeID: nil,
                    routeIdentifier: nil,
                    routeRevision: nil,
                    planningNotice: nil
                )
                return
            }
            guard !shouldPreserveCurrentPreview else { return }
            guard let destination else { return }
            planningStatus = "Refreshing route options…"
            shouldClearPlanningStatus = true
            defer {
                if shouldClearPlanningStatus {
                    planningStatus = nil
                }
            }
            appModel.routeRequest = RoutePlanRequest(
                origin: appModel.locationService.bestLocation,
                destination: destination,
                providerID: sourceToReuse.primaryProviderID
            )
            await appModel.planRoute(using: sourceToReuse, preferredTitle: searchController.query.isEmpty ? activeNavigationTitle : searchController.query)
            appModel.routeHistoryService.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceToReuse.displayName)
        }
    }

    func clearPreview() {
        searchController.cancelActiveSearch()
        activeRouteIdentifier = nil
        homeMode = .planning
        compassMode = .autoFollow
        planningStatus = nil
        searchController.query = ""
        searchController.suggestions = []
        searchController.closeSearch()
        appModel.routeHistoryService.clearPreviewSelection()
    }

    func exploreAlternateRoutes() {
        guard homeMode == .phoneGuidance,
              let destination = appModel.sessionManager.session.destinationCoordinate
        else { return }
        let sourceToReuse = appModel.sessionManager.session.sourceMode
        let titleHint = appModel.sessionManager.session.destinationLabel
        compassMode = .northLocked
        isExploringAlternativesFromGuidance = true
        explorationSelectedID = nil
        planningStatus = "Looking for alternatives…"
        appModel.routingDiagnosticsStore.recordEvent(.exploreAlternatives)
        appModel.routeRequest = RoutePlanRequest(
            origin: appModel.locationService.bestLocation,
            destination: destination,
            providerID: sourceToReuse.primaryProviderID
        )
        Task {
            defer {
                if planningStatus == "Looking for alternatives…" {
                    planningStatus = nil
                }
            }
            await appModel.planRoute(using: sourceToReuse, preferredTitle: titleHint)
            appModel.routeHistoryService.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceToReuse.displayName)
        }
    }

    func cancelAlternativesExploration() {
        isExploringAlternativesFromGuidance = false
        explorationSelectedID = nil
        compassMode = .autoFollow
    }

    func deselectForExploration() {
        guard isExploringAlternativesFromGuidance else { return }
        explorationSelectedID = nil
    }

    func handleDeviceChipTap() {
        guard let state = deviceChipState else { return }
        switch state {
        case .pairedDisconnected:
            Task { await appModel.connectToDevice() }
        case .connecting:
            break
        case .connected:
            showConnectionPopover = true
        }
    }

    func handleCompassTap() {
        mapRecenterRequestID &+= 1
        guard homeMode == .phoneGuidance else { return }
        let prevCompassMode = String(describing: compassMode)
        switch compassMode {
        case .autoFollow:
            enterNorthLocked()
        case .northPreview:
            enterNorthLocked()
        case .northLocked:
            compassMode = .autoFollow
        }
        let newCompassMode = String(describing: compassMode)
        if newCompassMode != prevCompassMode {
            appModel.routingDiagnosticsStore.recordEvent(.compassModeChanged(from: prevCompassMode, to: newCompassMode))
        }
    }

    func handleCompassDoubleTap() {
        guard homeMode == .phoneGuidance else { return }
        enterNorthLocked()
    }

    func routingBearingDegrees(rider: CoordinatePoint?) -> CLLocationDirection? {
        guard let geometry = guidanceRoute?.geometry, geometry.count >= 2 else {
            return nil
        }
        let progressM = rider.map { PolylineGeo.projectProgress(onto: geometry, rider: $0) } ?? 0.0
        var traversed = 0.0
        for i in 0..<(geometry.count - 1) {
            let segLen = PolylineGeo.straightLineMeters(geometry[i], geometry[i + 1])
            if segLen < 1e-6 { continue }
            if progressM < traversed + segLen {
                return bearingDegrees(from: geometry[i], to: geometry[i + 1])
            }
            traversed += segLen
        }
        return bearingDegrees(from: geometry[geometry.count - 2], to: geometry[geometry.count - 1])
    }

    func notifyRiderLocationUpdated() {
        let inRouting = homeMode == .phoneGuidance
        let moving = travelHeadingDegrees != nil
        guard inRouting || moving else { return }
        mapFollowRiderTick &+= 1
    }

    func ingestRiderLocationFix(_ point: CoordinatePoint, timestampMs: Int64) {
        headingTrail.recordFix(point, timestampMs: timestampMs)
        if homeMode == .phoneGuidance {
            advanceProgress(rider: point, nowMs: timestampMs)
        }
    }

    func cameraHeadingDegrees(rider: CoordinatePoint?) -> Double? {
        if let trail = headingTrail.travelHeadingDegrees {
            return trail
        }
        return routingBearingDegrees(rider: rider)
    }

    func noteUserMapInteraction() {
        guard homeMode == .phoneGuidance else { return }
        isUserInteractingWithMap = true
        mapInteractionRecenterTask?.cancel()
        let delay = mapInteractionRecenterDelay
        mapInteractionRecenterTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard let self, !Task.isCancelled else { return }
            guard self.homeMode == .phoneGuidance else {
                self.isUserInteractingWithMap = false
                return
            }
            self.isUserInteractingWithMap = false
            self.mapRecenterRequestID &+= 1
        }
    }

    func recordReroutingAttempt(now: Double) -> Double {
        let delayMs = offRouteTracker.recordReroutingAttempt(now: now)
        syncOffRouteState()
        return delayMs
    }

    func isWaitingToReroute(now: Double) -> Bool {
        offRouteTracker.isWaitingToReroute(now: now)
    }

    func requestManualReroute() {
        offRouteTracker.requestManualReroute()
        syncOffRouteState()
    }

    func dismissArrivalNotice() {
        cancelArrivalNoticeAutoDismiss()
        arrivalNotice = nil
    }

    func dispatchCueTick() {
        CueDispatcher.dispatch(
            isExploringAlternativesFromGuidance: isExploringAlternativesFromGuidance,
            guidanceRoute: guidanceRoute,
            routeId: routeKey(),
            progressDistanceM: progressDistanceM,
            routeTotalDistanceM: routeTotalDistanceM,
            offRoute: offRoute,
            rerouteRequested: rerouteRequested,
            arrivalNotice: arrivalNotice,
            offRouteDistanceM: offRouteDistanceM,
            isDeviceConnectedForCueSuppression: appModel.deviceManager.isDeviceConnectedForCueSuppression,
            routingActivityCoordinator: appModel.routingActivityCoordinator,
            liveActivityCoordinator: appModel.liveActivityCoordinator,
            isRoutingInProgress: appModel.isRoutingInProgress,
            isAppInBackground: appModel.isAppInBackground,
            settings: appModel.settings,
            onSetActiveGuidanceRoute: { appModel.activeGuidanceRoute = $0 }
        )
    }

    private func enterNorthLocked() {
        let prev = String(describing: compassMode)
        compassMode = .northLocked
        appModel.routingDiagnosticsStore.recordEvent(.compassModeChanged(from: prev, to: "northLocked"))
    }

    private func routeKey() -> String? {
        guard let id = appModel.sessionManager.session.routeIdentifier else { return nil }
        let rev = appModel.sessionManager.session.routeRevision ?? 0
        return "\(id)-rev\(rev)"
    }

    private func syncOffRouteState() {
        offRouteDistanceM = offRouteTracker.offRouteDistanceM
        offRoute = offRouteTracker.offRoute
        rerouteRequested = offRouteTracker.rerouteRequested
        reroutingAttemptTimestampsMs = offRouteTracker.reroutingAttemptTimestampsMs
        reroutingDelayedUntilMs = offRouteTracker.reroutingDelayedUntilMs
        progressDistanceM = offRouteTracker.progressDistanceM
        routeTotalDistanceM = offRouteTracker.routeTotalDistanceM
    }

    private func markAutoRerouteDispatched() {
        offRouteTracker.markAutoRerouteDispatched()
        syncOffRouteState()
    }

    private func advanceProgress(rider: CoordinatePoint, nowMs: Int64) {
        guard let geometry = (guidanceRoute ?? activeRoutePackage)?.geometry, geometry.count >= 2 else { return }
        offRouteTracker.advanceProgress(
            rider: rider,
            nowMs: nowMs,
            geometry: geometry,
            routeKey: routeKey(),
            travelHeadingDegrees: travelHeadingDegrees,
            speedMps: appModel.locationService.currentSpeedMps
        )
        syncOffRouteState()
        dispatchCueTick()
    }

    private func declareArrival() {
        arrivalNotice = "Arrived at destination"
        scheduleArrivalNoticeAutoDismiss()
        stopActiveNavigation(afterArrival: true)
    }

    private func scheduleArrivalNoticeAutoDismiss() {
        cancelArrivalNoticeAutoDismiss()
        let delay = HomeViewModel.arrivalNoticeAutoDismissDelayForTesting
            ?? HomeViewModel.arrivalNoticeAutoDismissDelay
        arrivalNoticeAutoDismissTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard let self, !Task.isCancelled else { return }
            self.arrivalNotice = nil
        }
    }

    private func cancelArrivalNoticeAutoDismiss() {
        arrivalNoticeAutoDismissTask?.cancel()
        arrivalNoticeAutoDismissTask = nil
    }
}
