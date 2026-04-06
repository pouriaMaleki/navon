import Foundation

enum RouteSuggestionMode: String, CaseIterable, Identifiable {
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

enum RouteStartBehavior: String, CaseIterable, Identifiable {
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

struct RoutePlannerPreferences: Equatable {
    var providerID: RouteProviderID
    var suggestionMode: RouteSuggestionMode
    var startBehavior: RouteStartBehavior

    static let defaults = RoutePlannerPreferences(
        providerID: .hsl,
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

enum RouteHistorySource: String, Equatable {
    case recentDestination
    case plannedRoute
    case gpxImport
    case googleMaps
    case partner
}

struct RouteHistoryItem: Identifiable, Equatable {
    var id: String
    var title: String
    var subtitle: String
    var source: RouteHistorySource
    var sourceLabel: String
    var createdAt: Date
    var destination: CoordinatePoint?
    var routePackage: NormalizedRoutePackage?
}
