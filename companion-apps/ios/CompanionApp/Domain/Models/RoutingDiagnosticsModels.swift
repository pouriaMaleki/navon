import Foundation

// MARK: - Event types

enum RoutingDiagEventData: Codable, Equatable {
    case locationUpdate(lat: Double, lon: Double, heading: Double?, speed: Double?, accuracyM: Double?)
    case destinationChanged(label: String, lat: Double, lon: Double)
    case routeAlternativesSuggested(alternatives: [RouteAltInfo])
    case routeSelected(alternativeId: String, providerName: String, routeId: String, label: String)
    case routeStarted
    case routeStopped(reason: String?)
    case exploreAlternatives
    case compassModeChanged(from: String, to: String)
    case audioCueDispatched(cueType: String, messageText: String)
    case nextTurnAlerted(instructionText: String, distanceRemainingM: Double)
    case offRouteDetected(distanceM: Double)
    case rerouteRequested
    case rerouteCompleted(result: String)

    // MARK: Codable

    private enum CodingKeys: String, CodingKey {
        case kind, lat, lon, heading, speed, accuracyM
        case label, alternatives, alternativeId, providerName, routeId
        case instructionText, distanceRemainingM, distanceM
        case from, to, cueType, messageText, reason, result
    }

    enum Kind: String, Codable {
        case locationUpdate, destinationChanged, routeAlternativesSuggested
        case routeSelected, routeStarted, routeStopped, exploreAlternatives
        case compassModeChanged, audioCueDispatched, nextTurnAlerted
        case offRouteDetected, rerouteRequested, rerouteCompleted
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(Kind.self, forKey: .kind)
        switch kind {
        case .locationUpdate:
            self = .locationUpdate(
                lat: try container.decode(Double.self, forKey: .lat),
                lon: try container.decode(Double.self, forKey: .lon),
                heading: try container.decodeIfPresent(Double.self, forKey: .heading),
                speed: try container.decodeIfPresent(Double.self, forKey: .speed),
                accuracyM: try container.decodeIfPresent(Double.self, forKey: .accuracyM)
            )
        case .destinationChanged:
            self = .destinationChanged(
                label: try container.decode(String.self, forKey: .label),
                lat: try container.decode(Double.self, forKey: .lat),
                lon: try container.decode(Double.self, forKey: .lon)
            )
        case .routeAlternativesSuggested:
            self = .routeAlternativesSuggested(
                alternatives: try container.decode([RouteAltInfo].self, forKey: .alternatives)
            )
        case .routeSelected:
            self = .routeSelected(
                alternativeId: try container.decode(String.self, forKey: .alternativeId),
                providerName: try container.decode(String.self, forKey: .providerName),
                routeId: try container.decode(String.self, forKey: .routeId),
                label: try container.decode(String.self, forKey: .label)
            )
        case .routeStarted:
            self = .routeStarted
        case .routeStopped:
            self = .routeStopped(
                reason: try container.decodeIfPresent(String.self, forKey: .reason)
            )
        case .exploreAlternatives:
            self = .exploreAlternatives
        case .compassModeChanged:
            self = .compassModeChanged(
                from: try container.decode(String.self, forKey: .from),
                to: try container.decode(String.self, forKey: .to)
            )
        case .audioCueDispatched:
            self = .audioCueDispatched(
                cueType: try container.decode(String.self, forKey: .cueType),
                messageText: try container.decode(String.self, forKey: .messageText)
            )
        case .nextTurnAlerted:
            self = .nextTurnAlerted(
                instructionText: try container.decode(String.self, forKey: .instructionText),
                distanceRemainingM: try container.decode(Double.self, forKey: .distanceRemainingM)
            )
        case .offRouteDetected:
            self = .offRouteDetected(
                distanceM: try container.decode(Double.self, forKey: .distanceM)
            )
        case .rerouteRequested:
            self = .rerouteRequested
        case .rerouteCompleted:
            self = .rerouteCompleted(
                result: try container.decode(String.self, forKey: .result)
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .locationUpdate(let lat, let lon, let heading, let speed, let accuracyM):
            try container.encode(Kind.locationUpdate, forKey: .kind)
            try container.encode(lat, forKey: .lat)
            try container.encode(lon, forKey: .lon)
            try container.encodeIfPresent(heading, forKey: .heading)
            try container.encodeIfPresent(speed, forKey: .speed)
            try container.encodeIfPresent(accuracyM, forKey: .accuracyM)
        case .destinationChanged(let label, let lat, let lon):
            try container.encode(Kind.destinationChanged, forKey: .kind)
            try container.encode(label, forKey: .label)
            try container.encode(lat, forKey: .lat)
            try container.encode(lon, forKey: .lon)
        case .routeAlternativesSuggested(let alternatives):
            try container.encode(Kind.routeAlternativesSuggested, forKey: .kind)
            try container.encode(alternatives, forKey: .alternatives)
        case .routeSelected(let alternativeId, let providerName, let routeId, let label):
            try container.encode(Kind.routeSelected, forKey: .kind)
            try container.encode(alternativeId, forKey: .alternativeId)
            try container.encode(providerName, forKey: .providerName)
            try container.encode(routeId, forKey: .routeId)
            try container.encode(label, forKey: .label)
        case .routeStarted:
            try container.encode(Kind.routeStarted, forKey: .kind)
        case .routeStopped(let reason):
            try container.encode(Kind.routeStopped, forKey: .kind)
            try container.encodeIfPresent(reason, forKey: .reason)
        case .exploreAlternatives:
            try container.encode(Kind.exploreAlternatives, forKey: .kind)
        case .compassModeChanged(let from, let to):
            try container.encode(Kind.compassModeChanged, forKey: .kind)
            try container.encode(from, forKey: .from)
            try container.encode(to, forKey: .to)
        case .audioCueDispatched(let cueType, let messageText):
            try container.encode(Kind.audioCueDispatched, forKey: .kind)
            try container.encode(cueType, forKey: .cueType)
            try container.encode(messageText, forKey: .messageText)
        case .nextTurnAlerted(let instructionText, let distanceRemainingM):
            try container.encode(Kind.nextTurnAlerted, forKey: .kind)
            try container.encode(instructionText, forKey: .instructionText)
            try container.encode(distanceRemainingM, forKey: .distanceRemainingM)
        case .offRouteDetected(let distanceM):
            try container.encode(Kind.offRouteDetected, forKey: .kind)
            try container.encode(distanceM, forKey: .distanceM)
        case .rerouteRequested:
            try container.encode(Kind.rerouteRequested, forKey: .kind)
        case .rerouteCompleted(let result):
            try container.encode(Kind.rerouteCompleted, forKey: .kind)
            try container.encode(result, forKey: .result)
        }
    }
}

struct RouteAltInfo: Codable, Equatable {
    let providerName: String
    let routeId: String
    let label: String
}

// MARK: - Route Geometry Entry

struct RouteGeometryEntry: Codable, Equatable {
    let routeId: String
    let providerName: String
    let geometry: [CoordinatePoint]
}

// MARK: - Event

struct RoutingDiagEvent: Identifiable, Codable, Equatable {
    let id: String
    let timestampMs: UInt64
    let data: RoutingDiagEventData
}

// MARK: - Session

struct RoutingDiagSession: Identifiable, Codable, Equatable {
    let id: String
    let createdAtMs: UInt64
    var updatedAtMs: UInt64
    var events: [RoutingDiagEvent]
    var routeGeometries: [RouteGeometryEntry]?

    var eventCount: Int { events.count }
    var durationMs: UInt64 {
        guard !events.isEmpty else { return 0 }
        return updatedAtMs - createdAtMs
    }

    var debugPackageText: String {
        let pkg = RoutingDiagDebugPackage(
            formatVersion: 1,
            sessionId: id,
            createdAtMs: createdAtMs,
            eventCount: eventCount,
            events: events,
            routeGeometries: routeGeometries
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(pkg),
              let str = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return str
    }
}

// MARK: - Debug Package

struct RoutingDiagDebugPackage: Codable {
    let formatVersion: Int
    let sessionId: String
    let createdAtMs: UInt64
    let eventCount: Int
    let events: [RoutingDiagEvent]
    let routeGeometries: [RouteGeometryEntry]?
}

// MARK: - Constants

let ROUTING_DIAGNOSTICS_SESSION_LIMIT = 20
let LOCATION_EVENT_THROTTLE_MS: UInt64 = 5000

func newSessionId() -> String {
    "rd-\(UInt64(Date().timeIntervalSince1970 * 1000))-\(UUID().uuidString.prefix(6))"
}

func newEventId(_ counter: inout Int) -> String {
    counter += 1
    return "e\(counter)"
}
