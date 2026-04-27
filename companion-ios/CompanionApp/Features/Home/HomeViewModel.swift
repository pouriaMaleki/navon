import CoreLocation
import Foundation

@MainActor
final class HomeViewModel: ObservableObject {
    func syncQueryFromCurrentPreview() {
        let sessionTitle = appModel.activeSession.destinationLabel.trimmingCharacters(in: .whitespacesAndNewlines)
        if !sessionTitle.isEmpty, sessionTitle != "No destination" {
            query = sessionTitle
        } else if let destination = appModel.preview.selectedAlternative?.normalizedPackage.summary.destinationLabel, !destination.isEmpty {
            query = destination
        } else if let route = appModel.preview.selectedAlternative {
            query = route.title
        }
    }

    func revealImportedPreview() async {
        latestSearchTask?.cancel()
        northPreviewTask?.cancel()
        activeRouteIdentifier = nil
        homeMode = .planning
        compassMode = .autoFollow
        suggestions = []
        closeSearch()

        if let pending = appModel.pendingHomeImportPresentation {
            planningStatus = "Opening imported destination…"
            defer { planningStatus = nil }
            if let item = appModel.routeHistoryItems.first(where: { $0.id == pending.routeHistoryItemID }) {
                await appModel.applyRouteHistoryPreview(item)
            } else if let destination = pending.destination {
                appModel.routeRequest = RoutePlanRequest(
                    origin: appModel.riderLocation,
                    destination: destination,
                    providerID: sourceMode.primaryProviderID
                )
                await appModel.planRoute(using: sourceMode, preferredTitle: pending.title)
            }
            query = pending.title
            appModel.clearPendingHomeImportPresentation()
            return
        }

        syncQueryFromCurrentPreview()
    }

    @Published var query = ""
    @Published var isSearchOpen = false
    @Published var suggestions: [DestinationSearchResult] = []
    @Published var visibleSuggestionCount = 10
    @Published var visibleRecentCount = 10
    @Published var activeRouteIdentifier: String?
    @Published var homeMode: HomeMode = .planning
    @Published var compassMode: HomeCompassMode = .autoFollow
    @Published private(set) var planningStatus: String?
    /// True while a pasted URL (e.g. maps.app.goo.gl) is being followed to a destination.
    @Published private(set) var isResolvingUrl: Bool = false
    /// Last URL-resolve failure message for the search panel.
    @Published private(set) var urlResolveError: String?
    /// Monotonic counter the map view observes to recenter on the rider.
    /// Spec line 39: companion-only side-effect of a compass tap. Also
    /// bumped by `noteUserMapInteraction` after the inactivity timeout
    /// (spec line 104).
    @Published private(set) var mapRecenterRequestID: Int = 0
    /// Monotonic counter the map view observes to follow the rider on
    /// every GPS update during routing (spec line 84).
    @Published private(set) var mapFollowRiderTick: Int = 0
    private var latestUrlResolveTask: Task<Void, Never>?
    private var mapInteractionRecenterTask: Task<Void, Never>?
    /// Spec line 110 (authoritative): smoothed travel heading from the last
    /// few GPS fixes. When available, overrides the route-segment bearing
    /// so the camera rotates to the rider's actual direction of travel.
    /// Parameters match runtime-core / companion-web (3m floor, α=0.25).
    private let headingTrail = HeadingTrail(
        maxAgeMs: 5_000, maxFixes: 10,
        minDisplacementM: 3.0, smoothingAlpha: 0.25
    )
    /// Pinned auto-recenter delay for user map interactions during routing.
    /// Mirrors `recenter_inactivity_ms` in parity-fixtures/data/ux-constants.toml
    /// (spec line 104).
    private let mapInteractionRecenterDelay: TimeInterval = 1.3

    private let appModel: AppModel
    private let placeSearchService: PlaceSearchService
    private var latestSearchTask: Task<Void, Never>?
    private var northPreviewTask: Task<Void, Never>?
    private let switchablePlanningProviders: Set<RouteProviderID> = [.hsl, .osm]

    init(appModel: AppModel, placeSearchService: PlaceSearchService = MapKitPlaceSearchService()) {
        self.appModel = appModel
        self.placeSearchService = placeSearchService
    }

    var plannerPreferences: RoutePlannerPreferences {
        get { appModel.routePlannerPreferences }
        set { appModel.routePlannerPreferences = newValue }
    }

    var sourceMode: RouteSourceMode {
        get { appModel.currentSourceMode }
        set {
            appModel.currentSourceMode = newValue
            plannerPreferences = RoutePlannerPreferences(
                defaultSourceMode: newValue,
                suggestionMode: plannerPreferences.suggestionMode,
                startBehavior: plannerPreferences.startBehavior
            )
        }
    }

    var recentItems: [RouteHistoryItem] {
        Array(appModel.routeHistoryItems.prefix(visibleRecentCount))
    }

    var visibleSuggestions: [DestinationSearchResult] {
        Array(suggestions.prefix(visibleSuggestionCount))
    }

    var previewAlternatives: [RouteAlternative] {
        let limit = plannerPreferences.suggestionMode == .bestOnly ? 1 : 3
        return Array(appModel.preview.alternatives.prefix(limit))
    }

    var selectedPreview: RouteAlternative? {
        appModel.preview.selectedAlternative
    }

    var guidanceRoute: NormalizedRoutePackage? {
        switch homeMode {
        case .phoneGuidance, .deviceOverview, .sendingToDevice:
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

    var shouldShowSearchPanel: Bool {
        guard homeMode == .planning, isSearchOpen else { return false }
        if isResolvingUrl || urlResolveError != nil { return true }
        let trimmedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedQuery.isEmpty {
            return !recentItems.isEmpty
        }
        return !visibleSuggestions.isEmpty
    }

    var shouldShowSourceControl: Bool {
        homeMode == .planning
            && !previewAlternatives.isEmpty
            && !isPreviewLockedToImportedRoute
            && appModel.sourceModeOptions.count > 1
    }

    var routeSuggestionsTitle: String {
        isPreviewLockedToImportedRoute ? "Imported route" : "Suggested routes"
    }

    var isShowingActiveNavigation: Bool {
        homeMode == .phoneGuidance || homeMode == .deviceOverview || homeMode == .sendingToDevice
    }

    var startButtonTitle: String {
        switch homeMode {
        case .sendingToDevice:
            return "Starting on device…"
        case .planning:
            return appModel.isDeviceConnected ? "Start on device" : "Start"
        case .phoneGuidance, .deviceOverview:
            return "Start"
        }
    }

    var activeNavigationTitle: String {
        if let destination = guidanceRoute?.summary.destinationLabel {
            return destination
        }
        return appModel.activeSession.destinationLabel
    }

    var activeNavigationSubtitle: String {
        switch homeMode {
        case .phoneGuidance:
            // Mirror web's `activeNavigationSubtitle`: "X km remaining • Y min".
            // The destination address is now bundled into `guidanceSubtitleLine`
            // (the top card) so the bottom no longer shows it.
            let remaining = remainingDistanceM
            if remaining > 0 {
                let km = String(format: "%.1f", remaining / 1000)
                let etaMin = max(1, Int(ceil(remainingDurationSeconds / 60.0)))
                return "\(km) km remaining • \(etaMin) min"
            }
            if let route = guidanceRoute {
                let km = String(format: "%.1f", route.summary.totalDistanceMeters / 1000)
                let min = max(1, route.summary.estimatedDurationSeconds / 60)
                return "\(km) km • \(min) min"
            }
            return "Phone guidance ready"
        case .deviceOverview, .sendingToDevice:
            return appModel.bleService.sessionState.lastSyncResult
        case .planning:
            return selectedPreview?.normalizedPackage.summaryLine ?? ""
        }
    }

    /// Single line: destination address + remaining distance + remaining
    /// minutes. Pinned as the subtitle of the top guidance card so the
    /// bottom no longer needs to repeat the same information; it shows
    /// only a floating Stop button. Empty destination labels are dropped
    /// so the line never starts with a stray separator.
    var guidanceSubtitleLine: String {
        let raw = guidanceRoute?.summary.destinationLabel ?? appModel.activeSession.destinationLabel
        let destination = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let remainingPart = activeNavigationSubtitle
        if destination.isEmpty || destination == "No destination" {
            return remainingPart
        }
        return "\(destination) • \(remainingPart)"
    }

    /// Remaining distance (m) along the active route from the rider's current
    /// progress. Returns 0 when there's no active route.
    var remainingDistanceM: Double {
        guard routeTotalDistanceM > 0 else { return 0 }
        return max(0, routeTotalDistanceM - progressDistanceM)
    }

    /// Estimated remaining duration (s), proportional to the fraction of the
    /// route still to cover. Mirrors web `remainingDurationSeconds`.
    var remainingDurationSeconds: Double {
        guard let route = guidanceRoute, routeTotalDistanceM > 0 else { return 0 }
        let fraction = max(0, 1 - progressDistanceM / routeTotalDistanceM)
        return Double(route.summary.estimatedDurationSeconds) * fraction
    }

    /// Geometry the route-overview camera should fit when the user taps the
    /// compass during routing. Equals the remaining route ahead of the
    /// rider — late in a ride the start segment is no longer relevant, so
    /// including it would zoom the camera out unnecessarily. Falls back to
    /// the full geometry before any progress.
    var routeOverviewGeometry: [CoordinatePoint] {
        guard let geometry = guidanceRoute?.geometry, !geometry.isEmpty else { return [] }
        let split = splitPolylineAtDistance(geometry, distance: progressDistanceM)
        if split.remaining.count >= 2 { return split.remaining }
        return geometry
    }

    /// Set when the rider has reached the destination and guidance has been
    /// auto-stopped. Cleared on the next route start.
    @Published var arrivalNotice: String?

    var nextInstructionLine: String? {
        // Spec line 102: as the rider advances along the route, the
        // displayed line must switch to the NEXT upcoming maneuver. We
        // skip depart/arrive turn-types and pick the first maneuver
        // whose distance-from-start is still ahead of `progressDistanceM`.
        // Distance shown is the REMAINING distance to that maneuver, not
        // the static distance-from-route-start.
        guard let route = guidanceRoute else { return nil }
        for m in route.maneuvers {
            if m.maneuverType == .depart || m.maneuverType == .arrive { continue }
            let remaining = m.distanceFromStartMeters - progressDistanceM
            if remaining < 0 { continue }
            let instruction = m.instructionText ?? "Continue"
            return "\(instruction) • \(formatDistance(remaining))"
        }
        // Past the last interesting maneuver: show "Arrive" remaining.
        if let arrive = route.maneuvers.last(where: { $0.maneuverType == .arrive }) {
            let remaining = max(0, arrive.distanceFromStartMeters - progressDistanceM)
            return "Arrive in \(formatDistance(remaining))"
        }
        return nil
    }

    private func formatDistance(_ meters: Double) -> String {
        if meters >= 1000 { return String(format: "%.1f km", meters / 1000) }
        return "\(Int(meters.rounded())) m"
    }

    private func polylineLengthMeters(_ points: [CoordinatePoint]) -> Double {
        guard points.count >= 2 else { return 0 }
        let metersPerDegLat = 111_320.0
        var total = 0.0
        for i in 0..<(points.count - 1) {
            let a = points[i]
            let b = points[i + 1]
            let meanLat = (a.latitude + b.latitude) / 2.0 * .pi / 180.0
            let dN = (b.latitude - a.latitude) * metersPerDegLat
            let dE = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegLat
            total += (dN * dN + dE * dE).squareRoot()
        }
        return total
    }

    var compassSymbolName: String {
        switch compassMode {
        case .autoFollow:
            return "location.fill"
        case .northPreview:
            return "location.north.line.fill"
        case .northLocked:
            return "location.north.line.fill"
        }
    }

    /// Short window after `selectSuggestion` during which `openSearch()` is
    /// absorbed. The user-reported bug is the dropdown re-opens visually
    /// after a pick because the SwiftUI TextField still holds focus and
    /// re-fires onFocus on the next layout pass. This latch swallows that
    /// follow-up open while a legitimate "user explicitly tapped the
    /// input again" intent fires after the window closes.
    private let postSelectionLatchSeconds: TimeInterval = 0.35
    private var postSelectionLatchUntil: Date = .distantPast

    func openSearch() {
        guard homeMode == .planning else { return }
        if Date() < postSelectionLatchUntil { return }
        isSearchOpen = true
        visibleRecentCount = 10
        visibleSuggestionCount = 10
    }

    func closeSearch() {
        isSearchOpen = false
        cancelUrlResolve()
        urlResolveError = nil
    }

    func loadMoreRecentsIfNeeded(for item: RouteHistoryItem) {
        if item.id == recentItems.last?.id {
            visibleRecentCount += 10
        }
    }

    func loadMoreSuggestionsIfNeeded(for item: DestinationSearchResult) {
        if item.id == visibleSuggestions.last?.id {
            visibleSuggestionCount += 10
        }
    }

    func updateQuery(_ newValue: String) {
        guard homeMode == .planning else { return }
        query = newValue
        visibleSuggestionCount = 10
        latestSearchTask?.cancel()
        let trimmed = newValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            suggestions = []
            urlResolveError = nil
            cancelUrlResolve()
            return
        }
        if trimmed.lowercased().hasPrefix("http://") || trimmed.lowercased().hasPrefix("https://") {
            suggestions = []
            startUrlResolve(trimmed)
            return
        }
        urlResolveError = nil
        cancelUrlResolve()
        latestSearchTask = Task { [weak self] in
            guard let self else { return }
            // Spec line 75: bias typeahead toward the rider's area so
            // same-city results rank first.
            let bias = appModel.locationService.currentLocation
                ?? appModel.locationService.lastKnownLocation
            let results = await placeSearchService.searchDestinations(
                matching: newValue,
                limit: 30,
                riderBias: bias
            )
            if !Task.isCancelled {
                suggestions = results
            }
        }
    }

    private func cancelUrlResolve() {
        latestUrlResolveTask?.cancel()
        latestUrlResolveTask = nil
        isResolvingUrl = false
    }

    private func startUrlResolve(_ urlString: String) {
        latestUrlResolveTask?.cancel()
        isResolvingUrl = true
        urlResolveError = nil
        latestUrlResolveTask = Task { [weak self] in
            guard let self else { return }
            let resolution = await appModel.resolveDestinationFromUrl(urlString, using: placeSearchService)
            if Task.isCancelled { return }
            isResolvingUrl = false
            switch resolution {
            case .coordinate(let point, let suggestedTitle):
                let title = suggestedTitle ?? "Imported destination"
                appModel.routeRequest = RoutePlanRequest(
                    origin: appModel.riderLocation,
                    destination: point,
                    providerID: sourceMode.primaryProviderID
                )
                closeSearch()
                planningStatus = "Planning route to \(title)…"
                defer { planningStatus = nil }
                await appModel.planRoute(using: sourceMode, preferredTitle: title)
                appModel.recordRecentDestination(title: title, coordinate: point)
                appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceMode.displayName)
                query = title
            case .noDestinationFound:
                urlResolveError = "Couldn't find a destination in that URL."
            case .networkError(let message):
                urlResolveError = "URL expansion failed: \(message)"
            }
        }
    }

    func selectSuggestion(_ suggestion: DestinationSearchResult) {
        latestSearchTask?.cancel()
        closeSearch()
        postSelectionLatchUntil = Date().addingTimeInterval(postSelectionLatchSeconds)
        Task {
            planningStatus = "Planning route to \(suggestion.title)…"
            defer { planningStatus = nil }
            appModel.routeRequest = RoutePlanRequest(
                origin: appModel.riderLocation,
                destination: suggestion.coordinate,
                providerID: sourceMode.primaryProviderID
            )
            await appModel.planRoute(using: sourceMode, preferredTitle: suggestion.title)
            appModel.recordRecentDestination(title: suggestion.title, coordinate: suggestion.coordinate)
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceMode.displayName)
            query = suggestion.title
            homeMode = .planning
            if plannerPreferences.startBehavior == .automatic {
                await startSelectedRoute()
            }
        }
    }

    func selectRecent(_ item: RouteHistoryItem) {
        latestSearchTask?.cancel()
        closeSearch()
        Task {
            planningStatus = "Opening \(item.title)…"
            defer { planningStatus = nil }
            await appModel.applyRouteHistoryPreview(item)
            query = item.title
            homeMode = .planning
            if plannerPreferences.startBehavior == .automatic {
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

    func setSourceMode(_ mode: RouteSourceMode) {
        guard shouldShowSourceControl, sourceMode != mode else { return }
        sourceMode = mode
        guard let destination = destinationCoordinate else { return }
        Task {
            planningStatus = "Refreshing route options…"
            defer { planningStatus = nil }
            appModel.routeRequest = RoutePlanRequest(
                origin: appModel.riderLocation,
                destination: destination,
                providerID: mode.primaryProviderID
            )
            await appModel.planRoute(using: mode, preferredTitle: query.isEmpty ? activeNavigationTitle : query)
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: mode.displayName)
        }
    }

    func selectAlternative(_ alternativeID: UUID) {
        appModel.selectAlternative(alternativeID)
    }

    func startSelectedRoute() async {
        closeSearch()
        guard let selectedPreview else { return }
        activeRouteIdentifier = selectedPreview.normalizedPackage.routeIdentifier
        // Starting a new route clears any stale arrival banner from the
        // previous trip.
        arrivalNotice = nil
        if appModel.isDeviceConnected {
            homeMode = .sendingToDevice
            let success = await appModel.sendSelectedRoute()
            homeMode = success ? .deviceOverview : .planning
        } else {
            appModel.activeSession.sourceMode = sourceMode
            homeMode = .phoneGuidance
            compassMode = .autoFollow
            // Reset progress for the new route so spec line 102 (next-turn
            // tracking) starts at 0 and advances as the rider proceeds.
            progressDistanceM = 0
            routeTotalDistanceM = polylineLengthMeters(selectedPreview.normalizedPackage.geometry)
        }
    }

    func stopActiveNavigation() {
        Task {
            var shouldClearPlanningStatus = false
            let destination = destinationCoordinate
            let sourceToReuse = appModel.activeSession.sourceMode
            let shouldPreserveCurrentPreview = isPreviewLockedToImportedRoute
            if homeMode == .deviceOverview || homeMode == .sendingToDevice {
                _ = await appModel.clearActiveRoute()
            }
            northPreviewTask?.cancel()
            compassMode = .autoFollow
            activeRouteIdentifier = nil
            homeMode = .planning
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
                origin: appModel.riderLocation,
                destination: destination,
                providerID: sourceToReuse.primaryProviderID
            )
            await appModel.planRoute(using: sourceToReuse, preferredTitle: query.isEmpty ? activeNavigationTitle : query)
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceToReuse.displayName)
        }
    }

    func clearPreview() {
        latestSearchTask?.cancel()
        northPreviewTask?.cancel()
        activeRouteIdentifier = nil
        homeMode = .planning
        compassMode = .autoFollow
        planningStatus = nil
        query = ""
        suggestions = []
        closeSearch()
        appModel.clearPreviewSelection()
    }

    func handleCompassTap() {
        guard homeMode == .phoneGuidance else { return }
        // Spec line 39: on companion apps a compass tap also recenters the
        // camera. Bump the request id so the map view observes the change.
        mapRecenterRequestID &+= 1
        switch compassMode {
        case .autoFollow:
            enterNorthLocked()
        case .northPreview:
            enterNorthLocked()
        case .northLocked:
            northPreviewTask?.cancel()
            compassMode = .autoFollow
        }
    }

    func handleCompassDoubleTap() {
        guard homeMode == .phoneGuidance else { return }
        enterNorthLocked()
    }

    private func enterNorthLocked() {
        northPreviewTask?.cancel()
        compassMode = .northLocked
    }

    /// Compass-heading bearing (degrees clockwise from north) of the route
    /// segment the rider is currently progressed onto. Spec line 101: the
    /// camera rotates so "immediate route direction is towards top of the
    /// screen (riding towards, even when stationary yet)". Returns nil if
    /// there's no active route geometry.
    func routingBearingDegrees(rider: CoordinatePoint?) -> CLLocationDirection? {
        guard let geometry = guidanceRoute?.geometry, geometry.count >= 2 else {
            return nil
        }
        let metersPerDegreeLat = 111_320.0
        func lengthMeters(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Double {
            let latMeters = (b.latitude - a.latitude) * metersPerDegreeLat
            let meanLat = ((a.latitude + b.latitude) / 2.0) * .pi / 180.0
            let lonMeters = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
            return sqrt(latMeters * latMeters + lonMeters * lonMeters)
        }
        func bearing(_ a: CoordinatePoint, _ b: CoordinatePoint) -> CLLocationDirection {
            let latMeters = (b.latitude - a.latitude) * metersPerDegreeLat
            let meanLat = ((a.latitude + b.latitude) / 2.0) * .pi / 180.0
            let lonMeters = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
            return atan2(lonMeters, latMeters) * 180.0 / .pi
        }
        // Project rider onto polyline to find the current progress distance.
        // Fall back to start-of-route if no rider is provided.
        let progressM = rider.map { projectProgress(onto: geometry, rider: $0) } ?? 0.0
        var traversed = 0.0
        for i in 0..<(geometry.count - 1) {
            let segLen = lengthMeters(geometry[i], geometry[i + 1])
            if segLen < 1e-6 { continue }
            // Strict `<` so the rider exactly at a vertex snaps to the NEXT
            // segment (spec: riding TOWARDS).
            if progressM < traversed + segLen {
                return bearing(geometry[i], geometry[i + 1])
            }
            traversed += segLen
        }
        // Past the end — use the last segment.
        return bearing(geometry[geometry.count - 2], geometry[geometry.count - 1])
    }

    /// Project `rider` onto `polyline` and return the distance along the
    /// polyline to the closest projected point. Small local copy of the
    /// web `projectOntoPolyline` helper.
    private func projectProgress(onto polyline: [CoordinatePoint], rider: CoordinatePoint) -> Double {
        guard polyline.count >= 2 else { return 0.0 }
        let metersPerDegreeLat = 111_320.0
        var bestDistSq = Double.infinity
        var bestProgress = 0.0
        var traversed = 0.0
        for i in 0..<(polyline.count - 1) {
            let a = polyline[i]
            let b = polyline[i + 1]
            let meanLat = ((a.latitude + rider.latitude) / 2.0) * .pi / 180.0
            let cosLat = cos(meanLat)
            let endX = (b.longitude - a.longitude) * cosLat * metersPerDegreeLat
            let endY = (b.latitude - a.latitude) * metersPerDegreeLat
            let riderX = (rider.longitude - a.longitude) * cosLat * metersPerDegreeLat
            let riderY = (rider.latitude - a.latitude) * metersPerDegreeLat
            let segLenSq = endX * endX + endY * endY
            guard segLenSq > 1e-12 else { continue }
            let t = max(0.0, min(1.0, (riderX * endX + riderY * endY) / segLenSq))
            let projX = t * endX
            let projY = t * endY
            let dx = riderX - projX
            let dy = riderY - projY
            let distSq = dx * dx + dy * dy
            if distSq < bestDistSq {
                bestDistSq = distSq
                let segLen = sqrt(segLenSq)
                bestProgress = traversed + segLen * t
            }
            traversed += sqrt(segLenSq)
        }
        return bestProgress
    }

    /// Called on every new rider-location update. Spec line 84 (during
    /// routing) AND spec lines 108-118 ("when moving with or without a
    /// route"): bump the follow-rider tick whenever the rider is in a
    /// state that wants riding-mode camera. The Map view's onChange
    /// then runs `refreshCameraForCurrentMode` which respects mode +
    /// trail heading.
    func notifyRiderLocationUpdated() {
        let inRouting = homeMode == .phoneGuidance
        let moving = travelHeadingDegrees != nil
        guard inRouting || moving else { return }
        mapFollowRiderTick &+= 1
    }

    /// Feed a GPS fix into the trail buffer + advance routing progress.
    /// Used by the location-service callback and the test harness. Drives
    /// spec line 110 (GPS-derived camera rotation) and spec line 102
    /// (next-turn instruction tracking).
    func ingestRiderLocationFix(_ point: CoordinatePoint, timestampMs: Int64) {
        headingTrail.recordFix(point, timestampMs: timestampMs)
        if homeMode == .phoneGuidance {
            advanceProgress(rider: point)
        }
    }

    /// Monotonic distance (m) the rider has progressed along the active
    /// route, projecting the latest fix onto the polyline. Drives spec
    /// line 102 (next-turn switching as the rider passes maneuvers).
    @Published private(set) var progressDistanceM: Double = 0

    /// Total length of the active route in meters. Cached on Start.
    private var routeTotalDistanceM: Double = 0

    /// Project the rider onto the active route geometry and advance
    /// `progressDistanceM` monotonically (never regresses), mirroring
    /// runtime-core's behaviour.
    private func advanceProgress(rider: CoordinatePoint) {
        guard let geometry = guidanceRoute?.geometry, geometry.count >= 2 else { return }
        let projected = projectProgress(onto: geometry, rider: rider)
        let bounded = min(routeTotalDistanceM > 0 ? routeTotalDistanceM : projected, projected)
        progressDistanceM = max(progressDistanceM, bounded)
        // Spec: when the rider arrives at the destination, end routing.
        // Use straight-line distance to the last geometry vertex so a rider
        // approaching from any side trips arrival, not just one travelling
        // along-route.
        if let last = geometry.last,
           Self.straightLineMeters(last, rider) <= Self.arrivalRadiusM {
            declareArrival()
        }
    }

    /// Larger than the off-route exit distance so a rider drifting around
    /// the destination doesn't bounce between "off route" and "arrived".
    private static let arrivalRadiusM: Double = 25

    private func declareArrival() {
        arrivalNotice = "Arrived at destination"
        // Reuse the same teardown as a manual stop — keep persistence + UI
        // intent consistent. arrivalNotice stays set because stopActiveNavigation
        // does not clear it.
        stopActiveNavigation()
    }

    /// Equirectangular distance approximation in meters. Same formula used
    /// elsewhere in this file; pulled into one place for the arrival check.
    static func straightLineMeters(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Double {
        let metersPerDegLat = 111_320.0
        let dN = (b.latitude - a.latitude) * metersPerDegLat
        let meanLat = ((a.latitude + b.latitude) / 2.0) * .pi / 180.0
        let dE = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegLat
        return (dN * dN + dE * dE).squareRoot()
    }

    /// Split a polyline at a given distance-along-route. Mirrors the web
    /// `splitPolylineAtDistance` helper: returns the prefix up to that
    /// distance and the suffix from there to the end. Inserts a synthesized
    /// vertex on the segment that contains the split point so each side
    /// stays a valid polyline.
    func splitPolylineAtDistance(
        _ polyline: [CoordinatePoint],
        distance: Double
    ) -> (completed: [CoordinatePoint], remaining: [CoordinatePoint]) {
        guard polyline.count >= 2, distance > 0 else { return ([], polyline) }
        let metersPerDegLat = 111_320.0
        var traversed = 0.0
        var completed: [CoordinatePoint] = [polyline[0]]
        for i in 0..<(polyline.count - 1) {
            let a = polyline[i]
            let b = polyline[i + 1]
            let meanLat = ((a.latitude + b.latitude) / 2.0) * .pi / 180.0
            let dN = (b.latitude - a.latitude) * metersPerDegLat
            let dE = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegLat
            let segLen = (dN * dN + dE * dE).squareRoot()
            if traversed + segLen >= distance {
                let t = segLen <= 0 ? 0 : (distance - traversed) / segLen
                let split = CoordinatePoint(
                    latitude: a.latitude + (b.latitude - a.latitude) * t,
                    longitude: a.longitude + (b.longitude - a.longitude) * t
                )
                completed.append(split)
                var remaining: [CoordinatePoint] = [split]
                remaining.append(contentsOf: polyline[(i + 1)...])
                return (completed, remaining)
            }
            completed.append(b)
            traversed += segLen
        }
        return (polyline, [polyline.last!])
    }

    /// Smoothed travel heading from the last few GPS fixes, `nil` while
    /// stationary. Spec line 110: this overrides the route-segment bearing
    /// when the rider is moving.
    var travelHeadingDegrees: Double? {
        headingTrail.travelHeadingDegrees
    }

    /// The bearing the in-routing camera should rotate to, merging spec
    /// lines 110 (GPS trail — wins when moving) and 101 (route segment —
    /// fallback when stationary). Returns `nil` only when there is neither
    /// an active route nor a usable trail.
    func cameraHeadingDegrees(rider: CoordinatePoint?) -> Double? {
        if let trail = headingTrail.travelHeadingDegrees {
            return trail
        }
        return routingBearingDegrees(rider: rider)
    }

    /// MapKit's `MapCamera(centerCoordinate:)` renders that point at the
    /// SCREEN CENTER. Spec line 84 wants the rider in the bottom quarter
    /// of the *visible* map area — but iOS has no `mapPadding` anchor
    /// equivalent. We shift the camera center AHEAD of the rider in the
    /// camera's heading direction so the rider renders below center.
    ///
    /// The previous 264 m offset matched web's 0.72 anchor against the
    /// FULL viewport, which placed the rider behind the routing card
    /// (Stop button + turn instruction occupies the bottom ~180 px on a
    /// 896 px screen). 130 m drops the rider to roughly 0.62 of the full
    /// viewport — comfortably above the card on iPhone 14 Pro Max-class
    /// devices and still distinctly in the lower half.
    func cameraCenterCoordinate(rider: CoordinatePoint, headingDegrees: Double) -> CoordinatePoint {
        let anchorOffsetMeters = 130.0
        let metersPerDegLat = 111_320.0
        let headingRad = headingDegrees * .pi / 180.0
        let dNorth = cos(headingRad) * anchorOffsetMeters
        let dEast = sin(headingRad) * anchorOffsetMeters
        let cosLat = cos(rider.latitude * .pi / 180.0)
        return CoordinatePoint(
            latitude: rider.latitude + dNorth / metersPerDegLat,
            longitude: rider.longitude + dEast / (metersPerDegLat * cosLat)
        )
    }

    /// Called by the map view on every user pan/zoom/rotate during routing.
    /// Schedules a recenter to the routing default after the pinned
    /// inactivity timeout (spec line 104). Outside phoneGuidance it's a
    /// no-op. Successive interactions reset the timer.
    func noteUserMapInteraction() {
        guard homeMode == .phoneGuidance else { return }
        mapInteractionRecenterTask?.cancel()
        let delay = mapInteractionRecenterDelay
        mapInteractionRecenterTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard let self, !Task.isCancelled else { return }
            guard self.homeMode == .phoneGuidance else { return }
            self.mapRecenterRequestID &+= 1
        }
    }
}
