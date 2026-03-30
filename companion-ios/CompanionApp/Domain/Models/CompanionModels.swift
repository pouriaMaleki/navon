import Foundation

enum RouteProviderID: String, CaseIterable, Identifiable {
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
}

struct RoutePreviewModel: Equatable {
    var alternatives: [RouteAlternative]
    var selectedAlternativeID: UUID?
    var routeIdentifier: String?
    var routeRevision: Int?
}

struct ActiveRouteSession: Equatable {
    var routeIdentifier: String?
    var routeRevision: Int?
    var destinationLabel: String
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
