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
        case .slightLeft:                 return .slightLeft
        case .slightRight:                return .slightRight
        case .uturn:                      return .uturn
        case .roundabout:                 return .roundabout
        case .merge:                      return .merge
        case .ramp:                       return .ramp
        case
             .straight,
             .depart, .arrive:
            return nil
        }
    }
}
