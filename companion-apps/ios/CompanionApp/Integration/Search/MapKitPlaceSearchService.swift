import Foundation
import MapKit
import CoreLocation

protocol PlaceSearchService {
    /// Search for destinations matching `query`, optionally biased toward
    /// `riderBias` so nearby results rank first. Mirrors the web contract —
    /// see `docs/ux-specs.md` line 75.
    func searchDestinations(
        matching query: String,
        limit: Int,
        riderBias: CoordinatePoint?
    ) async -> [DestinationSearchResult]
    func resolveDestination(at coordinate: CoordinatePoint, fallbackTitle: String, preserveFallbackTitle: Bool) async -> DestinationSearchResult?
}

extension PlaceSearchService {
    /// Back-compat wrapper: callers that don't yet supply a rider bias still
    /// compile. The runtime value is nil, so MapKit ranks results globally
    /// and the flow is identical to the old signature.
    func searchDestinations(matching query: String, limit: Int) async -> [DestinationSearchResult] {
        await searchDestinations(matching: query, limit: limit, riderBias: nil)
    }
}

struct MapKitPlaceSearchService: PlaceSearchService {
    func searchDestinations(
        matching query: String,
        limit: Int,
        riderBias: CoordinatePoint?
    ) async -> [DestinationSearchResult] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }
        let request = MKLocalSearch.Request()
        request.naturalLanguageQuery = trimmed
        request.resultTypes = [.pointOfInterest, .address]
        // MapKit takes a region hint to bias toward nearby results. Use a
        // ~25 km span which roughly matches "same city / area" per spec.
        if let bias = riderBias {
            request.region = MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: bias.latitude, longitude: bias.longitude),
                span: MKCoordinateSpan(latitudeDelta: 0.25, longitudeDelta: 0.25)
            )
        }

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
