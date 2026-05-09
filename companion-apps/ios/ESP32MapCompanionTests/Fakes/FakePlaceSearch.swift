import Foundation
@testable import ESP32MapCompanion

final class FakePlaceSearch: PlaceSearchService {
    var searchResults: [DestinationSearchResult] = []
    var resolveResult: DestinationSearchResult?
    private(set) var searchCalls: [(String, Int)] = []
    private(set) var lastQueryBias: CoordinatePoint?
    private(set) var resolveCalls: [CoordinatePoint] = []

    func searchDestinations(
        matching query: String,
        limit: Int,
        riderBias: CoordinatePoint?
    ) async -> [DestinationSearchResult] {
        searchCalls.append((query, limit))
        lastQueryBias = riderBias
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
