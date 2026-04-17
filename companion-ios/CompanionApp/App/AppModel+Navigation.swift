import Foundation

extension AppModel {
    var routeHistoryItems: [RouteHistoryItem] {
        persistence.loadRecentRouteHistory()
    }

    var pendingHomeImportPresentation: PendingHomeImportPresentation? {
        persistence.loadPendingHomeImportPresentation()
    }

    var routePlannerPreferences: RoutePlannerPreferences {
        get { persistence.loadRoutePlannerPreferences() }
        set {
            persistence.saveRoutePlannerPreferences(newValue)
            currentSourceMode = newValue.defaultSourceMode
        }
    }

    @discardableResult
    func recordPlannedPreview(source: RouteHistorySource, sourceLabel: String) -> RouteHistoryItem? {
        guard let selected = preview.selectedAlternative?.normalizedPackage else { return nil }
        let item = RouteHistoryItem(
            id: selected.routeIdentifier,
            title: activeSession.destinationLabel,
            subtitle: selected.summaryLine,
            source: source,
            sourceLabel: sourceLabel,
            createdAt: Date(),
            destination: selected.geometry.last,
            routePackage: selected,
            occurrenceCount: nil
        )
        persistence.saveRouteHistoryItem(item)
        notePersistenceChanged()
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
        notePersistenceChanged()
    }

    func dismissRouteHistoryItem(id: String) {
        persistence.dismissRouteHistoryItem(id: id)
        notePersistenceChanged()
    }

    func activateRouteHistoryItem(_ item: RouteHistoryItem, startImmediately: Bool = false) {
        Task {
            await resetCurrentRouteForHistoryActivation()
            await applyRouteHistoryPreview(item)
            homePreviewRequestID = UUID()
            if startImmediately {
                homeStartRequestID = UUID()
            }
        }
    }

    func clearPreviewSelection() {
        preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
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
        notePersistenceChanged()
    }

    func clearPendingHomeImportPresentation() {
        persistence.clearPendingHomeImportPresentation()
        notePersistenceChanged()
    }

    private func resetCurrentRouteForHistoryActivation() async {
        preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
        if isDeviceConnected, activeSession.routeIdentifier != nil {
            _ = await clearActiveRoute()
        }
        activeSession.routeIdentifier = nil
        activeSession.routeRevision = nil
        activeSession.destinationLabel = "No destination"
        activeSession.destinationCoordinate = nil
        activeSession.lastRerouteReason = nil
        activeSession.lastRerouteTimestamp = nil
        persistence.saveSession(activeSession)
        refreshDiagnostics()
    }
}
