import Foundation

@MainActor
final class HomeViewModel: ObservableObject {
    @Published var query = ""
    @Published var isSearchOpen = false
    @Published var suggestions: [DestinationSearchResult] = []
    @Published var visibleSuggestionCount = 10
    @Published var visibleRecentCount = 10
    @Published var activeRouteIdentifier: String?

    private let appModel: AppModel
    private let placeSearchService: PlaceSearchService
    private var latestSearchTask: Task<Void, Never>?

    init(appModel: AppModel, placeSearchService: PlaceSearchService = MapKitPlaceSearchService()) {
        self.appModel = appModel
        self.placeSearchService = placeSearchService
    }

    var plannerPreferences: RoutePlannerPreferences {
        get { appModel.routePlannerPreferences }
        set { appModel.routePlannerPreferences = newValue }
    }

    var recentItems: [RouteHistoryItem] {
        Array(appModel.routeHistoryItems.prefix(visibleRecentCount))
    }

    var visibleSuggestions: [DestinationSearchResult] {
        Array(suggestions.prefix(visibleSuggestionCount))
    }

    var previewAlternatives: [RouteAlternative] {
        if plannerPreferences.suggestionMode == .bestOnly {
            return Array(appModel.preview.alternatives.prefix(1))
        }
        return Array(appModel.preview.alternatives.prefix(3))
    }

    var selectedPreview: RouteAlternative? {
        appModel.preview.selectedAlternative
    }

    var activeRoute: NormalizedRoutePackage? {
        guard activeRouteIdentifier != nil else { return nil }
        return selectedPreview?.normalizedPackage
    }

    var previewRoute: NormalizedRoutePackage? {
        selectedPreview?.normalizedPackage
    }

    var destinationCoordinate: CoordinatePoint? {
        activeRoute?.geometry.last ?? previewRoute?.geometry.last
    }

    var originCoordinate: CoordinatePoint? {
        activeRoute?.geometry.first ?? previewRoute?.geometry.first
    }

    var displayedRouteCoordinates: [CoordinatePoint] {
        activeRoute?.geometry ?? previewRoute?.geometry ?? []
    }

    var shouldShowSearchPanel: Bool {
        isSearchOpen && (!query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !recentItems.isEmpty)
    }

    func openSearch() {
        isSearchOpen = true
        visibleRecentCount = 10
        visibleSuggestionCount = 10
    }

    func closeSearch() {
        isSearchOpen = false
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
        query = newValue
        visibleSuggestionCount = 10
        latestSearchTask?.cancel()
        guard !newValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            suggestions = []
            return
        }
        latestSearchTask = Task { [weak self] in
            guard let self else { return }
            let results = await placeSearchService.searchDestinations(matching: newValue, limit: 30)
            if !Task.isCancelled {
                suggestions = results
            }
        }
    }

    func selectSuggestion(_ suggestion: DestinationSearchResult) {
        Task {
            appModel.selectedProviderID = plannerPreferences.providerID
            appModel.routeRequest = RoutePlanRequest(
                origin: appModel.simulatedRiderLocation,
                destination: suggestion.coordinate,
                providerID: plannerPreferences.providerID
            )
            await appModel.planRoute()
            appModel.recordRecentDestination(title: suggestion.title, coordinate: suggestion.coordinate)
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: plannerPreferences.providerID.displayName)
            query = suggestion.title
            if plannerPreferences.startBehavior == .automatic {
                startSelectedRoute()
            }
            closeSearch()
        }
    }

    func selectRecent(_ item: RouteHistoryItem) {
        Task {
            if let package = item.routePackage {
                let alternative = RouteAlternative(
                    id: UUID(),
                    title: item.title,
                    subtitle: item.subtitle,
                    distanceMeters: Int(package.summary.totalDistanceMeters.rounded()),
                    durationSeconds: package.summary.estimatedDurationSeconds,
                    normalizedPackage: package
                )
                appModel.preview = RoutePreviewModel(
                    alternatives: [alternative],
                    selectedAlternativeID: alternative.id,
                    routeIdentifier: package.routeIdentifier,
                    routeRevision: package.revision,
                    planningNotice: item.sourceLabel
                )
                if let destination = item.destination {
                    appModel.routeRequest = RoutePlanRequest(origin: appModel.simulatedRiderLocation, destination: destination, providerID: plannerPreferences.providerID)
                }
            } else if let destination = item.destination {
                appModel.selectedProviderID = plannerPreferences.providerID
                appModel.routeRequest = RoutePlanRequest(origin: appModel.simulatedRiderLocation, destination: destination, providerID: plannerPreferences.providerID)
                await appModel.planRoute()
                appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: plannerPreferences.providerID.displayName)
            }
            if plannerPreferences.startBehavior == .automatic {
                startSelectedRoute()
            }
            closeSearch()
        }
    }

    func setDestinationFromMap(_ coordinate: CoordinatePoint) {
        let manualDrop = DestinationSearchResult(
            id: "long-press-\(coordinate.latitude)-\(coordinate.longitude)",
            title: "Dropped pin",
            subtitle: "Map destination",
            coordinate: coordinate
        )
        selectSuggestion(manualDrop)
    }

    func selectAlternative(_ alternativeID: UUID) {
        appModel.selectAlternative(alternativeID)
    }

    func startSelectedRoute() {
        closeSearch()
        activeRouteIdentifier = selectedPreview?.normalizedPackage.routeIdentifier
    }

    func stopGuidance() {
        activeRouteIdentifier = nil
        guard let destination = destinationCoordinate else { return }
        Task {
            appModel.selectedProviderID = plannerPreferences.providerID
            appModel.routeRequest = RoutePlanRequest(origin: appModel.simulatedRiderLocation, destination: destination, providerID: plannerPreferences.providerID)
            await appModel.planRoute()
            appModel.recordPlannedPreview(source: .plannedRoute, sourceLabel: plannerPreferences.providerID.displayName)
        }
    }

    func clearPreview() {
        activeRouteIdentifier = nil
        query = ""
        suggestions = []
        closeSearch()
        appModel.clearPreviewSelection()
    }
}
