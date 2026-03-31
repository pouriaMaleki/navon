import Foundation

enum RouteProviderID: String, CaseIterable, Identifiable, Codable {
    case hsl
    case osm
    case googleIngest
    case gpxImport
    case fitImport
    case tcxImport
    case garminApi
    case garminFile

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .hsl:
            return "HSL"
        case .osm:
            return "OSM"
        case .googleIngest:
            return "Google Ingest"
        case .gpxImport:
            return "GPX Import"
        case .fitImport:
            return "FIT Import"
        case .tcxImport:
            return "TCX Import"
        case .garminApi:
            return "Garmin API"
        case .garminFile:
            return "Garmin File"
        }
    }

    var isAvailableInV1: Bool {
        self == .hsl
    }
}

struct CoordinatePoint: Equatable, Codable {
    var latitude: Double
    var longitude: Double
}

struct RoutePackageVersion: Equatable, Codable {
    var major: UInt16
    var minor: UInt16

    static let current = RoutePackageVersion(major: 1, minor: 0)
}

enum RouteManeuverType: String, Equatable, Codable {
    case depart
    case straight
    case slightLeft
    case left
    case sharpLeft
    case slightRight
    case right
    case sharpRight
    case uturn
    case roundabout
    case merge
    case ramp
    case arrive
}

struct RouteManeuver: Equatable, Codable {
    var id: String
    var maneuverType: RouteManeuverType
    var location: CoordinatePoint
    var distanceFromStartMeters: Double
    var distanceToNextMeters: Double?
    var instructionText: String?
}

struct RouteSummary: Equatable, Codable {
    var totalDistanceMeters: Double
    var estimatedDurationSeconds: Int
    var startLabel: String?
    var destinationLabel: String?
}

struct RouteProvenance: Equatable, Codable {
    var providerID: RouteProviderID
    var sourceReference: String?
    var generatedAtUnixMs: UInt64
}

struct NormalizedRoutePackage: Equatable, Codable {
    var version: RoutePackageVersion
    var routeIdentifier: String
    var revision: Int
    var geometry: [CoordinatePoint]
    var maneuvers: [RouteManeuver]
    var summary: RouteSummary
    var provenance: RouteProvenance

    var geometryPointCount: Int { geometry.count }
    var maneuverCount: Int { maneuvers.count }

    var summaryLine: String {
        let minutes = max(summary.estimatedDurationSeconds / 60, 1)
        return "\(Int(summary.totalDistanceMeters)) m • \(minutes) min"
    }
}

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
        case .set:
            return "set"
        case .update:
            return "update"
        case .clear:
            return "clear"
        case .status:
            return "status"
        case .rerouteRequest:
            return "reroute_request"
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

struct RoutePlanRequest: Equatable {
    var origin: CoordinatePoint
    var destination: CoordinatePoint
    var providerID: RouteProviderID
}

struct CompanionSettings: Equatable {
    var preferLiveHslRouting: Bool
    var hslSubscriptionKey: String
    var hslEndpointURL: String

    static let defaults = CompanionSettings(
        preferLiveHslRouting: false,
        hslSubscriptionKey: "",
        hslEndpointURL: "https://api.digitransit.fi/routing/v2/hsl/gtfs/v1"
    )
}

struct RouteAlternative: Identifiable, Equatable {
    let id: UUID
    var title: String
    var subtitle: String
    var distanceMeters: Int
    var durationSeconds: Int
    var normalizedPackage: NormalizedRoutePackage
}

struct RoutePreviewModel: Equatable {
    var alternatives: [RouteAlternative]
    var selectedAlternativeID: UUID?
    var routeIdentifier: String?
    var routeRevision: Int?
    var planningNotice: String?

    var selectedAlternative: RouteAlternative? {
        alternatives.first { $0.id == selectedAlternativeID } ?? alternatives.first
    }
}

struct ActiveRouteSession: Equatable {
    var routeIdentifier: String?
    var routeRevision: Int?
    var destinationLabel: String
    var destinationCoordinate: CoordinatePoint?
    var providerID: RouteProviderID
    var lastRerouteReason: String?
    var lastRerouteTimestamp: Date?
}

enum DeviceConnectionState: String, Equatable {
    case disconnected
    case scanning
    case connecting
    case connected
}

enum RouteSyncState: String, Equatable {
    case idle
    case preparing
    case transferring
    case awaitingAck
    case synced
    case failed
}

enum RouteSyncFaultInjectionMode: String, Equatable, Codable, CaseIterable, Identifiable {
    case retryableInterruption
    case writeFailure
    case disconnectAfterChunkWrite
    case dropNextInboundStatus

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .retryableInterruption:
            return "Retryable interruption"
        case .writeFailure:
            return "Write failure"
        case .disconnectAfterChunkWrite:
            return "Disconnect after chunk"
        case .dropNextInboundStatus:
            return "Drop next inbound status"
        }
    }
}

struct RouteTransferProgress: Equatable {
    var transferIdentifier: String
    var messageKind: String
    var routeIdentifier: String?
    var routeRevision: Int?
    var payloadBytes: Int
    var chunkSizeBytes: Int
    var totalChunks: Int
    var acknowledgedChunks: Int
    var retryCount: Int
    var checksumHex: String
    var resumeChunkIndex: Int?
    var lastError: String?

    var percentComplete: Int {
        guard totalChunks > 0 else { return 0 }
        return Int((Double(acknowledgedChunks) / Double(totalChunks) * 100.0).rounded())
    }
}

struct SyncSessionState: Equatable {
    var connectionState: DeviceConnectionState
    var routeSyncState: RouteSyncState
    var lastSyncResult: String
    var lastDeviceName: String?
    var pendingRouteIdentifier: String?
    var pendingRouteRevision: Int?
    var activeRouteIdentifier: String?
    var activeRouteRevision: Int?
    var activeRouteChecksumHex: String?
    var transferProgress: RouteTransferProgress?
    var retryableInterruptionArmed: Bool
    var armedFaultInjectionMode: RouteSyncFaultInjectionMode?
    var lastOutboundMessage: RouteSyncMessage?
    var lastInboundMessage: RouteSyncMessage?
    var lastStatusCode: RouteSyncStatusCode?
}

struct CompanionDiagnostics: Equatable {
    var providerName: String
    var routeIdentifier: String
    var routeRevision: Int
    var bleState: String
    var lastSyncResult: String
    var lastRerouteOutcome: String
}
