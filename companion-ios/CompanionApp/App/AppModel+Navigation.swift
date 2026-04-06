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

    func activateRouteHistoryItem(_ item: RouteHistoryItem, startImmediately: Bool = false) {
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
            routeRequest = RoutePlanRequest(
                origin: simulatedRiderLocation,
                destination: item.destination ?? package.geometry.last ?? routeRequest.destination,
                providerID: routePlannerPreferences.providerID
            )
        } else if let destination = item.destination {
            selectedProviderID = routePlannerPreferences.providerID
            routeRequest = RoutePlanRequest(origin: simulatedRiderLocation, destination: destination, providerID: routePlannerPreferences.providerID)
            Task {
                await planRoute()
            }
        }
        homePreviewRequestID = UUID()
        if startImmediately {
            homeStartRequestID = UUID()
        }
    }

    func clearPreviewSelection() {
        preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
        homePreviewRequestID = UUID()
    }
}
