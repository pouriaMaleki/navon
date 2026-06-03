import Foundation

@MainActor
final class SearchController: ObservableObject {
    private let appModel: AppModel
    private let placeSearchService: PlaceSearchService
    private var latestSearchTask: Task<Void, Never>?
    private var latestUrlResolveTask: Task<Void, Never>?
    private let postSelectionLatchSeconds: TimeInterval = 0.35
    private var postSelectionLatchUntil: Date = .distantPast

    var onUrlResolved: ((String, CoordinatePoint) async -> Void)?

    @Published var query = ""
    @Published var isSearchOpen = false
    @Published var suggestions: [DestinationSearchResult] = []
    @Published var visibleSuggestionCount = 10
    @Published var visibleRecentCount = 10
    @Published private(set) var isResolvingUrl: Bool = false
    @Published private(set) var urlResolveError: String?

    init(appModel: AppModel, placeSearchService: PlaceSearchService) {
        self.appModel = appModel
        self.placeSearchService = placeSearchService
    }

    func syncQueryFromCurrentPreview() {
        let sessionTitle = appModel.sessionManager.session.destinationLabel.trimmingCharacters(in: .whitespacesAndNewlines)
        if !Self.isGenericDestinationTitle(sessionTitle) {
            query = sessionTitle
        } else if let destination = appModel.preview.selectedAlternative?.normalizedPackage.summary.destinationLabel,
                  !Self.isGenericDestinationTitle(destination) {
            query = destination
        } else if let route = appModel.preview.selectedAlternative {
            query = route.title
        }
    }

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

    func openSearch() {
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
        if item.id == appModel.routeHistoryService.routeHistoryItems.prefix(visibleRecentCount).last?.id {
            visibleRecentCount += 10
        }
    }

    func loadMoreSuggestionsIfNeeded(for item: DestinationSearchResult) {
        if item.id == suggestions.prefix(visibleSuggestionCount).last?.id {
            visibleSuggestionCount += 10
        }
    }

    func updateQuery(_ newValue: String) {
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

    func cancelActiveSearch() {
        latestSearchTask?.cancel()
    }

    func cancelUrlResolve() {
        latestUrlResolveTask?.cancel()
        latestUrlResolveTask = nil
        isResolvingUrl = false
    }

    func startUrlResolve(_ urlString: String) {
        latestUrlResolveTask?.cancel()
        isResolvingUrl = true
        urlResolveError = nil
        latestUrlResolveTask = Task { [weak self] in
            guard let self else { return }
            let resolution = await appModel.shareImportService.resolveDestinationFromUrl(urlString, using: placeSearchService)
            if Task.isCancelled { return }
            isResolvingUrl = false
            switch resolution {
            case .coordinate(let point, let suggestedTitle):
                let title = suggestedTitle ?? "Imported destination"
                appModel.routeRequest = RoutePlanRequest(
                    origin: appModel.locationService.bestLocation,
                    destination: point,
                    providerID: appModel.currentSourceMode.primaryProviderID
                )
                closeSearch()
                self.query = title
                await onUrlResolved?(title, point)
            case .noDestinationFound:
                urlResolveError = "Couldn't find a destination in that URL."
            case .networkError(let message):
                urlResolveError = "URL expansion failed: \(message)"
            }
        }
    }

    func selectSuggestion(_ suggestion: DestinationSearchResult) {
        applyPostSelectionLatch()
        query = suggestion.title
        appModel.routeRequest = RoutePlanRequest(
            origin: appModel.locationService.bestLocation,
            destination: suggestion.coordinate,
            providerID: appModel.currentSourceMode.primaryProviderID
        )
    }

    func selectRecent(_ item: RouteHistoryItem) {
        applyPostSelectionLatch()
        query = item.title
    }

    func setDestinationFromMap(_ coordinate: CoordinatePoint) {
        applyPostSelectionLatch()
    }

    /// Arms a short post-selection window during which `openSearch()` is a
    /// no-op. Prevents the SwiftUI TextField from re-opening the dropdown
    /// when SwiftUI echoes `set(currentValue)` immediately after a
    /// programmatic `query =` assignment. Every code path that finishes a
    /// destination pick (`selectSuggestion`, `selectRecent`,
    /// `setDestinationFromMap`) must go through here, otherwise the
    /// dropdown reopens and starts a fresh search for the address the
    /// user just selected.
    private func applyPostSelectionLatch() {
        latestSearchTask?.cancel()
        postSelectionLatchUntil = Date().addingTimeInterval(postSelectionLatchSeconds)
    }
}
