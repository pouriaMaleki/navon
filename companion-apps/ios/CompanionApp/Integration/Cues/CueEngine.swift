import Foundation

enum ManeuverKind {
    case left, right
    case slightLeft, slightRight
    case exitLeft, exitRight
    case uturn
    case roundabout, merge, ramp
    case generic
}

struct CueManeuver: Equatable {
    let id: String
    let kind: ManeuverKind
    let distanceFromStartM: Double

    init(id: String, kind: ManeuverKind, distanceFromStartM: Double) {
        self.id = id
        self.kind = kind
        self.distanceFromStartM = distanceFromStartM
    }
}

struct CueSnapshot {
    let routeId: String?
    let pairedWithDevice: Bool
    let progressDistanceM: Double
    let maneuvers: [CueManeuver]
    let offRoute: Bool
    let rerouting: Bool
    let arrived: Bool
    let distanceFromRouteM: Double
    let routeTotalDistanceM: Double
}

enum CueEvent: Equatable {
    case turn50m(ManeuverKind, distanceM: Double, followUpKind: ManeuverKind? = nil)
    case turn10m(ManeuverKind, followUpKind: ManeuverKind? = nil)
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
    var reroutingEpisodeCount: Int = 0
    var offRouteTickCount: Int = 0
}

enum CueEngine {
    private static let approach50M = 50.0
    private static let approach10M = 15.0
    private static let skip50mBelowDistanceM = 100.0
    private static let passedTurnM = 10.0
    private static let onTrackConfirmSamples = 5
    private static let onTrackCorridorM = 22.0
    private static let repeatOffTrackSilenceThreshold = 2
    private static let offRouteHysteresisTicks = 3
    private static let offRouteImmediateDistanceM = 50.0
    private static let reroutingCueCap = 2
    static let backToBackThresholdM = 30.0
    private static let closeToDestinationM = 30.0

    struct Result {
        let events: [CueEvent]
        let nextState: CueEngineState
    }

    static func tick(snapshot: CueSnapshot, state: CueEngineState) -> Result {
        if snapshot.pairedWithDevice {
            return Result(events: [], nextState: state)
        }
        var s = resetStateIfRouteChanged(snapshot, state)
        var events: [CueEvent] = []

        events += announceRouteStart(snapshot, &s)
        events += detectOffRoute(snapshot, &s)
        events += detectRerouting(snapshot, &s)
        events += announceManeuvers(snapshot, &s)
        events += detectArrival(snapshot, &s)

        s.prevOffRoute = snapshot.offRoute
        s.prevRerouting = snapshot.rerouting
        return Result(events: events, nextState: s)
    }

    private static func resetStateIfRouteChanged(_ snapshot: CueSnapshot, _ state: CueEngineState) -> CueEngineState {
        guard snapshot.routeId != state.lastRouteId else { return state }
        var s = CueEngineState(lastRouteId: snapshot.routeId)
        s.reroutingEpisodeCount = state.reroutingEpisodeCount
        return s
    }

    private static func announceRouteStart(_ snapshot: CueSnapshot, _ s: inout CueEngineState) -> [CueEvent] {
        guard snapshot.routeId != nil, !s.routeStartedAnnounced else { return [] }
        s.routeStartedAnnounced = true
        guard let firstM = snapshot.maneuvers.first(where: { $0.distanceFromStartM - snapshot.progressDistanceM >= 0 })
        else { return [] }

        let distanceM = firstM.distanceFromStartM - snapshot.progressDistanceM

        if distanceM > approach50M {
            return announceRouteStartFar(firstM, distanceM, snapshot, &s)
        }
        return announceRouteStartImminent(firstM, snapshot, &s)
    }

    private static func announceRouteStartFar(
        _ m: CueManeuver, _ distanceM: Double, _ snapshot: CueSnapshot, _ s: inout CueEngineState
    ) -> [CueEvent] {
        let idx = snapshot.maneuvers.firstIndex { $0.id == m.id } ?? -1
        let isLastManeuver = idx == snapshot.maneuvers.count - 1
        let distToEnd = snapshot.routeTotalDistanceM - m.distanceFromStartM

        if isLastManeuver && distToEnd < closeToDestinationM {
            if !s.approachingDestinationAnnounced {
                s.approachingDestinationAnnounced = true
                s.announced50m.insert(m.id)
                s.announced10m.insert(m.id)
                return [.arrivingInM(distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM)]
            }
            return []
        }

        if distanceM < skip50mBelowDistanceM {
            s.announced50m.insert(m.id)
        }
        return [.nextTurnInAbout(turnKind: m.kind, distanceM: distanceM)]
    }

    private static func announceRouteStartImminent(
        _ m: CueManeuver, _ snapshot: CueSnapshot, _ s: inout CueEngineState
    ) -> [CueEvent] {
        let upcomingIdx = snapshot.maneuvers.firstIndex { $0.id == m.id } ?? -1
        let follow = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
            ? snapshot.maneuvers[upcomingIdx + 1] : nil
        let gap = follow.map { $0.distanceFromStartM - m.distanceFromStartM } ?? .infinity

        if let follow = follow, gap <= backToBackThresholdM {
            s.announced50m.insert(m.id)
            s.announced50m.insert(follow.id)
            s.announced10m.insert(m.id)
            s.announced10m.insert(follow.id)
            s.announcedNextTurnAfter.insert(m.id)
            return [.turn50m(m.kind, distanceM: m.distanceFromStartM - snapshot.progressDistanceM, followUpKind: follow.kind)]
        }
        s.announced50m.insert(m.id)
        return []
    }

    private static func detectOffRoute(_ snapshot: CueSnapshot, _ s: inout CueEngineState) -> [CueEvent] {
        var events: [CueEvent] = []

        s.offRouteTickCount = snapshot.offRoute ? s.offRouteTickCount + 1 : 0

        let immediate = snapshot.offRoute
            && snapshot.distanceFromRouteM > offRouteImmediateDistanceM
            && s.offRouteTickCount == 1
        let hysteresis = snapshot.offRoute && s.offRouteTickCount == offRouteHysteresisTicks
        let offTrackFired = immediate || hysteresis

        if offTrackFired { s.offRouteEpisodeCount += 1 }

        if !snapshot.offRoute && snapshot.distanceFromRouteM < onTrackCorridorM {
            s.consecutiveOnRouteSamples += 1
        } else {
            s.consecutiveOnRouteSamples = 0
        }

        if s.silenced && s.consecutiveOnRouteSamples >= onTrackConfirmSamples && !s.onTrackAnnounced {
            events.append(.onTrack)
            s.silenced = false
            s.onTrackAnnounced = true
            s.offRouteEpisodeCount = 0
        }
        if s.consecutiveOnRouteSamples >= onTrackConfirmSamples {
            s.reroutingEpisodeCount = 0
        }

        if offTrackFired && s.offRouteEpisodeCount > repeatOffTrackSilenceThreshold && !s.silenced {
            events.append(.repeatedOffTrackSilence)
            s.silenced = true
            s.onTrackAnnounced = false
        } else if offTrackFired && !s.silenced {
            events.append(.offTrack)
        }

        return events
    }

    private static func detectRerouting(_ snapshot: CueSnapshot, _ s: inout CueEngineState) -> [CueEvent] {
        let rose = !s.prevRerouting && snapshot.rerouting
        guard rose else { return [] }
        s.reroutingEpisodeCount += 1
        guard !s.silenced && s.reroutingEpisodeCount <= reroutingCueCap else { return [] }
        return [.rerouting]
    }

    private static func announceManeuvers(_ snapshot: CueSnapshot, _ s: inout CueEngineState) -> [CueEvent] {
        guard !s.silenced && !snapshot.offRoute && !snapshot.rerouting && !snapshot.arrived else { return [] }

        var events: [CueEvent] = []
        var announced50m = s.announced50m
        var announced10m = s.announced10m
        var announcedNextTurnAfter = s.announcedNextTurnAfter
        let upcoming = snapshot.maneuvers.first { $0.distanceFromStartM - snapshot.progressDistanceM >= 0 }
        let upcomingDistance = upcoming.map { $0.distanceFromStartM - snapshot.progressDistanceM }

        // After-passing block — must run before 50m check so it can pre-latch announced50m.
        events += announceAfterPassing(snapshot, upcoming?.id, &announced50m, &announced10m, &announcedNextTurnAfter, &s)

        // 50 m approach.
        if let m = upcoming, let d = upcomingDistance,
           d <= approach50M, d > approach10M, !announced50m.contains(m.id) {
            let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == m.id }) ?? -1
            let followUp = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                ? snapshot.maneuvers[upcomingIdx + 1] : nil
            let gapToFollowUp = followUp.map { $0.distanceFromStartM - m.distanceFromStartM } ?? .infinity
            if let followUp = followUp, gapToFollowUp <= backToBackThresholdM {
                events.append(.turn50m(m.kind, distanceM: d, followUpKind: followUp.kind))
                announced50m.insert(m.id)
                announced50m.insert(followUp.id)
                announcedNextTurnAfter.insert(m.id)
            } else {
                events.append(.turn50m(m.kind, distanceM: d, followUpKind: nil))
                announced50m.insert(m.id)
            }
        }

        // 10 m approach.
        if let m = upcoming, let d = upcomingDistance,
           d <= approach10M, !announced10m.contains(m.id) {
            let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == m.id }) ?? -1
            let followUp = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                ? snapshot.maneuvers[upcomingIdx + 1] : nil
            let gapToFollowUp = followUp.map { $0.distanceFromStartM - m.distanceFromStartM } ?? .infinity
            if let followUp = followUp, gapToFollowUp <= backToBackThresholdM {
                events.append(.turn10m(m.kind, followUpKind: followUp.kind))
                announced10m.insert(m.id)
                announced10m.insert(followUp.id)
                announced50m.insert(followUp.id)
                announcedNextTurnAfter.insert(m.id)
            } else {
                events.append(.turn10m(m.kind, followUpKind: nil))
                announced10m.insert(m.id)
            }
        }

        s.announced50m = announced50m
        s.announced10m = announced10m
        s.announcedNextTurnAfter = announcedNextTurnAfter
        return events
    }

    private static func announceAfterPassing(
        _ snapshot: CueSnapshot,
        _ upcomingId: String?,
        _ announced50m: inout Set<String>,
        _ announced10m: inout Set<String>,
        _ announcedNextTurnAfter: inout Set<String>,
        _ s: inout CueEngineState
    ) -> [CueEvent] {
        let lastPassed = snapshot.maneuvers
            .filter { snapshot.progressDistanceM - $0.distanceFromStartM >= passedTurnM }
            .max { $0.distanceFromStartM < $1.distanceFromStartM }
        guard let lastPassed = lastPassed, !announcedNextTurnAfter.contains(lastPassed.id) else { return [] }

        let indexOfLast = snapshot.maneuvers.firstIndex { $0.id == lastPassed.id } ?? -1
        let nextAfter = (indexOfLast >= 0 && indexOfLast + 1 < snapshot.maneuvers.count)
            ? snapshot.maneuvers[indexOfLast + 1] : nil

        if let nextAfter = nextAfter {
            return announceTurnAfterPassing(lastPassed, nextAfter, indexOfLast, snapshot,
                                            &announced50m, &announced10m, &announcedNextTurnAfter, &s)
        }
        return announceDestinationAfterLastManeuver(snapshot, lastPassed, &announcedNextTurnAfter, &s)
    }

    private static func announceTurnAfterPassing(
        _ lastPassed: CueManeuver,
        _ nextAfter: CueManeuver,
        _ indexOfLast: Int,
        _ snapshot: CueSnapshot,
        _ announced50m: inout Set<String>,
        _ announced10m: inout Set<String>,
        _ announcedNextTurnAfter: inout Set<String>,
        _ s: inout CueEngineState
    ) -> [CueEvent] {
        let distanceToNext = nextAfter.distanceFromStartM - snapshot.progressDistanceM
        let isLastManeuver = indexOfLast + 1 == snapshot.maneuvers.count - 1
        let distNextToEnd = snapshot.routeTotalDistanceM - nextAfter.distanceFromStartM

        if distanceToNext <= 0 {
            announcedNextTurnAfter.insert(lastPassed.id)
            if isLastManeuver && !s.approachingDestinationAnnounced && !snapshot.arrived {
                s.approachingDestinationAnnounced = true
                return [.arrivingInM(distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM)]
            }
            return []
        }

        if isLastManeuver && distNextToEnd < closeToDestinationM {
            if !s.approachingDestinationAnnounced && !snapshot.arrived {
                s.approachingDestinationAnnounced = true
                announcedNextTurnAfter.insert(lastPassed.id)
                announced50m.insert(nextAfter.id)
                announced10m.insert(nextAfter.id)
                return [.arrivingInM(distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM)]
            }
            return []
        }

        if distanceToNext > approach50M {
            if distanceToNext < skip50mBelowDistanceM {
                announced50m.insert(nextAfter.id)
            }
            announcedNextTurnAfter.insert(lastPassed.id)
            return [.nextTurnInAbout(turnKind: nextAfter.kind, distanceM: distanceToNext)]
        }

        announcedNextTurnAfter.insert(lastPassed.id)
        return []
    }

    private static func announceDestinationAfterLastManeuver(
        _ snapshot: CueSnapshot,
        _ lastPassed: CueManeuver,
        _ announcedNextTurnAfter: inout Set<String>,
        _ s: inout CueEngineState
    ) -> [CueEvent] {
        guard !s.approachingDestinationAnnounced && !snapshot.arrived else { return [] }
        s.approachingDestinationAnnounced = true
        announcedNextTurnAfter.insert(lastPassed.id)
        return [.arrivingInM(distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM)]
    }

    private static func detectArrival(_ snapshot: CueSnapshot, _ s: inout CueEngineState) -> [CueEvent] {
        guard snapshot.arrived, !s.arrivedAnnounced else { return [] }
        s.arrivedAnnounced = true
        return [.arrived]
    }

    struct CueMessage: Equatable {
        let key: String
        let args: [String: String]
        let numericArgs: [String: Double]

        var values: [String: MessageValue] {
            var out: [String: MessageValue] = [:]
            for (k, v) in args { out[k] = .string(v) }
            for (k, v) in numericArgs { out[k] = .number(v) }
            return out
        }
    }

    static func cueMessage(_ event: CueEvent, distanceMode: DistanceMode = .metric) -> CueMessage {
        switch event {
        case .turn50m(let k, let distanceM, let followUp):
            let pair = distanceCueValues(distanceM, mode: distanceMode)
            if let followUp = followUp {
                return CueMessage(
                    key: "cue.turn50mCombined",
                    args: ["distanceUnit": pair.unit, "first": maneuverSlug(k), "second": maneuverSlug(followUp)],
                    numericArgs: ["distance": pair.distance]
                )
            }
            return CueMessage(
                key: "cue.turn50m.\(maneuverSlug(k))",
                args: ["distanceUnit": pair.unit],
                numericArgs: ["distance": pair.distance]
            )
        case .turn10m(let k, let followUp):
            if let followUp = followUp {
                return CueMessage(
                    key: "cue.turn10mCombined",
                    args: ["first": maneuverSlug(k), "second": maneuverSlug(followUp)],
                    numericArgs: [:]
                )
            }
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

    static func format(_ event: CueEvent) -> String {
        let msg = cueMessage(event, distanceMode: .metric)
        return T.stringIn(.en, msg.key, msg.values)
    }

    private static func maneuverSlug(_ k: ManeuverKind) -> String {
        switch k {
        case .left: return "left"
        case .right: return "right"
        case .exitLeft: return "exitLeft"
        case .exitRight: return "exitRight"
        case .uturn: return "uturn"
        case .roundabout: return "roundabout"
        case .merge: return "merge"
        case .ramp: return "ramp"
        case .generic: return "generic"
        case .slightLeft: return "slightLeft"
        case .slightRight: return "slightRight"
        }
    }

    private static func nextTurnDirection(_ k: ManeuverKind) -> String {
        switch k {
        case .left, .exitLeft: return "left"
        case .right, .exitRight: return "right"
        case .uturn: return "uturn"
        case .roundabout: return "roundabout"
        case .merge: return "merge"
        case .ramp: return "ramp"
        case .generic: return "generic"
        case .slightLeft: return "slightLeft"
        case .slightRight: return "slightRight"
        }
    }

    private static func distanceCueValues(_ meters: Double, mode: DistanceMode) -> (distance: Double, unit: String) {
        DistanceFormatter.cueDistanceAndUnit(meters: meters, mode: mode)
    }
}
