import Foundation
import MapKit

protocol PlaceSearchService {
    func searchDestinations(matching query: String, limit: Int) async -> [DestinationSearchResult]
}

struct MapKitPlaceSearchService: PlaceSearchService {
    func searchDestinations(matching query: String, limit: Int) async -> [DestinationSearchResult] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }
        let request = MKLocalSearch.Request()
        request.naturalLanguageQuery = trimmed
        request.resultTypes = .pointOfInterest

        do {
            let response = try await MKLocalSearch(request: request).start()
            return response.mapItems.prefix(limit).enumerated().map { index, item in
                DestinationSearchResult(
                    id: "search-\(index)-\(item.placemark.coordinate.latitude)-\(item.placemark.coordinate.longitude)",
                    title: item.name ?? trimmed,
                    subtitle: [item.placemark.title, item.placemark.locality].compactMap { $0 }.joined(separator: " • "),
                    coordinate: CoordinatePoint(
                        latitude: item.placemark.coordinate.latitude,
                        longitude: item.placemark.coordinate.longitude
                    )
                )
            }
        } catch {
            return []
        }
    }
}
