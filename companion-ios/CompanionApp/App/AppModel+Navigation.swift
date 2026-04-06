import Foundation

extension AppModel {
    var routeHistoryItems: [RouteHistoryItem] {
        persistence.loadRecentRouteHistory()
    }

    var routePlannerPreferences: RoutePlannerPreferences {
        get { persistence.loadRoutePlannerPreferences() }
        set { persistence.saveRoutePlannerPreferences(newValue) }
    }

    func recordPlannedPreview(source: RouteHistorySource, sourceLabel: String) {
        guard let selected = preview.selectedAlternative?.normalizedPackage else { return }
        persistence.saveRouteHistoryItem(
            RouteHistoryItem(
                id: selected.routeIdentifier,
                title: selected.summary.destinationLabel ?? "Route",
                subtitle: selected.summaryLine,
                source: source,
                sourceLabel: sourceLabel,
                createdAt: Date(),
                destination: selected.geometry.last,
                routePackage: selected
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
                routePackage: nil
            )
        )
        persistence.saveRecentDestination(coordinate)
    }

    func dismissRouteHistoryItem(id: String) {
        persistence.dismissRouteHistoryItem(id: id)
    }
}
