import Foundation

enum RouteSuggestionMode: String, CaseIterable, Identifiable, Codable {
    case bestOnly
    case threeRoutes

    var id: String { rawValue }
    var displayName: String {
        switch self {
        case .bestOnly: return "Best route"
        case .threeRoutes: return "3 suggestions"
        }
    }
}

enum RouteStartBehavior: String, CaseIterable, Identifiable, Codable {
    case explicit
    case automatic

    var id: String { rawValue }
    var displayName: String {
        switch self {
        case .explicit: return "Ask before start"
        case .automatic: return "Start automatically"
        }
    }
}

enum RouteSourceMode: String, CaseIterable, Identifiable, Codable {
    case mixed
    case hsl
    case osm

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .mixed: return "Mixed"
        case .hsl: return "HSL"
        case .osm: return "OSM"
        }
    }

    var primaryProviderID: RouteProviderID {
        switch self {
        case .mixed, .hsl:
            return .hsl
        case .osm:
            return .osm
        }
    }

    var providerIDs: [RouteProviderID] {
        switch self {
        case .mixed:
            return [.hsl, .osm]
        case .hsl:
            return [.hsl]
        case .osm:
            return [.osm]
        }
    }
}

enum RouteSuggestionKind: String, CaseIterable, Codable {
    case fastest
    case quieter
    case simpler

    var displayName: String {
        switch self {
        case .fastest: return "Fastest"
        case .quieter: return "Quieter"
        case .simpler: return "Simpler"
        }
    }
}

enum HomeMode: Equatable {
    case planning
    case sendingToDevice
    case phoneGuidance
    case deviceOverview
}

enum HomeCompassMode: Equatable {
    case autoFollow
    case northPreview
    case northLocked
}

struct RoutePlannerPreferences: Equatable, Codable {
    var defaultSourceMode: RouteSourceMode
    var suggestionMode: RouteSuggestionMode
    var startBehavior: RouteStartBehavior

    static let defaults = RoutePlannerPreferences(
        defaultSourceMode: .mixed,
        suggestionMode: .threeRoutes,
        startBehavior: .explicit
    )
}

struct DestinationSearchResult: Identifiable, Equatable {
    var id: String
    var title: String
    var subtitle: String
    var coordinate: CoordinatePoint
}

enum RouteHistorySource: String, Equatable, Codable {
    case recentDestination
    case plannedRoute
    case gpxImport
    case googleMaps
    case shareImport

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        switch rawValue {
        case Self.recentDestination.rawValue:
            self = .recentDestination
        case Self.plannedRoute.rawValue:
            self = .plannedRoute
        case Self.gpxImport.rawValue:
            self = .gpxImport
        case Self.googleMaps.rawValue:
            self = .googleMaps
        case Self.shareImport.rawValue, "partner":
            self = .shareImport
        default:
            self = .plannedRoute
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

struct RouteHistoryItem: Identifiable, Equatable, Codable {
    var id: String
    var title: String
    var subtitle: String
    var source: RouteHistorySource
    var sourceLabel: String
    var createdAt: Date
    var destination: CoordinatePoint?
    var routePackage: NormalizedRoutePackage?
    var occurrenceCount: Int?
}
