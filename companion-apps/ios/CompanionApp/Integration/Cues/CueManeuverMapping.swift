import Foundation

/// Single source of truth for "given a `RouteManeuverType`, what audio cue
/// kind do we emit?" Returns `nil` for maneuver types that produce no cue
/// at all — silence-by-design avoids on-route noise that doesn't match a
/// UI element the rider can act on.
enum CueManeuverMapping {
    static func kind(for type: RouteManeuverType) -> ManeuverKind? {
        switch type {
        case .left, .sharpLeft:           return .left
        case .right, .sharpRight:         return .right
        case .uturn:                      return .uturn
        case .roundabout:                 return .roundabout
        case .merge:                      return .merge
        case .ramp:                       return .ramp
        // Silenced kinds: these reach the maneuver list from the routing
        // adapter but produce no audio cue.
        //   - `.depart` / `.arrive` — the cue stream uses dedicated
        //     `arrived` / `arrivingInM` events, not maneuver entries.
        //   - `.straight` — not a turn; "Next turn in about X meters" /
        //     "Follow the route" with no matching UI element is the bug
        //     this filter exists to prevent.
        case .slightLeft:                return .left
        case .slightRight:               return .right
        case
             .straight,
             .depart, .arrive:
            return nil
        }
    }
}
