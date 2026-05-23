import Foundation

enum RouteSyncStatusCode: String, Equatable, Codable {
    case accepted
    case applying
    case active
    case cleared
    case rejected
    case retryableFailure
    case fatalFailure
}

struct RouteSetMessage: Equatable, Codable {
    var route: NormalizedRoutePackage
}

struct RouteUpdateMessage: Equatable, Codable {
    var routeIdentifier: String
    var revision: Int
    var route: NormalizedRoutePackage
}

struct RouteClearMessage: Equatable, Codable {
    var routeIdentifier: String?
}

struct RouteStatusMessage: Equatable, Codable {
    var routeIdentifier: String?
    var revision: Int?
    var status: RouteSyncStatusCode
    var detail: String?
}

struct RouteRerouteRequestMessage: Equatable, Codable {
    var routeIdentifier: String
    var riderLocation: CoordinatePoint
    var reason: String
}

enum RouteSyncMessage: Equatable, Codable {
    case set(RouteSetMessage)
    case update(RouteUpdateMessage)
    case clear(RouteClearMessage)
    case status(RouteStatusMessage)
    case rerouteRequest(RouteRerouteRequestMessage)

    var kindLabel: String {
        switch self {
        case .set:           return "set"
        case .update:        return "update"
        case .clear:         return "clear"
        case .status:        return "status"
        case .rerouteRequest: return "reroute_request"
        }
    }

    var debugSummary: String {
        switch self {
        case .set(let message):
            return "set \(message.route.routeIdentifier) rev \(message.route.revision)"
        case .update(let message):
            return "update \(message.routeIdentifier) rev \(message.revision)"
        case .clear(let message):
            return "clear \(message.routeIdentifier ?? "current")"
        case .status(let message):
            return "status \(message.status.rawValue) \(message.routeIdentifier ?? "none")"
        case .rerouteRequest(let message):
            return "reroute_request \(message.routeIdentifier)"
        }
    }
}
