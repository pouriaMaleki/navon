import Foundation

/// Pure mapping from routing-domain types to the Live Activity content
/// state. No UIKit, no ActivityKit calls — keeps the visual maneuver layer
/// fully unit-testable and consistent with the audio cue layer.
enum LiveActivityMapper {

    static let backToBackThresholdM = CueEngine.backToBackThresholdM

    static func glyph(
        primary: RouteManeuverType,
        followUp: RouteManeuverType?,
        gapMeters: Double?
    ) -> ManeuverGlyph {
        if let f = followUp,
           let g = gapMeters,
           g <= Self.backToBackThresholdM,
           let pSide = lrSide(primary),
           let fSide = lrSide(f) {
            switch (pSide, fSide) {
            case (.left, .left):   return .leftThenLeft
            case (.left, .right):  return .leftThenRight
            case (.right, .left):  return .rightThenLeft
            case (.right, .right): return .rightThenRight
            }
        }
        return atomic(primary)
    }

    static func contentState(
        route: NormalizedRoutePackage,
        progressDistanceM: Double,
        offRoute: Bool,
        rerouting: Bool,
        arrived: Bool,
        isImperial: Bool,
        now: Date = Date()
    ) -> RouteGuidanceActivityAttributes.ContentState? {
        let total = route.summary.totalDistanceMeters
        let remaining = max(0, total - progressDistanceM)
        let nowMs = UInt64(now.timeIntervalSince1970 * 1000)

        if arrived {
            return RouteGuidanceActivityAttributes.ContentState(
                glyph: .arrive,
                distanceToNextM: 0,
                distanceRemainingM: 0,
                etaUnixMs: nowMs,
                status: .arrived,
                isImperial: isImperial
            )
        }

        // Pick the next maneuver: skip depart, skip anything already passed.
        let upcomingIdx = route.maneuvers.firstIndex { m in
            m.maneuverType != .depart && m.distanceFromStartMeters > progressDistanceM
        }
        guard let idx = upcomingIdx else {
            // No upcoming maneuver — treat as effectively at destination.
            return RouteGuidanceActivityAttributes.ContentState(
                glyph: .arrive,
                distanceToNextM: 0,
                distanceRemainingM: remaining,
                etaUnixMs: nowMs,
                status: status(offRoute: offRoute, rerouting: rerouting),
                isImperial: isImperial
            )
        }

        let primary = route.maneuvers[idx]
        let follow: RouteManeuver? = (idx + 1 < route.maneuvers.count) ? route.maneuvers[idx + 1] : nil
        let gap: Double? = follow.map { $0.distanceFromStartMeters - primary.distanceFromStartMeters }
        let g = glyph(primary: primary.maneuverType, followUp: follow?.maneuverType, gapMeters: gap)
        let distanceToNext = max(0, primary.distanceFromStartMeters - progressDistanceM)

        // ETA: remaining fraction of the total estimated duration ahead of `now`.
        // Falls back to `now` when we have no usable distance to scale against.
        let durationS = Double(route.summary.estimatedDurationSeconds)
        let secondsAhead: Double = (total > 0) ? (remaining / total) * durationS : 0
        let etaMs = nowMs + UInt64(secondsAhead * 1000)

        return RouteGuidanceActivityAttributes.ContentState(
            glyph: g,
            distanceToNextM: distanceToNext,
            distanceRemainingM: remaining,
            etaUnixMs: etaMs,
            status: status(offRoute: offRoute, rerouting: rerouting),
            isImperial: isImperial
        )
    }

    private static func status(offRoute: Bool, rerouting: Bool) -> GuidanceStatus {
        if offRoute { return .offRoute }
        if rerouting { return .rerouting }
        return .onRoute
    }

    private enum LRSide { case left, right }

    /// Folds the slight / sharp / plain L+R variants into the side family
    /// used to compose compound glyphs. Anything outside this family
    /// (uturn, roundabout, merge, ramp, straight, depart, arrive) returns
    /// nil so the compound branch falls back to the atomic glyph.
    private static func lrSide(_ t: RouteManeuverType) -> LRSide? {
        switch t {
        case .left, .slightLeft, .sharpLeft:    return .left
        case .right, .slightRight, .sharpRight: return .right
        default: return nil
        }
    }

    private static func atomic(_ t: RouteManeuverType) -> ManeuverGlyph {
        switch t {
        case .depart:      return .depart
        case .straight:    return .straight
        case .slightLeft:  return .slightLeft
        case .left:        return .left
        case .sharpLeft:   return .sharpLeft
        case .slightRight: return .slightRight
        case .right:       return .right
        case .sharpRight:  return .sharpRight
        case .uturn:       return .uturn
        case .roundabout:  return .roundabout
        case .merge:       return .merge
        case .ramp:        return .ramp
        case .arrive:      return .arrive
        }
    }
}
