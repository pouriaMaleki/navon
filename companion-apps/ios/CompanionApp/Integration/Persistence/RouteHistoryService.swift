import Foundation
import Combine

/// Route history CRUD, planner preferences, and pending import presentation management.
@MainActor
final class RouteHistoryService: ObservableObject {
    private let persistence: CompanionPersistence
    private let deviceManager: DeviceManager
    private let sessionManager: SessionManager
    private let routeSyncService: RouteSyncService
    private let settingsProvider: () -> CompanionSettings
    private let previewProvider: () -> RoutePreviewModel
    private let onRefreshDiagnostics: () -> Void
    private let onSetCurrentSourceMode: (RouteSourceMode) -> Void
    private let onSetPreview: (RoutePreviewModel) -> Void

    init(
        persistence: CompanionPersistence,
        deviceManager: DeviceManager,
        sessionManager: SessionManager,
        routeSyncService: RouteSyncService,
        settingsProvider: @escaping () -> CompanionSettings,
        previewProvider: @escaping () -> RoutePreviewModel,
        onRefreshDiagnostics: @escaping () -> Void,
        onSetCurrentSourceMode: @escaping (RouteSourceMode) -> Void,
        onSetPreview: @escaping (RoutePreviewModel) -> Void
    ) {
        self.persistence = persistence
        self.deviceManager = deviceManager
        self.sessionManager = sessionManager
        self.routeSyncService = routeSyncService
        self.settingsProvider = settingsProvider
        self.previewProvider = previewProvider
        self.onRefreshDiagnostics = onRefreshDiagnostics
        self.onSetCurrentSourceMode = onSetCurrentSourceMode
        self.onSetPreview = onSetPreview
    }

    /// Cached snapshot to avoid re-decoding the persistence JSON on every read.
    /// Invalidated by `invalidateHistoryCache()` from each mutating path below.
    private var cachedRouteHistoryItems: [RouteHistoryItem]?

    var routeHistoryItems: [RouteHistoryItem] {
        if let cached = cachedRouteHistoryItems { return cached }
        let items = persistence.loadRecentRouteHistory()
        cachedRouteHistoryItems = items
        return items
    }

    /// Drops the in-memory snapshot so the next `routeHistoryItems` read
    /// re-decodes from persistence. Callers that write to the underlying
    /// `companion.routeHistory` key directly must invoke this so the UI
    /// sees the newly-saved row.
    func invalidateHistoryCache() {
        cachedRouteHistoryItems = nil
    }

    /// Persist a history item and notify observers in one step. Prefer this
    /// over calling `persistence.saveRouteHistoryItem` directly so that the
    /// cache is invalidated and `objectWillChange` fires on this service.
    func saveHistoryItem(_ item: RouteHistoryItem) {
        persistence.saveRouteHistoryItem(item)
        invalidateHistoryCache()
        objectWillChange.send()
    }

    var pendingHomeImportPresentation: PendingHomeImportPresentation? {
        persistence.loadPendingHomeImportPresentation()
    }

    var routePlannerPreferences: RoutePlannerPreferences {
        get { persistence.loadRoutePlannerPreferences() }
        set {
            var sanitized = newValue
            if !HslAvailabilityService.isHslLiveConfigured(settings: settingsProvider()) && sanitized.defaultSourceMode != .osm {
                sanitized.defaultSourceMode = .osm
            }
            persistence.saveRoutePlannerPreferences(sanitized)
            onSetCurrentSourceMode(sanitized.defaultSourceMode)
            objectWillChange.send()
        }
    }

    @discardableResult
    func recordPlannedPreview(source: RouteHistorySource, sourceLabel: String) -> RouteHistoryItem? {
        guard let selected = previewProvider().selectedAlternative?.normalizedPackage else { return nil }
        let item = RouteHistoryItem(
            id: selected.routeIdentifier,
            title: sessionManager.session.destinationLabel,
            subtitle: selected.summaryLine,
            source: source,
            sourceLabel: sourceLabel,
            createdAt: Date(),
            destination: selected.geometry.last,
            routePackage: selected,
            occurrenceCount: nil
        )
        persistence.saveRouteHistoryItem(item)
        invalidateHistoryCache()
        objectWillChange.send()
        return item
    }

    func recordRecentDestination(title: String, coordinate: CoordinatePoint) {
        persistence.saveRouteHistoryItem(
            RouteHistoryItem(
                id: "recent-\(coordinate.latitude)-\(coordinate.longitude)-\(title)",
                title: title,
                subtitle: "Recent destination",
                source: .recentDestination,
                sourceLabel: "Recent",
                createdAt: Date(),
                destination: coordinate,
                routePackage: nil,
                occurrenceCount: 1
            )
        )
        persistence.saveRecentDestination(coordinate)
        invalidateHistoryCache()
        objectWillChange.send()
    }

    func dismissRouteHistoryItem(id: String) {
        persistence.dismissRouteHistoryItem(id: id)
        invalidateHistoryCache()
        objectWillChange.send()
    }

    func savePendingHomeImportPresentation(item: RouteHistoryItem, debugTrail: [String]) {
        persistence.savePendingHomeImportPresentation(
            PendingHomeImportPresentation(
                routeHistoryItemID: item.id,
                title: item.title,
                sourceLabel: item.sourceLabel,
                destination: item.destination,
                createdAt: Date(),
                debugTrail: debugTrail
            )
        )
        objectWillChange.send()
    }

    func clearPendingHomeImportPresentation() {
        persistence.clearPendingHomeImportPresentation()
        objectWillChange.send()
    }

    func clearPreviewSelection() {
        onSetPreview(RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil))
    }

    func resetCurrentRouteForHistoryActivation() async {
        onSetPreview(RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil))
        if deviceManager.isDeviceConnected, sessionManager.session.routeIdentifier != nil {
            _ = await routeSyncService.clearActiveRoute()
        }
        sessionManager.session.routeIdentifier = nil
        sessionManager.session.routeRevision = nil
        sessionManager.session.destinationLabel = "No destination"
        sessionManager.session.destinationCoordinate = nil
        sessionManager.session.lastRerouteReason = nil
        sessionManager.session.lastRerouteTimestamp = nil
        onRefreshDiagnostics()
    }
}
