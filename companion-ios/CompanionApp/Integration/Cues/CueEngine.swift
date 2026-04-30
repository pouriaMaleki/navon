import Foundation

/// Audio-cue trigger engine. Pure data-in / data-out — consumed by the
/// wiring layer which assembles a `CueSnapshot` each guidance tick and
/// routes resulting events to a `SpeechPort`.
///
/// Spec: docs/ux-specs.md lines 133-143.

enum ManeuverKind {
    case left, right
    case keepLeft, keepRight
    case exitLeft, exitRight
    case uturn
    case generic
}

struct CueManeuver: Equatable {
    let id: String
    let kind: ManeuverKind
    let distanceFromStartM: Double
}

struct CueSnapshot {
    let routeId: String?
    let pairedWithDevice: Bool
    let progressDistanceM: Double
    /// Excluding depart/arrive; ordered by distanceFromStartM ascending.
    let maneuvers: [CueManeuver]
    let offRoute: Bool
    let rerouting: Bool
    let arrived: Bool
    let distanceFromRouteM: Double
    let routeTotalDistanceM: Double
}

enum CueEvent: Equatable {
    case routeStarted
    case turn50m(ManeuverKind)
    case turn10m(ManeuverKind)
    case nextTurnInAbout(turnKind: ManeuverKind, distanceM: Double)
    case arrivingInM(distanceM: Double)
    case arrived
    case offTrack
    case rerouting
    case repeatedOffTrackSilence
    case onTrack
}

struct CueEngineState: Equatable {
    var lastRouteId: String? = nil
    var routeStartedAnnounced: Bool = false
    var announced50m: Set<String> = []
    var announced10m: Set<String> = []
    var announcedNextTurnAfter: Set<String> = []
    var approachingDestinationAnnounced: Bool = false
    var arrivedAnnounced: Bool = false
    var offRouteEpisodeCount: Int = 0
    var prevOffRoute: Bool = false
    var prevRerouting: Bool = false
    var silenced: Bool = false
    var consecutiveOnRouteSamples: Int = 0
    var onTrackAnnounced: Bool = false
}

enum CueEngine {
    private static let approach50M = 50.0
    private static let approach10M = 10.0
    private static let passedTurnM = 10.0
    private static let onTrackConfirmSamples = 5
    private static let onTrackCorridorM = 22.0
    private static let repeatOffTrackSilenceThreshold = 2

    struct Result {
        let events: [CueEvent]
        let nextState: CueEngineState
    }

    static func tick(snapshot: CueSnapshot, state: CueEngineState) -> Result {
        if snapshot.pairedWithDevice {
            return Result(events: [], nextState: state)
        }

        var s: CueEngineState
        if snapshot.routeId != state.lastRouteId {
            s = CueEngineState(lastRouteId: snapshot.routeId)
        } else {
            s = state
        }

        var events: [CueEvent] = []

        if let _ = snapshot.routeId, !s.routeStartedAnnounced {
            events.append(.routeStarted)
            s.routeStartedAnnounced = true
        }

        let offRouteRose = !s.prevOffRoute && snapshot.offRoute
        var offRouteEpisodeCount = s.offRouteEpisodeCount
        if offRouteRose { offRouteEpisodeCount += 1 }

        var silenced = s.silenced
        var onTrackAnnounced = s.onTrackAnnounced
        var consecutiveOnRouteSamples = s.consecutiveOnRouteSamples

        if !snapshot.offRoute && snapshot.distanceFromRouteM < Self.onTrackCorridorM {
            consecutiveOnRouteSamples += 1
        } else {
            consecutiveOnRouteSamples = 0
        }

        if silenced && consecutiveOnRouteSamples >= Self.onTrackConfirmSamples && !onTrackAnnounced {
            events.append(.onTrack)
            silenced = false
            onTrackAnnounced = true
            offRouteEpisodeCount = 0
        }

        if offRouteRose && offRouteEpisodeCount > Self.repeatOffTrackSilenceThreshold && !silenced {
            events.append(.repeatedOffTrackSilence)
            silenced = true
            onTrackAnnounced = false
        } else if offRouteRose && !silenced {
            events.append(.offTrack)
        }

        let reroutingRose = !s.prevRerouting && snapshot.rerouting
        if reroutingRose && !silenced {
            events.append(.rerouting)
        }

        if !silenced && !snapshot.offRoute {
            var announced50m = s.announced50m
            var announced10m = s.announced10m
            var announcedNextTurnAfter = s.announcedNextTurnAfter

            let upcoming = snapshot.maneuvers.first { $0.distanceFromStartM - snapshot.progressDistanceM >= 0 }
            let upcomingDistance = upcoming.map { $0.distanceFromStartM - snapshot.progressDistanceM }

            if let m = upcoming, let d = upcomingDistance,
               d <= Self.approach50M, d > Self.approach10M, !announced50m.contains(m.id) {
                events.append(.turn50m(m.kind))
                announced50m.insert(m.id)
            }
            if let m = upcoming, let d = upcomingDistance,
               d <= Self.approach10M, !announced10m.contains(m.id) {
                events.append(.turn10m(m.kind))
                announced10m.insert(m.id)
            }

            let lastPassed = snapshot.maneuvers
                .filter { snapshot.progressDistanceM - $0.distanceFromStartM >= Self.passedTurnM }
                .max { $0.distanceFromStartM < $1.distanceFromStartM }
            if let lastPassed = lastPassed, !announcedNextTurnAfter.contains(lastPassed.id) {
                let indexOfLast = snapshot.maneuvers.firstIndex { $0.id == lastPassed.id } ?? -1
                let nextAfter = (indexOfLast >= 0 && indexOfLast + 1 < snapshot.maneuvers.count)
                    ? snapshot.maneuvers[indexOfLast + 1] : nil
                if let nextAfter = nextAfter {
                    events.append(.nextTurnInAbout(
                        turnKind: nextAfter.kind,
                        distanceM: nextAfter.distanceFromStartM - snapshot.progressDistanceM
                    ))
                    announcedNextTurnAfter.insert(lastPassed.id)
                } else if !s.approachingDestinationAnnounced {
                    events.append(.arrivingInM(
                        distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM
                    ))
                    announcedNextTurnAfter.insert(lastPassed.id)
                    s.approachingDestinationAnnounced = true
                }
            }

            s.announced50m = announced50m
            s.announced10m = announced10m
            s.announcedNextTurnAfter = announcedNextTurnAfter
        }

        if snapshot.arrived && !s.arrivedAnnounced {
            events.append(.arrived)
            s.arrivedAnnounced = true
        }

        s.prevOffRoute = snapshot.offRoute
        s.prevRerouting = snapshot.rerouting
        s.offRouteEpisodeCount = offRouteEpisodeCount
        s.silenced = silenced
        s.consecutiveOnRouteSamples = consecutiveOnRouteSamples
        s.onTrackAnnounced = onTrackAnnounced

        return Result(events: events, nextState: s)
    }

    /// Locale-agnostic structured cue: a catalog key + ICU placeholder
    /// values. Wiring layers feed this to `T.string(key, args)` against
    /// the active locale; parity tests render via `T.stringIn(.en, ...)`.
    struct CueMessage: Equatable {
        let key: String
        let args: [String: String]
        let numericArgs: [String: Double]

        /// Convert to the `[String: MessageValue]` map consumed by `T.string`.
        var values: [String: MessageValue] {
            var out: [String: MessageValue] = [:]
            for (k, v) in args { out[k] = .string(v) }
            for (k, v) in numericArgs { out[k] = .number(v) }
            return out
        }
    }

    /// Map a `CueEvent` to its (key, values) tuple. `distanceMode`
    /// chooses metric vs imperial for spoken distance values.
    static func cueMessage(_ event: CueEvent, distanceMode: DistanceMode = .metric) -> CueMessage {
        switch event {
        case .routeStarted:
            return CueMessage(key: "cue.routeStarted", args: [:], numericArgs: [:])
        case .turn50m(let k):
            let pair = distanceCueValues(50, mode: distanceMode)
            return CueMessage(
                key: "cue.turn50m.\(maneuverSlug(k))",
                args: ["distanceUnit": pair.unit],
                numericArgs: ["distance": pair.distance]
            )
        case .turn10m(let k):
            return CueMessage(key: "cue.turn10m.\(maneuverSlug(k))", args: [:], numericArgs: [:])
        case .nextTurnInAbout(let k, let d):
            let pair = distanceCueValues(d, mode: distanceMode)
            return CueMessage(
                key: "cue.nextTurnInAbout.\(nextTurnDirection(k))",
                args: ["distanceUnit": pair.unit],
                numericArgs: ["distance": pair.distance]
            )
        case .arrivingInM(let d):
            let pair = distanceCueValues(d, mode: distanceMode)
            return CueMessage(
                key: "cue.arrivingInM",
                args: ["distanceUnit": pair.unit],
                numericArgs: ["distance": pair.distance]
            )
        case .arrived:
            return CueMessage(key: "cue.arrived", args: [:], numericArgs: [:])
        case .offTrack, .repeatedOffTrackSilence:
            return CueMessage(key: "cue.offTrack", args: [:], numericArgs: [:])
        case .rerouting:
            return CueMessage(key: "cue.rerouting", args: [:], numericArgs: [:])
        case .onTrack:
            return CueMessage(key: "cue.onTrack", args: [:], numericArgs: [:])
        }
    }

    /// Legacy English formatter — kept as the exact-byte path that
    /// existing tests assert against. New call sites should go through
    /// `cueMessage(_:)` + `T.string(_:_:)` instead.
    static func format(_ event: CueEvent) -> String {
        let msg = cueMessage(event, distanceMode: .metric)
        return T.stringIn(.en, msg.key, msg.values)
    }

    private static func maneuverSlug(_ k: ManeuverKind) -> String {
        switch k {
        case .left: return "left"
        case .right: return "right"
        case .keepLeft: return "keepLeft"
        case .keepRight: return "keepRight"
        case .exitLeft: return "exitLeft"
        case .exitRight: return "exitRight"
        case .uturn: return "uturn"
        case .generic: return "generic"
        }
    }

    /// Collapse 8 maneuver kinds into the 4 directions the
    /// `cue.nextTurnInAbout.*` catalog supports.
    private static func nextTurnDirection(_ k: ManeuverKind) -> String {
        switch k {
        case .left, .keepLeft, .exitLeft: return "left"
        case .right, .keepRight, .exitRight: return "right"
        case .uturn: return "uturn"
        case .generic: return "generic"
        }
    }

    private static func distanceCueValues(_ meters: Double, mode: DistanceMode) -> (distance: Double, unit: String) {
        switch mode {
        case .imperial:
            let ft = Double(DistanceFormatter.roundTo10(meters * 3.280839895))
            return (ft, "feet")
        case .metric:
            let m = Double(DistanceFormatter.roundTo10(meters))
            return (m, "meters")
        }
    }
}
