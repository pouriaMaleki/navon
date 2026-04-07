import Foundation

extension AppModel {
    var routeHistoryItems: [RouteHistoryItem] {
        persistence.loadRecentRouteHistory()
    }

    var routePlannerPreferences: RoutePlannerPreferences {
        get { persistence.loadRoutePlannerPreferences() }
        set {
            persistence.saveRoutePlannerPreferences(newValue)
            currentSourceMode = newValue.defaultSourceMode
        }
    }

    func recordPlannedPreview(source: RouteHistorySource, sourceLabel: String) {
        guard let selected = preview.selectedAlternative?.normalizedPackage else { return }
        persistence.saveRouteHistoryItem(
            RouteHistoryItem(
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
        )
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
    }

    func dismissRouteHistoryItem(id: String) {
        persistence.dismissRouteHistoryItem(id: id)
    }

    func activateRouteHistoryItem(_ item: RouteHistoryItem, startImmediately: Bool = false) {
        Task {
            await applyRouteHistoryPreview(item)
            homePreviewRequestID = UUID()
            if startImmediately {
                homeStartRequestID = UUID()
            }
        }
    }

    func clearPreviewSelection() {
        preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
        homePreviewRequestID = UUID()
    }
}
