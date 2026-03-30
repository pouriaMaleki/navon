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

struct RoutePlanRequest: Equatable {
    var origin: CoordinatePoint
    var destination: CoordinatePoint
    var providerID: RouteProviderID
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

struct SyncSessionState: Equatable {
    var connectionState: DeviceConnectionState
    var routeSyncState: RouteSyncState
    var lastSyncResult: String
    var lastDeviceName: String?
}

struct CompanionDiagnostics: Equatable {
    var providerName: String
    var routeIdentifier: String
    var routeRevision: Int
    var bleState: String
    var lastSyncResult: String
    var lastRerouteOutcome: String
}
