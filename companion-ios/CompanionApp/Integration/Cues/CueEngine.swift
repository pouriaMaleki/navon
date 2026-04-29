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

    static func format(_ event: CueEvent) -> String {
        switch event {
        case .routeStarted: return "Route started"
        case .turn50m(let k): return "In 50 meters, \(turnVerb(k))"
        case .turn10m(let k): return turnImperative(k)
        case .nextTurnInAbout(let k, let d):
            return "Next turn \(turnDirectionWord(k)) in about \(roundTo10(d)) meters"
        case .arrivingInM(let d):
            return "Arriving at your destination in \(roundTo10(d)) meters"
        case .arrived: return "You have arrived at your destination"
        case .offTrack, .repeatedOffTrackSilence: return "Off track"
        case .rerouting: return "Rerouting"
        case .onTrack: return "On track"
        }
    }

    private static func turnVerb(_ k: ManeuverKind) -> String {
        switch k {
        case .left: return "turn left"
        case .right: return "turn right"
        case .keepLeft: return "keep left"
        case .keepRight: return "keep right"
        case .exitLeft: return "take the left exit"
        case .exitRight: return "take the right exit"
        case .uturn: return "make a U-turn"
        case .generic: return "follow the route"
        }
    }

    private static func turnImperative(_ k: ManeuverKind) -> String {
        switch k {
        case .left: return "Turn left"
        case .right: return "Turn right"
        case .keepLeft: return "Keep left"
        case .keepRight: return "Keep right"
        case .exitLeft: return "Take the left exit"
        case .exitRight: return "Take the right exit"
        case .uturn: return "Make a U-turn"
        case .generic: return "Follow the route"
        }
    }

    private static func turnDirectionWord(_ k: ManeuverKind) -> String {
        switch k {
        case .left, .keepLeft, .exitLeft: return "left"
        case .right, .keepRight, .exitRight: return "right"
        case .uturn: return "u-turn"
        case .generic: return "ahead"
        }
    }

    private static func roundTo10(_ meters: Double) -> Int {
        Int((meters / 10.0).rounded()) * 10
    }
}
