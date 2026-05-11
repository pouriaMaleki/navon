import CoreLocation
import Foundation
import MapKit

@MainActor
final class HomeViewModel: ObservableObject {
    func syncQueryFromCurrentPreview() {
        let sessionTitle = appModel.activeSession.destinationLabel.trimmingCharacters(in: .whitespacesAndNewlines)
        if !Self.isGenericDestinationTitle(sessionTitle) {
            query = sessionTitle
        } else if let destination = appModel.preview.selectedAlternative?.normalizedPackage.summary.destinationLabel,
                  !Self.isGenericDestinationTitle(destination) {
            query = destination
        } else if let route = appModel.preview.selectedAlternative {
            query = route.title
        }
    }

    /// Generic placeholder labels that providers fall back to when a route's
    /// destination is anonymous (e.g. dropped pin, OSM/HSL fallback). The
    /// search field must not surface these on launch — they prefill the
    /// "Where to?" input with text that didn't come from the user.
    static func isGenericDestinationTitle(_ title: String) -> Bool {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return true }
        switch trimmed.lowercased() {
        case "no destination",
             "selected destination",
             "recent destination",
             "dropped pin",
             "route":
            return true
        default:
            return false
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

        // Intentionally NOT calling syncQueryFromCurrentPreview() on cold
        // launch: prefilling the "Where to?" field with the last persisted
        // destination is confusing for riders who killed the app and just
        // want to plan a new route. The function itself stays as a public
        // utility for places where prefilling IS desired (none today, but
        // the unit tests in HomeViewModelQueryInitTests still pin its
        // behaviour for future reuse).
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
    /// User-reported feedback: the iOS camera felt laggy through tight
    /// turns. The previous params (α=0.25, maxAgeMs=5000, maxFixes=10)
    /// matched runtime-core / companion-web for parity, but at cycling
    /// speeds the smoother takes ~5 s to land within 10° of a new
    /// heading — by which time the rider is past the turn. Tightening
    /// to α=0.45 + 3 s window halves the lag while keeping enough
    /// smoothing to absorb GPS jitter (raised the displacement floor
    /// to 4 m to match). Web/Android keep the old values for now —
    /// the regression report was iOS-specific and the platforms can
    /// re-converge when the user signals the same complaint elsewhere.
    private let headingTrail = HeadingTrail(
        maxAgeMs: 3_000, maxFixes: 6,
        minDisplacementM: 4.0, smoothingAlpha: 0.45
    )

    /// Test seam — exposes the heading-trail buffer so unit tests can
    /// drive fixes through it directly and assert convergence against
    /// the same production parameters the live app uses.
    var headingTrailForTesting: HeadingTrail { headingTrail }
    /// Pinned auto-recenter delay for user map interactions during routing.
    /// Mirrors `recenter_inactivity_ms` in parity-fixtures/data/ux-constants.toml
    /// (spec line 104).
    private let mapInteractionRecenterDelay: TimeInterval = 3.0

    /// True while the user is actively panning/zooming/rotating the map during
    /// routing. When set, the view layer skips GPS-driven camera-follow updates
    /// so the user's manual camera position isn't overridden by every GPS fix.
    @Published private(set) var isUserInteractingWithMap: Bool = false

    /// Default camera distance (m) for the riding-mode follow-rider camera
    /// when the rider hasn't overridden it via the on-map +/- buttons.
    /// 1200 m is the same scale companion-web uses (zoom 16 ≈ 1200 m
    /// distance at typical mid-latitudes).
    static let defaultRidingCameraDistanceM: Double = 1200
    /// Lower bound for `ridingCameraDistanceM` so successive zoom-in taps
    /// can't drive the camera into a sub-block scale where the rider can
    /// no longer see the next turn.
    static let minRidingCameraDistanceM: Double = 250
    /// Upper bound so a stray zoom-out chain can't end up viewing the
    /// entire region from orbit.
    static let maxRidingCameraDistanceM: Double = 8000
    /// Multiplicative step the +/- buttons apply per tap. 1.5× ≈ 0.6
    /// MapLibre zoom levels — same as companion-web's per-tap step.
    static let ridingZoomStepFactor: Double = 1.5

    /// Camera distance (m) used by the autoFollow follow-rider camera in
    /// routing mode. Reads the persisted override (set by the on-map +/-
    /// buttons). Falls back to `defaultRidingCameraDistanceM` when no
    /// override has ever been written. The view layer must use this
    /// value — NOT a hardcoded constant — so the rider's preferred zoom
    /// survives compass cycles + every GPS-fix-driven camera refresh.
    var ridingCameraDistanceM: Double {
        appModel.settings.ridingCameraDistanceM ?? Self.defaultRidingCameraDistanceM
    }

    enum RidingZoomDirection { case zoomIn, zoomOut }

    /// Apply a single +/- tap in routing mode. Multiplies the saved
    /// distance by 1/1.5 (zoom-in) or 1.5 (zoom-out), clamps to
    /// [`minRidingCameraDistanceM`, `maxRidingCameraDistanceM`], and
    /// persists. Mirrors the web companion's `settings.ridingZoom` path
    /// where overview/planning zooms are intentionally session-only.
    func bumpRidingZoom(direction: RidingZoomDirection) {
        let factor = direction == .zoomIn ? (1.0 / Self.ridingZoomStepFactor) : Self.ridingZoomStepFactor
        let raw = ridingCameraDistanceM * factor
        let next = min(Self.maxRidingCameraDistanceM, max(Self.minRidingCameraDistanceM, raw))
        appModel.settings.ridingCameraDistanceM = next
        appModel.persistSettings()
    }

    private let appModel: AppModel
    private let placeSearchService: PlaceSearchService
    private var latestSearchTask: Task<Void, Never>?
    private var northPreviewTask: Task<Void, Never>?
    private let switchablePlanningProviders: Set<RouteProviderID> = [.hsl, .osm]

    init(appModel: AppModel, placeSearchService: PlaceSearchService = MapKitPlaceSearchService()) {
        self.appModel = appModel
        self.placeSearchService = placeSearchService
    }

    /// Derived chip state for the home-screen device pairing affordance.
    /// Returns `nil` when there is no paired record: the chip stays hidden
    /// from home, and pairing initiation lives only in
    /// `DeviceSettingsView` (single-bond model — the user explicitly opts
    /// in / out from Settings, not from the busy home screen).
    var deviceChipState: DeviceChipState? {
        let connection = appModel.bleService.sessionState.connectionState
        guard let record = appModel.pairedPeripheral else {
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

    /// Action invoked when the chip is tapped. The unpaired path is no
    /// longer reachable from the chip — pairing starts from Settings.
    /// Connecting state is dropped at the chip layer (`.disabled(true)`);
    /// kept here as `break` so a future enabling of the state doesn't
    /// silently misroute.
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

    /// Toggled by tapping a connected device chip; the home view binds a
    /// popover or sheet to it. Exposed `@Published` so SwiftUI re-renders.
    @Published var showConnectionPopover: Bool = false

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
        isPreviewLockedToImportedRoute
            ? T.string("home.importedRoute")
            : T.string("home.suggestedRoutes")
    }

    var isShowingActiveNavigation: Bool {
        homeMode == .phoneGuidance || homeMode == .deviceOverview || homeMode == .sendingToDevice
    }

    var startButtonTitle: String {
        switch homeMode {
        case .sendingToDevice:
            return T.string("home.startingOnDevice")
        case .planning:
            return appModel.isDeviceConnected
                ? T.string("home.startOnDevice")
                : T.string("home.start")
        case .phoneGuidance, .deviceOverview:
            return T.string("home.start")
        }
    }

    var activeNavigationTitle: String {
        let placeholders: Set<String> = ["selected destination", "current location"]
        if let destination = guidanceRoute?.summary.destinationLabel {
            let trimmed = destination.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty && !placeholders.contains(trimmed.lowercased()) {
                return trimmed
            }
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
    /// auto-stopped. Cleared on the next route start, on manual dismissal,
    /// or after `arrivalNoticeAutoDismissDelay` seconds.
    @Published var arrivalNotice: String?

    /// Auto-dismiss delay for the arrival banner. The banner used to persist
    /// forever; without a timeout, a rider who walked away from the phone
    /// would come back to a banner blocking the new route suggestions card.
    /// Sixty seconds matches the web/Android constant.
    private static let arrivalNoticeAutoDismissDelay: TimeInterval = 60
    /// Test seam — when non-nil, overrides the production 60s delay so unit
    /// tests can verify the auto-dismiss path without sleeping a minute.
    static var arrivalNoticeAutoDismissDelayForTesting: TimeInterval?
    private var arrivalNoticeAutoDismissTask: Task<Void, Never>?

    /// Spec mirroring web's GuidanceStore: distance from the projected
    /// route, off-route latched state with hysteresis, and a reroute-request
    /// flag set after sustained off-route. Surface a single string label so
    /// the view layer doesn't have to know the state machine.
    @Published private(set) var offRouteDistanceM: Double = 0
    @Published private(set) var offRoute: Bool = false
    @Published private(set) var rerouteRequested: Bool = false
    private var offRouteDurationMs: Double = 0
    /// Sentinel `-1` means "no fix yet seen" — distinguishes a fresh
    /// session from `timestampMs == 0`. Using `0` collides with
    /// timestamps that legitimately start at zero (e.g. unit tests).
    private var lastAdvanceTimestampMs: Int64 = -1

    /// Latch so a single off-route episode kicks at most one auto-reroute.
    /// Reset to `false` when the rider returns into the route corridor.
    private var autoReroutePending: Bool = false

    /// Sliding-window log of timestamps when an auto-reroute was attempted.
    /// Used to compute the backoff delay; entries older than the window age
    /// out automatically on every `recordReroutingAttempt` call.
    @Published private(set) var reroutingAttemptTimestampsMs: [Double] = []
    /// Wall-clock millisecond timestamp at which the currently-deferred auto
    /// reroute will fire, or nil if no reroute is being held back. Drives
    /// the "Waiting to reroute" UI and the manual-override button.
    @Published private(set) var reroutingDelayedUntilMs: Double?

    private static let reroutingBackoffWindowMs: Double = 30_000
    private static let reroutingThrottleAtAttempts: Int = 3
    private static let reroutingEscalateAtAttempts: Int = 5
    private static let reroutingBackoffDelayMs: Double = 5_000
    private static let reroutingBackoffLongDelayMs: Double = 10_000

    /// Records an auto-reroute attempt at `now` and returns the required delay
    /// before the reroute should actually fire. 0 ms means fire immediately,
    /// > 0 ms means defer (and the UI surfaces the wait via `isWaitingToReroute`).
    func recordReroutingAttempt(now: Double) -> Double {
        reroutingAttemptTimestampsMs.removeAll { now - $0 >= Self.reroutingBackoffWindowMs }
        reroutingAttemptTimestampsMs.append(now)
        let count = reroutingAttemptTimestampsMs.count
        var delayMs: Double = 0
        if count >= Self.reroutingEscalateAtAttempts {
            delayMs = Self.reroutingBackoffLongDelayMs
        } else if count >= Self.reroutingThrottleAtAttempts {
            delayMs = Self.reroutingBackoffDelayMs
        }
        reroutingDelayedUntilMs = delayMs > 0 ? now + delayMs : nil
        return delayMs
    }

    /// True while an auto-reroute is being held back by the throttle. The
    /// view layer passes the current wall-clock so this stays a pure read.
    func isWaitingToReroute(now: Double) -> Bool {
        guard let until = reroutingDelayedUntilMs else { return false }
        return now < until
    }

    /// Rider tapped "Reroute now" — clear the throttle delay so the next
    /// observer pass fires the reroute immediately.
    func requestManualReroute() {
        reroutingDelayedUntilMs = nil
    }

    /// Called when an auto-reroute is actually dispatched. Re-arms the
    /// reroute request latch while preserving off-route state so sustained
    /// off-route riding can schedule a follow-up reroute attempt without
    /// requiring a temporary return to the on-route corridor.
    private func markAutoRerouteDispatched() {
        rerouteRequested = false
        offRouteDurationMs = 0
        reroutingDelayedUntilMs = nil
    }

    /// The most recently dispatched auto-reroute task. Tests await this
    /// to observe the asynchronous `AppModel.rerouteActiveSession`
    /// completing; production code does not need to read it.
    private(set) var pendingAutoRerouteTask: Task<Void, Never>?

    /// Single off-route label for the top guidance card. Returns "Rerouting…"
    /// once the off-route dwell threshold trips, "Off route" while in the
    /// off-route hysteresis band, and `nil` otherwise.
    var offRouteLabel: String? {
        if rerouteRequested { return "Rerouting…" }
        if offRoute { return "Off route" }
        return nil
    }

    /// Legacy side-rail enum, kept so the original layout test
    /// (`RoutingTopOverlayLayoutTests`) still compiles after the icons
    /// moved out of the routing top card and into the persistent
    /// top-right / bottom-right rails. The `routingTopLayout.sideRail`
    /// array is now always empty in production.
    enum RoutingSideRailItem: Equatable {
        case deviceChip
        case alternateRoutes
        case compass
    }

    /// Layout description of the routing-mode top overlay. Three text
    /// lines, no icons (icons live on the persistent rails so the layout
    /// doesn't reflow between modes).
    ///
    ///   - `headline`               — next-turn instruction
    ///   - `distanceToDestinationLine` — "8.6 km to Alppila" (distance
    ///     first, then short destination name, dropping " to <name>" when
    ///     the destination has no usable label)
    ///   - `minutesRemainingLine`   — "16 min remaining"
    ///
    /// `subtitle` is retained for backwards-compat callers and renders
    /// the same content as `distanceToDestinationLine • minutesRemainingLine`.
    /// `nil` outside of `phoneGuidance` mode.
    struct RoutingTopLayout: Equatable {
        let headline: String
        let subtitle: String
        let distanceToDestinationLine: String
        let minutesRemainingLine: String
        let offRouteLabel: String?
        let sideRail: [RoutingSideRailItem]
    }

    var routingTopLayout: RoutingTopLayout? {
        guard homeMode == .phoneGuidance else { return nil }
        let distanceLine = formattedDistanceToDestinationLine()
        let minutesLine = formattedMinutesRemainingLine()
        let combined = [distanceLine, minutesLine].filter { !$0.isEmpty }.joined(separator: " • ")
        return RoutingTopLayout(
            headline: nextInstructionLine ?? activeNavigationTitle,
            subtitle: combined,
            distanceToDestinationLine: distanceLine,
            minutesRemainingLine: minutesLine,
            offRouteLabel: offRouteLabel,
            sideRail: []
        )
    }

    private func formattedDistanceToDestinationLine() -> String {
        let remaining = remainingDistanceM > 0
            ? remainingDistanceM
            : (guidanceRoute?.summary.totalDistanceMeters ?? 0)
        guard remaining > 0 else { return "" }
        let km = String(format: "%.1f", remaining / 1000)
        // Prefer the user-typed destination on activeSession (set by the
        // where-to flow). The route package's `summary.destinationLabel`
        // is often a generic placeholder ("Selected destination" from the
        // OSRM mapper, or empty/"Current location" from HSL legs) that
        // would erase the real address the rider picked.
        let candidates = [
            appModel.activeSession.destinationLabel,
            guidanceRoute?.summary.destinationLabel ?? "",
        ]
        let placeholderTitles: Set<String> = [
            "", "No destination", "Selected destination", "Current location",
        ]
        let address = candidates
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { !placeholderTitles.contains($0) })
        if let address, !address.isEmpty {
            return "\(km) km to \(address)"
        }
        return "\(km) km"
    }

    private func formattedMinutesRemainingLine() -> String {
        let seconds: Double = remainingDurationSeconds > 0
            ? remainingDurationSeconds
            : Double(guidanceRoute?.summary.estimatedDurationSeconds ?? 0)
        guard seconds > 0 else { return "" }
        let minutes = max(1, Int(ceil(seconds / 60.0)))
        return "\(minutes) min remaining"
    }

    /// Stable top-right icon column, sitting BELOW the Settings cog
    /// (which itself lives in the where-to top bar). Same items in every
    /// mode so the layout doesn't reflow when the rider transitions
    /// between planning and routing. Order, top → bottom: compass/
    /// north-up, device chip (only when paired). The compass tap
    /// recentres the camera in every mode (single tap = north-up;
    /// double-tap = lock north-up, matching the Rust implementation).
    ///
    /// `.settings` is left in the enum so callers can still pattern-match
    /// it when reading the cog from the top-bar, but it never appears in
    /// `topRightIconStack` itself.
    enum TopRightIcon: Equatable {
        case settings
        case compass
        case deviceChip
    }

    var topRightIconStack: [TopRightIcon] {
        // Always-on items first (their on-screen position never changes
        // between modes), conditional items at the bottom of the list.
        var icons: [TopRightIcon] = [.settings, .compass]
        if appModel.pairedPeripheral != nil {
            icons.append(.deviceChip)
        }
        return icons
    }

    /// Top-LEFT icon column, sitting BELOW the where-to search field on
    /// the left side of the screen. Mirrors the right rail's layout but
    /// stays clear of the suggested-routes card / device-overview footer
    /// at the bottom of the screen. Order, top → bottom: alternate-routes
    /// (only in phoneGuidance), zoom-in, zoom-out.
    enum TopLeftIcon: Equatable {
        case alternateRoutes
        case zoomIn
        case zoomOut
    }

    var topLeftIconStack: [TopLeftIcon] {
        // Zoom is always first/second so pressing Start (which adds the
        // alternate-routes button) does NOT push the zoom column down a
        // slot. Alternate-routes is appended at the bottom only in
        // routing mode.
        var icons: [TopLeftIcon] = [.zoomIn, .zoomOut]
        if homeMode == .phoneGuidance {
            icons.append(.alternateRoutes)
        }
        return icons
    }

    private static let offRouteEnterDistanceM: Double = 35
    private static let offRouteExitDistanceM: Double = 22
    private static let rerouteRequestDelayMs: Double = 2000

    var nextInstructionLine: String? {
        // Spec line 102: as the rider advances along the route, the
        // displayed line must switch to the NEXT upcoming maneuver. We
        // skip depart/arrive turn-types and pick the first maneuver
        // whose distance-from-start is still ahead of `progressDistanceM`.
        //
        // Format is "<distance> <instruction>" so the eye lands on the
        // metric first (matches the new routing top card's other two
        // lines: "8.6 km to Alppila", "16 min remaining").
        guard let route = guidanceRoute else { return nil }
        for m in route.maneuvers {
            if m.maneuverType == .depart || m.maneuverType == .arrive { continue }
            let remaining = m.distanceFromStartMeters - progressDistanceM
            if remaining < 0 { continue }
            let instruction = m.instructionText ?? "Continue"
            return "\(formatDistance(remaining)) \(instruction)"
        }
        // Past the last interesting maneuver: show "Arrive" remaining.
        if let arrive = route.maneuvers.last(where: { $0.maneuverType == .arrive }) {
            let remaining = max(0, arrive.distanceFromStartMeters - progressDistanceM)
            return "\(formatDistance(remaining)) Arrive"
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
        if isExploringAlternativesFromGuidance {
            explorationSelectedID = alternativeID
            appModel.selectAlternativePreviewOnly(alternativeID)
        } else {
            appModel.selectAlternative(alternativeID)
        }
    }

    func startSelectedRoute() async {
        closeSearch()
        guard let selectedPreview else { return }
        activeRouteIdentifier = selectedPreview.normalizedPackage.routeIdentifier
        // Starting a new route clears any stale arrival banner from the
        // previous trip.
        cancelArrivalNoticeAutoDismiss()
        arrivalNotice = nil
        // Reset off-route state so an old latch from a previous trip doesn't
        // bleed into the new one before the first fix arrives.
        offRoute = false
        offRouteDistanceM = 0
        offRouteDurationMs = 0
        rerouteRequested = false
        lastAdvanceTimestampMs = -1
        lastProgressRouteId = nil
        autoReroutePending = false
        pendingAutoRerouteTask?.cancel()
        pendingAutoRerouteTask = nil
        if appModel.isDeviceConnected {
            homeMode = .sendingToDevice
            let success = await appModel.sendSelectedRoute()
            homeMode = success ? .deviceOverview : .planning
        } else {
            // Populate the active session even on the phone-guidance path
            // so downstream consumers (split-way reroute, diagnostics,
            // off-route reroute) can read destination + identifier
            // without falling back to nil. The device branch gets this
            // for free through `appModel.sendSelectedRoute`; phone
            // guidance has to do it explicitly.
            let pkg = selectedPreview.normalizedPackage
            appModel.activeSession.routeIdentifier = pkg.routeIdentifier
            appModel.activeSession.routeRevision = pkg.revision
            appModel.activeSession.destinationCoordinate = pkg.geometry.last
                ?? appModel.activeSession.destinationCoordinate
            appModel.activeSession.providerID = pkg.provenance.providerID
            appModel.activeSession.sourceMode = sourceMode
            // Don't clobber a real user-typed destination on activeSession
            // with the route package's generic placeholder ("Selected
            // destination" comes baked into the OSRM mapper). Only
            // overwrite when the package carries a more specific label
            // (e.g. an HSL stop name) AND the session is currently
            // empty / placeholder.
            if let label = pkg.summary.destinationLabel {
                let placeholderTitles: Set<String> = [
                    "", "No destination", "Selected destination", "Current location",
                ]
                let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
                let sessionIsPlaceholder = placeholderTitles.contains(
                    appModel.activeSession.destinationLabel.trimmingCharacters(in: .whitespacesAndNewlines)
                )
                let labelIsMeaningful = !placeholderTitles.contains(trimmed)
                if labelIsMeaningful && (sessionIsPlaceholder || appModel.activeSession.destinationLabel.isEmpty) {
                    appModel.activeSession.destinationLabel = trimmed
                }
            }
            homeMode = .phoneGuidance
            compassMode = .autoFollow
            isExploringAlternativesFromGuidance = false
            explorationSelectedID = nil
            activeRoutePackage = selectedPreview.normalizedPackage
            // Reset progress for the new route so spec line 102 (next-turn
            // tracking) starts at 0 and advances as the rider proceeds.
            progressDistanceM = 0
            routeTotalDistanceM = polylineLengthMeters(selectedPreview.normalizedPackage.geometry)
            // Mark the app as routing-in-progress now (the explicit flag
            // gates the Live Activity / background-GPS rules).
            appModel.isRoutingInProgress = true
            // Kick the routing activity coordinator so the Live Activity
            // starts immediately, even before the first GPS fix lands.
            dispatchCueTick()
        }
        appModel.routingDiagnosticsStore.recordEvent(.routeStarted)
        if let sel = selectedPreview {
            appModel.routingDiagnosticsStore.recordEvent(.routeSelected(
                alternativeId: sel.id.uuidString,
                providerName: sel.normalizedPackage.provenance.providerID.rawValue,
                routeId: sel.normalizedPackage.routeIdentifier,
                label: sel.title
            ))
        }
    }

    func stopActiveNavigation(afterArrival: Bool = false) {
        appModel.routingDiagnosticsStore.recordEvent(.routeStopped(reason: afterArrival ? "arrival" : "manual"))
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
            // arrivalNotice is intentionally NOT cleared here. For manual
            // stop it was nil before (no arrival), so the omission is
            // harmless. For the arrival path (afterArrival=true),
            // declareArrival() just set it and it must persist until the
            // user starts a new route.
            // Cancel any in-flight auto-reroute before clearing the flags it
            // writes — without this the task can land after stop and mutate
            // activeSession / rerouteRequested on a route that no longer exists.
            pendingAutoRerouteTask?.cancel()
            pendingAutoRerouteTask = nil
            autoReroutePending = false
            // Reset all off-route / reroute hysteresis so the next ride starts
            // with a clean slate and stale @Published flags don't linger in the UI.
            offRoute = false
            offRouteDistanceM = 0
            offRouteDurationMs = 0
            rerouteRequested = false
            // Reset progress so stale distance doesn't carry into the next route
            // or confuse the CueEngine if the coordinator is ticked before the
            // next startSelectedRoute clears them.
            progressDistanceM = 0
            routeTotalDistanceM = 0
            lastProgressRouteId = nil
            lastAdvanceTimestampMs = -1
            // Clear the persisted active session so a later cold launch
            // does not see a stale `routeIdentifier` and mistake the user
            // for being mid-ride (which previously caused the Live
            // Activity to spin up on app launch with nothing on screen).
            appModel.activeSession.routeIdentifier = nil
            appModel.activeSession.routeRevision = nil
            appModel.persistence.saveSession(appModel.activeSession)
            // Drop the in-progress flag BEFORE dispatching. The didSet calls
            // syncRoutingActivityServices(), which resets the CueEngine state
            // and idles the idle timer before the final tick fires.
            appModel.isRoutingInProgress = false
            // Tell the routing activity coordinator that routing is over,
            // so it ends the Live Activity (and disables idle-timer hold).
            dispatchCueTick()
            if afterArrival {
                // Arrival: wipe the search field and all route alternatives
                // so the map returns to a blank "Where to?" state. The
                // arrival notice (set by declareArrival before this call)
                // persists until the rider starts a new route.
                query = ""
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
                origin: appModel.riderLocation,
                destination: destination,
                providerID: sourceToReuse.primaryProviderID
            )
            await appModel.planRoute(using: sourceToReuse, preferredTitle: query.isEmpty ? activeNavigationTitle : query)
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceToReuse.displayName)
        }
    }

    /// Spec #11 ("split-way reroute"): from inside an active route the rider
    /// can ask for fresh alternatives from their current location to the
    /// same destination, without committing to one until they Start it.
    /// True while the rider is browsing alternative routes from active guidance
    /// (the "split icon" flow). `homeMode` stays `.phoneGuidance` so guidance
    /// keeps running; the alternatives card is shown via this flag.
    @Published var isExploringAlternativesFromGuidance: Bool = false
    /// The alternative explicitly tapped by the user during exploration.
    /// Nil on enter so no row shows a checkmark until the user taps one.
    @Published private(set) var explorationSelectedID: UUID?
    /// Frozen snapshot of the active route when exploration begins, so
    /// `guidanceRoute` doesn't flip to a browsed alternative and break
    /// GPS progress projection and the map's green-route polyline.
    private var activeRoutePackage: NormalizedRoutePackage?

    /// Spec #11 ("split-way reroute"): from inside an active route the rider
    /// can ask for fresh alternatives from their current location to the same
    /// destination, without committing until they tap Start on one.
    /// Mirrors web's `RootStore.exploreAlternateRoutes`.
    ///
    /// Behavior:
    /// - Re-plans from `appModel.riderLocation` to the active destination.
    /// - homeMode stays `.phoneGuidance` — guidance keeps running.
    /// - Sets `isExploringAlternativesFromGuidance = true` so the UI shows
    ///   the alternatives card with a "Continue on current route" row at top.
    /// - Picking a new route and tapping Start swaps the active route via the
    ///   normal `startSelectedRoute` path; cancelling via
    ///   `cancelAlternativesExploration()` clears the flag without touching
    ///   the active session.
    func exploreAlternateRoutes() {
        guard homeMode == .phoneGuidance,
              let destination = appModel.activeSession.destinationCoordinate
        else { return }
        let sourceToReuse = appModel.activeSession.sourceMode
        let titleHint = appModel.activeSession.destinationLabel
        northPreviewTask?.cancel()
        compassMode = .northLocked
        // Set the flag immediately so the alternatives panel opens right away
        // with a loading indicator while the plan fetches in the background.
        isExploringAlternativesFromGuidance = true
        explorationSelectedID = nil
        planningStatus = "Looking for alternatives…"
        appModel.routingDiagnosticsStore.recordEvent(.exploreAlternatives)
        appModel.routeRequest = RoutePlanRequest(
            origin: appModel.riderLocation,
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
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: sourceToReuse.displayName)
        }
    }

    /// Cancel browsing alternatives — dismiss the card, resume normal routing UI.
    /// The original route is still active; camera returns to autoFollow.
    func cancelAlternativesExploration() {
        isExploringAlternativesFromGuidance = false
        explorationSelectedID = nil
        compassMode = .autoFollow
    }

    /// Clear the explicitly-tapped alternative selection so the checkmark
    /// moves back to "Continue on current route". Does NOT exit exploration.
    func deselectForExploration() {
        guard isExploringAlternativesFromGuidance else { return }
        explorationSelectedID = nil
    }

    /// The alternative ID that should show a checkmark in the suggestions card.
    /// During exploration, returns the ID explicitly tapped by the user (nil
    /// until first tap). Outside exploration, returns the planning-selected ID.
    var selectedAlternativeIDForDisplay: UUID? {
        isExploringAlternativesFromGuidance ? explorationSelectedID : appModel.preview.selectedAlternativeID
    }

    /// Alternative routes to render on the map during exploration.
    /// Returns an empty array outside of exploration.
    var guidanceAlternatives: [RouteAlternative] {
        isExploringAlternativesFromGuidance ? appModel.preview.alternatives : []
    }

    /// The MKCoordinateRegion to fit when showing a route overview. Pure
    /// math, exposed as a static so it can be unit-tested without
    /// instantiating a view.
    ///
    /// The bottom UI overlay (alternatives card during planning, arrival
    /// banner, etc.) covers roughly the lower 30% of the screen. MapKit
    /// has no `padding` API for `setRegion`, so we expand the latitude
    /// delta and shift the camera center SOUTH by ~15% of that delta.
    /// The bbox then sits in the upper ~65% of the visible region — the
    /// half the user can actually see — instead of being cropped behind
    /// the card.
    static func fittedRouteRegion(
        minLat: Double, maxLat: Double,
        minLon: Double, maxLon: Double
    ) -> MKCoordinateRegion {
        // The visible "uncovered" map strip during planning is bounded by
        // the top "Where to?" search bar (~12% of screen) and the bottom
        // alternatives card (which can run ~55% with 3 alternatives + source
        // picker + Start button on smaller iPhones). The fit must keep the
        // bbox INSIDE that strip — both extremes matter, and over-correcting
        // either way reintroduces the original crop:
        //
        //   - 2.2× / 0.22 cropped the route start behind the bottom card.
        //   - 2.6× / 0.30 (later 3.0× / 0.20) over-shifted upward — the
        //     route end ended up under the where-to bar at the top.
        //
        // 4.0× span + 0.22 southward shift puts the bbox at screen-y
        // 15.5%-40.5% (centred at ~28%), which sits comfortably inside the
        // 12%-45% visible strip even when the card runs slightly over 50%.
        // Both the top and bottom edges keep ~3-5% of buffer.
        let latDelta = max((maxLat - minLat) * 4.0, 0.022)
        let lonDelta = max((maxLon - minLon) * 2.0, 0.014)
        let bboxCenterLat = (minLat + maxLat) / 2.0
        let shiftedCenterLat = bboxCenterLat - 0.22 * latDelta
        return MKCoordinateRegion(
            center: CLLocationCoordinate2D(
                latitude: shiftedCenterLat,
                longitude: (minLon + maxLon) / 2.0
            ),
            span: MKCoordinateSpan(latitudeDelta: latDelta, longitudeDelta: lonDelta)
        )
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
        // Spec line 39: tapping the compass glyph always recenters the
        // camera, regardless of mode. Previously this bumped the request
        // id only after the routing-only guard, so in planning mode the
        // rider could pan the map and have no way to come back. Bump the
        // recenter request id unconditionally; only the compass-mode
        // toggle (north-preview/north-locked) is gated on phoneGuidance.
        mapRecenterRequestID &+= 1
        guard homeMode == .phoneGuidance else { return }
        let prevCompassMode = String(describing: compassMode)
        switch compassMode {
        case .autoFollow:
            enterNorthLocked()
        case .northPreview:
            enterNorthLocked()
        case .northLocked:
            northPreviewTask?.cancel()
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

    private func enterNorthLocked() {
        let prev = String(describing: compassMode)
        northPreviewTask?.cancel()
        compassMode = .northLocked
        appModel.routingDiagnosticsStore.recordEvent(.compassModeChanged(from: prev, to: "northLocked"))
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
            advanceProgress(rider: point, nowMs: timestampMs)
        }
    }

    /// Monotonic distance (m) the rider has progressed along the active
    /// route, projecting the latest fix onto the polyline. Drives spec
    /// line 102 (next-turn switching as the rider passes maneuvers).
    @Published private(set) var progressDistanceM: Double = 0

    /// Total length of the active route in meters. Cached on Start.
    private var routeTotalDistanceM: Double = 0

    /// Composite key that combines routeIdentifier + revision so that a
    /// reroute returning the SAME identifier with a bumped revision is still
    /// treated as a new route (progress + CueEngine state reset). Without the
    /// revision component, replanning the same origin→destination produced an
    /// identical identifier, preserving stale progressDistanceM and causing the
    /// CueEngine to see all new-route maneuvers as "already passed", which then
    /// fired a ghost arrivingInM cue as the first sound after rerouting.
    private var lastProgressRouteId: String? = nil

    private func routeKey() -> String? {
        guard let id = appModel.activeSession.routeIdentifier else { return nil }
        let rev = appModel.activeSession.routeRevision ?? 0
        return "\(id)-rev\(rev)"
    }

    /// Project the rider onto the active route geometry and advance
    /// `progressDistanceM` monotonically (never regresses), mirroring
    /// runtime-core's behaviour. Also runs the off-route hysteresis: when
    /// perpendicular distance to the route crosses the enter/exit
    /// thresholds, latch / unlatch the off-route flag and accumulate dwell
    /// time toward the reroute-request signal.
    private func advanceProgress(rider: CoordinatePoint, nowMs: Int64) {
        guard let geometry = guidanceRoute?.geometry, geometry.count >= 2 else { return }
        let currentRouteKey = routeKey()
        if currentRouteKey != lastProgressRouteId {
            lastProgressRouteId = currentRouteKey
            progressDistanceM = 0
            routeTotalDistanceM = guidanceRoute.map { polylineLengthMeters($0.geometry) } ?? 0
            lastAdvanceTimestampMs = -1
        }
        let projection = projectProgressWithDistance(onto: geometry, rider: rider)
        let bounded = min(routeTotalDistanceM > 0 ? routeTotalDistanceM : projection.progress, projection.progress)
        progressDistanceM = max(progressDistanceM, bounded)
        offRouteDistanceM = projection.distanceToRouteM

        let dt = lastAdvanceTimestampMs >= 0 ? Double(nowMs - lastAdvanceTimestampMs) : 0
        lastAdvanceTimestampMs = nowMs

        // Off-route latch with hysteresis (35 m enter, 22 m exit) — same
        // bands as runtime-core / companion-web.
        let wasOffRoute = offRoute
        let prevRerouteRequested = rerouteRequested
        if offRoute {
            if projection.distanceToRouteM <= Self.offRouteExitDistanceM {
                offRoute = false
            }
        } else if projection.distanceToRouteM >= Self.offRouteEnterDistanceM {
            offRoute = true
            appModel.routingDiagnosticsStore.recordEvent(.offRouteDetected(distanceM: projection.distanceToRouteM))
        }
        if offRoute {
            offRouteDurationMs = wasOffRoute ? offRouteDurationMs + dt : dt
            if offRouteDurationMs >= Self.rerouteRequestDelayMs {
                rerouteRequested = true
            }
        } else {
            offRouteDurationMs = 0
            rerouteRequested = false
            // Returning to the corridor re-arms auto-reroute for the
            // next off-route episode. Without this, only the first
            // departure of a session would ever re-fetch a route.
            autoReroutePending = false
        }

        // Rising edge of `rerouteRequested` (false → true) is the only
        // moment we kick the routing provider. Subsequent ticks while
        // the rider stays off-route hold the latch so we don't spam
        // requests. The backoff throttle holds repeat attempts longer
        // when the rider keeps drifting in a short window.
        if rerouteRequested && !prevRerouteRequested && !autoReroutePending {
            autoReroutePending = true
            appModel.routingDiagnosticsStore.recordEvent(.rerouteRequested)
            let now = Date().timeIntervalSince1970 * 1_000
            let delayMs = recordReroutingAttempt(now: now)
            let model = appModel
            pendingAutoRerouteTask = Task { @MainActor in
                if delayMs > 0 {
                    // Poll for manual override or delay expiry; manual override
                    // clears `reroutingDelayedUntilMs` so this loop short-circuits.
                    while let until = self.reroutingDelayedUntilMs,
                          Date().timeIntervalSince1970 * 1_000 < until {
                        try? await Task.sleep(nanoseconds: 250_000_000)
                        if Task.isCancelled { return }
                    }
                }
                self.markAutoRerouteDispatched()
                defer { self.autoReroutePending = false }
                await model.rerouteActiveSession(
                    from: rider,
                    reason: "Off-route",
                    rerouteContext: RerouteContext(
                        headingDegrees: self.travelHeadingDegrees,
                        speedMps: model.locationService.currentSpeedMps
                    )
                )
            }
        }

        // Spec: when the rider arrives at the destination, end routing.
        // Use straight-line distance to the last geometry vertex so a rider
        // approaching from any side trips arrival, not just one travelling
        // along-route.
        if let last = geometry.last,
           Self.straightLineMeters(last, rider) <= Self.arrivalRadiusM {
            declareArrival()
        }

        // Spec lines 133-143: dispatch a cue snapshot every tick so the
        // cue engine can emit "Route started", 50/10 m approach, off-route,
        // arrival, etc. The coordinator handles all gating (settings,
        // paired-with-device).
        dispatchCueTick()
        // Record next turn UI updates for routing diagnostics
        if let line = nextInstructionLine {
            appModel.routingDiagnosticsStore.recordEvent(.nextTurnAlerted(
                instructionText: line,
                distanceRemainingM: guidanceRoute?.maneuvers.first { m in
                    m.maneuverType != "depart" && m.maneuverType != "arrive" &&
                    m.distanceFromStartMeters > progressDistanceM
                }.map { $0.distanceFromStartMeters - progressDistanceM } ?? 0
            ))
        }
    }

    /// Builds a `CueSnapshot` from the current routing state and dispatches
    /// it to the routing-activity coordinator. Called every GPS fix during
    /// routing, on route start, and on stop — the coordinator handles all
    /// gating (settings, paired-with-device) and the Live Activity
    /// lifecycle (start when routing begins, update each tick, end on
    /// stop) from this single entry point.
    func dispatchCueTick() {
        guard !isExploringAlternativesFromGuidance else { return }
        // Spec lines 7 / 131: cues fire from the phone only when the
        // ESP32 device isn't actively driving the on-screen guidance.
        // Pairing alone is not enough — the device might be paired but
        // currently disconnected (rider out of range / device off), in
        // which case the phone is the only UI and SHOULD speak.
        let pairedWithDevice = appModel.isDeviceConnectedForCueSuppression
        // Filtering and kind-mapping live in `CueManeuverMapping` — the
        // single point of truth for which maneuver types reach the cue
        // stream. Returning nil silences a maneuver entirely (slight*
        // splits, .straight, .depart, .arrive).
        let cueManeuvers: [CueManeuver] = (guidanceRoute?.maneuvers ?? []).compactMap { m in
            CueManeuverMapping.kind(for: m.maneuverType).map { kind in
                CueManeuver(
                    id: m.id,
                    kind: kind,
                    distanceFromStartM: m.distanceFromStartMeters,
                    isMinorKeep: m.maneuverType == .slightLeft || m.maneuverType == .slightRight
                )
            }
        }
        let snapshot = CueSnapshot(
            routeId: routeKey(),
            pairedWithDevice: pairedWithDevice,
            progressDistanceM: progressDistanceM,
            maneuvers: cueManeuvers,
            offRoute: offRoute,
            rerouting: rerouteRequested,
            arrived: arrivalNotice != nil,
            distanceFromRouteM: offRouteDistanceM,
            routeTotalDistanceM: routeTotalDistanceM
        )
        appModel.routingActivityCoordinator.onGuidanceTick(
            snapshot: snapshot,
            settings: appModel.settings,
            isRouting: appModel.isRoutingInProgress,
            isAppInBackground: appModel.isAppInBackground
        )
        // Push the same tick into the Live Activity coordinator. The
        // coordinator gates on `liveActivityEnabled && allowBackgroundGps
        // && isRouting && route != nil` and silently no-ops when off.
        appModel.activeGuidanceRoute = guidanceRoute
        let isImperial = T.resolveDistanceUnit(appModel.settings.distanceUnit) == .imperial
        appModel.liveActivityCoordinator.onGuidanceTick(
            settings: appModel.settings,
            isRouting: appModel.isRoutingInProgress,
            route: guidanceRoute,
            progressDistanceM: progressDistanceM,
            offRoute: offRoute,
            rerouting: rerouteRequested,
            arrived: arrivalNotice != nil,
            isImperial: isImperial
        )
    }

    /// Variant of `projectProgress` that also returns the perpendicular
    /// distance to the closest segment — needed for off-route hysteresis.
    private func projectProgressWithDistance(
        onto polyline: [CoordinatePoint],
        rider: CoordinatePoint
    ) -> (progress: Double, distanceToRouteM: Double) {
        guard polyline.count >= 2 else { return (0, .infinity) }
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
        return (bestProgress, sqrt(bestDistSq))
    }

    /// Larger than the off-route exit distance so a rider drifting around
    /// the destination doesn't bounce between "off route" and "arrived".
    private static let arrivalRadiusM: Double = 25

    private func declareArrival() {
        arrivalNotice = "Arrived at destination"
        scheduleArrivalNoticeAutoDismiss()
        stopActiveNavigation(afterArrival: true)
    }

    /// Manual dismissal from the banner's close button.
    func dismissArrivalNotice() {
        cancelArrivalNoticeAutoDismiss()
        arrivalNotice = nil
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
    /// The offset MUST scale with `cameraDistanceM`. MapKit's `distance`
    /// (camera altitude in m) drives how many meters of ground show
    /// vertically on screen, so a fixed-meters offset is a different
    /// fraction of the visible map at every zoom level. At D=1200 m we
    /// want 90 m of offset (rider sits at ~0.6 of the visible area —
    /// distinctly bottom-half, but with daylight above the floating Stop
    /// pill). The same screen-relative position at D=300 m needs only
    /// 22.5 m of offset; otherwise the rider drifts off the bottom edge
    /// of the screen at deep zoom-in. Linear scaling keeps the rider's
    /// screen-relative anchor invariant under zoom.
    static let anchorOffsetMetersPerDistance: Double = 0.075
    func cameraCenterCoordinate(
        rider: CoordinatePoint,
        headingDegrees: Double,
        cameraDistanceM: Double
    ) -> CoordinatePoint {
        let anchorOffsetMeters = Self.anchorOffsetMetersPerDistance * cameraDistanceM
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

    /// Convenience overload that uses the rider's preferred routing-mode
    /// camera distance. Keeps existing call sites compiling and still
    /// produces a zoom-aware offset because `ridingCameraDistanceM`
    /// reflects the persisted +/- override.
    func cameraCenterCoordinate(rider: CoordinatePoint, headingDegrees: Double) -> CoordinatePoint {
        cameraCenterCoordinate(rider: rider, headingDegrees: headingDegrees, cameraDistanceM: ridingCameraDistanceM)
    }

    /// Called by the map view on every user pan/zoom/rotate during routing.
    /// Schedules a recenter to the routing default after the pinned
    /// inactivity timeout (spec line 104). Outside phoneGuidance it's a
    /// no-op. Successive interactions reset the timer.
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
} 
