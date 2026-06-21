import Foundation

enum RouteProviderID: String, CaseIterable, Identifiable, Codable {
    case hsl
    case osm
    case gpxImport
    case fitImport
    case tcxImport

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .hsl:
            return "HSL"
        case .osm:
            return "OSM"
        case .gpxImport:
            return "GPX Import"
        case .fitImport:
            return "FIT Import"
        case .tcxImport:
            return "TCX Import"
        }
    }

    var isAvailableInV1: Bool {
        self == .hsl || self == .gpxImport
    }

    var supportsCompanionPreview: Bool {
        true
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        switch rawValue {
        case Self.hsl.rawValue:
            self = .hsl
        case Self.osm.rawValue, "googleIngest", "garminApi":
            self = .osm
        case Self.gpxImport.rawValue, "garminFile":
            self = .gpxImport
        case Self.fitImport.rawValue:
            self = .fitImport
        case Self.tcxImport.rawValue:
            self = .tcxImport
        default:
            self = .osm
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
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

struct RerouteContext: Equatable {
    var headingDegrees: Double?
    var speedMps: Double?
}

enum SpeedUnit: String, CaseIterable, Identifiable, Codable {
    case kph
    case mph

    var id: String { rawValue }

    var label: String {
        switch self {
        case .kph: return "km/h"
        case .mph: return "mph"
        }
    }
}

struct CompanionSettings: Equatable, Codable {
    var hslEndpointURL: String
    /// Cyclist's planning speed in km/h. Used to override route ETA so that
    /// `estimatedDurationSeconds = totalDistanceMeters / (cyclingSpeedKph / 3.6)`.
    /// HSL Digitransit defaults to a slow bike speed and routinely returns
    /// inflated ETAs; override applies to both live and sample HSL itineraries.
    var cyclingSpeedKph: Double
    /// Display unit for the live-speed badge.
    var speedUnit: SpeedUnit
    /// Persistent camera distance (meters) for riding-mode follow-rider.
    /// The on-map zoom +/- buttons write here; `nil` falls back to the
    /// built-in default of 1200 m. Overview/planning zooms are NOT
    /// persisted.
    var ridingCameraDistanceM: Double?
    /// Prevents the screen from sleeping while a route is active
    /// (`UIApplication.shared.isIdleTimerDisabled = true`).
    var keepScreenOn: Bool
    /// Permission gate for Always location authorization. The user must
    /// flip to "Always" in iOS Settings; surface a hint when the OS keeps
    /// the app on When-In-Use after our request.
    var allowBackgroundGps: Bool
    /// Audio cues during routing. On by default, but gated on
    /// `allowBackgroundGps` at the UI and runtime layer.
    var audioCuesEnabled: Bool
    /// When true (the default), audio cues are suppressed while the rider
    /// has the app foregrounded — their phone screen is already showing
    /// the map — and only fire after the screen locks or they switch
    /// apps. Toggle off to hear cues even when the app is open.
    var audioCuesOnlyInBackground: Bool
    /// Lock-screen Live Activity (ActivityKit). Gated on `allowBackgroundGps`.
    var liveActivityEnabled: Bool
    /// When enabled, routing activities are recorded into timestamped
    /// diagnostics sessions stored on-device.
    var routingDiagnosticsEnabled: Bool
    /// App language preference. `.system` follows
    /// `Bundle.main.preferredLocalizations`; concrete cases override it.
    /// New shipped locales must be added to `AppLanguage`.
    var language: AppLanguage
    /// Distance unit used for both UI labels and spoken voice cues.
    /// `.system` resolves at format time from the user's locale.
    var distanceUnit: DistanceUnitPref

    static let defaults = CompanionSettings(
        hslEndpointURL: "https://navon.bike/api/hsl/routing",
        cyclingSpeedKph: 18,
        speedUnit: .kph,
        ridingCameraDistanceM: nil,
        keepScreenOn: false,
        allowBackgroundGps: false,
        audioCuesEnabled: true,
        audioCuesOnlyInBackground: true,
        liveActivityEnabled: false,
        routingDiagnosticsEnabled: false,
        language: .system,
        distanceUnit: .system
    )

    /// Tolerant decode so existing on-disk settings (no `cyclingSpeedKph` /
    /// `speedUnit` field) keep working after upgrade — the missing fields
    /// fall back to the defaults instead of failing the decode entirely.
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.hslEndpointURL = try container.decodeIfPresent(String.self, forKey: .hslEndpointURL)
            ?? Self.defaults.hslEndpointURL
        self.cyclingSpeedKph = try container.decodeIfPresent(Double.self, forKey: .cyclingSpeedKph)
            ?? Self.defaults.cyclingSpeedKph
        self.speedUnit = try container.decodeIfPresent(SpeedUnit.self, forKey: .speedUnit)
            ?? Self.defaults.speedUnit
        self.ridingCameraDistanceM = try container.decodeIfPresent(Double.self, forKey: .ridingCameraDistanceM)
        self.keepScreenOn = try container.decodeIfPresent(Bool.self, forKey: .keepScreenOn)
            ?? Self.defaults.keepScreenOn
        self.allowBackgroundGps = try container.decodeIfPresent(Bool.self, forKey: .allowBackgroundGps)
            ?? Self.defaults.allowBackgroundGps
        self.audioCuesEnabled = try container.decodeIfPresent(Bool.self, forKey: .audioCuesEnabled)
            ?? Self.defaults.audioCuesEnabled
        self.audioCuesOnlyInBackground = try container.decodeIfPresent(Bool.self, forKey: .audioCuesOnlyInBackground)
            ?? Self.defaults.audioCuesOnlyInBackground
        self.liveActivityEnabled = try container.decodeIfPresent(Bool.self, forKey: .liveActivityEnabled)
            ?? Self.defaults.liveActivityEnabled
        self.routingDiagnosticsEnabled = try container.decodeIfPresent(Bool.self, forKey: .routingDiagnosticsEnabled)
            ?? Self.defaults.routingDiagnosticsEnabled
        self.language = try container.decodeIfPresent(AppLanguage.self, forKey: .language)
            ?? Self.defaults.language
        self.distanceUnit = try container.decodeIfPresent(DistanceUnitPref.self, forKey: .distanceUnit)
            ?? Self.defaults.distanceUnit
    }

    init(
        hslEndpointURL: String,
        cyclingSpeedKph: Double,
        speedUnit: SpeedUnit,
        ridingCameraDistanceM: Double?,
        keepScreenOn: Bool = false,
        allowBackgroundGps: Bool = false,
        audioCuesEnabled: Bool = true,
        audioCuesOnlyInBackground: Bool = true,
        liveActivityEnabled: Bool = false,
        routingDiagnosticsEnabled: Bool = false,
        language: AppLanguage = .system,
        distanceUnit: DistanceUnitPref = .system
    ) {
        self.hslEndpointURL = hslEndpointURL
        self.cyclingSpeedKph = cyclingSpeedKph
        self.speedUnit = speedUnit
        self.ridingCameraDistanceM = ridingCameraDistanceM
        self.keepScreenOn = keepScreenOn
        self.allowBackgroundGps = allowBackgroundGps
        self.audioCuesEnabled = audioCuesEnabled
        self.audioCuesOnlyInBackground = audioCuesOnlyInBackground
        self.liveActivityEnabled = liveActivityEnabled
        self.routingDiagnosticsEnabled = routingDiagnosticsEnabled
        self.language = language
        self.distanceUnit = distanceUnit
    }

    private enum CodingKeys: String, CodingKey {
        case hslEndpointURL
        case cyclingSpeedKph
        case speedUnit
        case ridingCameraDistanceM
        case keepScreenOn
        case allowBackgroundGps
        case audioCuesEnabled
        case audioCuesOnlyInBackground
        case liveActivityEnabled
        case routingDiagnosticsEnabled
        case language
        case distanceUnit
    }
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

/// Locally persisted record of the BLE peripheral the user has paired with.
/// `identifier` is the platform-native peer ID — `peripheral.identifier.uuidString`
/// on iOS, BLE MAC on Android. The JSON wire format is shared with Android via
/// `parity-fixtures/data/paired_peripheral.json` so both companions can decode
/// it byte-for-byte.
struct PairedPeripheralRecord: Codable, Equatable {
    let identifier: String
    let friendlyName: String
    let pairedAt: Date
}

enum PairingFlowState: Equatable {
    case idle
    case instructions
    case scanning
    case connecting
    case confirming
    case succeeded
    case failed(String)
}

struct ActiveRouteSession: Equatable, Codable {
    var routeIdentifier: String?
    var routeRevision: Int?
    var destinationLabel: String
    var destinationCoordinate: CoordinatePoint?
    var providerID: RouteProviderID
    var sourceMode: RouteSourceMode
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

struct RouteTransferProgress: Equatable, Codable {
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
        return Int((Double(acknowledgedChunks) / Double(totalChunks)) * 100.0)
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

    static let empty = SyncSessionState(
        connectionState: .disconnected,
        routeSyncState: .idle,
        lastSyncResult: "Not sent yet",
        lastDeviceName: nil,
        pendingRouteIdentifier: nil,
        pendingRouteRevision: nil,
        activeRouteIdentifier: nil,
        activeRouteRevision: nil,
        activeRouteChecksumHex: nil,
        transferProgress: nil,
        retryableInterruptionArmed: false,
        armedFaultInjectionMode: nil,
        lastOutboundMessage: nil,
        lastInboundMessage: nil,
        lastStatusCode: nil
    )
}

struct CompanionDiagnostics: Equatable {
    var providerName: String
    var routeIdentifier: String
    var routeRevision: Int
    var bleState: String
    var lastSyncResult: String
    var lastRerouteOutcome: String
}
