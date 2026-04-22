import Foundation
@testable import ESP32MapCompanion

final class FakePlaceSearch: PlaceSearchService {
    var searchResults: [DestinationSearchResult] = []
    var resolveResult: DestinationSearchResult?
    private(set) var searchCalls: [(String, Int)] = []
    private(set) var resolveCalls: [CoordinatePoint] = []

    func searchDestinations(matching query: String, limit: Int) async -> [DestinationSearchResult] {
        searchCalls.append((query, limit))
        return searchResults
    }

    func resolveDestination(
        at coordinate: CoordinatePoint,
        fallbackTitle: String,
        preserveFallbackTitle: Bool
    ) async -> DestinationSearchResult? {
        resolveCalls.append(coordinate)
        return resolveResult
    }
}
