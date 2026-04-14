import Foundation
import MapKit
import CoreLocation

protocol PlaceSearchService {
    func searchDestinations(matching query: String, limit: Int) async -> [DestinationSearchResult]
    func resolveDestination(at coordinate: CoordinatePoint, fallbackTitle: String, preserveFallbackTitle: Bool) async -> DestinationSearchResult?
}

struct MapKitPlaceSearchService: PlaceSearchService {
    func searchDestinations(matching query: String, limit: Int) async -> [DestinationSearchResult] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }
        let request = MKLocalSearch.Request()
        request.naturalLanguageQuery = trimmed
        request.resultTypes = [.pointOfInterest, .address]

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

    func resolveDestination(
        at coordinate: CoordinatePoint,
        fallbackTitle: String = "Dropped pin",
        preserveFallbackTitle: Bool = false
    ) async -> DestinationSearchResult? {
        let location = CLLocation(latitude: coordinate.latitude, longitude: coordinate.longitude)
        do {
            let placemarks = try await CLGeocoder().reverseGeocodeLocation(location)
            guard let placemark = placemarks.first else { return nil }
            let title = preserveFallbackTitle ? fallbackTitle : simpleTitle(for: placemark, fallbackTitle: fallbackTitle)
            let subtitle = [placemark.locality, placemark.administrativeArea, placemark.country].compactMap { $0 }.joined(separator: " • ")
            return DestinationSearchResult(
                id: "reverse-\(coordinate.latitude)-\(coordinate.longitude)",
                title: title,
                subtitle: subtitle,
                coordinate: coordinate
            )
        } catch {
            return nil
        }
    }

    private func simpleTitle(for placemark: CLPlacemark, fallbackTitle: String) -> String {
        if let thoroughfare = placemark.thoroughfare, !thoroughfare.isEmpty {
            let components = [thoroughfare, placemark.subThoroughfare].compactMap { value -> String? in
                guard let value, !value.isEmpty else { return nil }
                return value
            }
            if !components.isEmpty {
                return components.joined(separator: " ")
            }
        }
        if let name = placemark.name, !name.isEmpty {
            return name
        }
        if let locality = placemark.locality, !locality.isEmpty {
            return locality
        }
        return fallbackTitle
    }
}
