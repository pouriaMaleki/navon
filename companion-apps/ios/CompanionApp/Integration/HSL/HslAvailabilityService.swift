import Foundation

/// HSL/Digitransit availability checks — purely geographic now (server handles auth).
enum HslAvailabilityService {
    /// Approximate bounding box for mainland Finland (including Åland). Digitransit's
    /// `finland` router aggregates GTFS feeds nationwide.
    static func isInFinland(_ point: CoordinatePoint) -> Bool {
        (59.7...70.1).contains(point.latitude) && (19.0...31.7).contains(point.longitude)
    }

    /// True when both endpoints of the request fall inside Finland.
    static func isHslApplicableForRequest(_ request: RoutePlanRequest) -> Bool {
        isInFinland(request.origin) && isInFinland(request.destination)
    }

    static func isHslAvailable(request: RoutePlanRequest) -> Bool {
        isHslApplicableForRequest(request)
    }

    static func sourceModeOptions(request: RoutePlanRequest) -> [RouteSourceMode] {
        isHslAvailable(request: request) ? RouteSourceMode.allCases : [.osm]
    }

    /// When HSL is geographically unavailable for this request, collapse to OSM.
    static func normalizeSourceModeForHslAvailability(
        currentSourceMode: inout RouteSourceMode,
        request: RoutePlanRequest
    ) {
        if !isHslAvailable(request: request) && currentSourceMode != .osm {
            currentSourceMode = .osm
        }
    }
}
