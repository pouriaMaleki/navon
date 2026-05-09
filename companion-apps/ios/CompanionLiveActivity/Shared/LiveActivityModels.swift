import Foundation
#if canImport(ActivityKit)
import ActivityKit
#endif

/// ActivityKit attributes for the routing-guidance Live Activity. Static
/// fields (here on the outer type) are set once when the activity is
/// requested; dynamic fields live in `ContentState` and are pushed on
/// every guidance tick the coordinator processes.
@available(iOS 16.1, *)
@available(macOS, unavailable)
@available(watchOS, unavailable)
@available(tvOS, unavailable)
public struct RouteGuidanceActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable {
        public var glyph: ManeuverGlyph
        public var distanceToNextM: Double
        public var distanceRemainingM: Double
        public var etaUnixMs: UInt64
        public var status: GuidanceStatus
        public var isImperial: Bool

        public init(
            glyph: ManeuverGlyph,
            distanceToNextM: Double,
            distanceRemainingM: Double,
            etaUnixMs: UInt64,
            status: GuidanceStatus,
            isImperial: Bool
        ) {
            self.glyph = glyph
            self.distanceToNextM = distanceToNextM
            self.distanceRemainingM = distanceRemainingM
            self.etaUnixMs = etaUnixMs
            self.status = status
            self.isImperial = isImperial
        }
    }

    public let routeId: String

    public init(routeId: String) {
        self.routeId = routeId
    }
}

/// Visual maneuver glyph rendered as a vector asset in the widget. Atomic
/// cases mirror `RouteManeuverType`; compound cases fold a back-to-back
/// pair (≤ 30 m apart, mirroring `CueEngine.backToBackThresholdM`) so the
/// rider sees one combined arrow on screen, matching the audio cue layer.
public enum ManeuverGlyph: String, Codable, Hashable, CaseIterable {
    // Atomic
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
    case depart
    case arrive
    // Compound — only the L/R family combines visually; everything else
    // falls back to its atomic primary glyph.
    case leftThenLeft
    case leftThenRight
    case rightThenLeft
    case rightThenRight

    public var assetName: String { "maneuver-\(rawValue)" }
}

public enum GuidanceStatus: String, Codable, Hashable {
    case onRoute
    case offRoute
    case rerouting
    case arrived
}
