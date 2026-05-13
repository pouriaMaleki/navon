import Foundation

/// Audio-cue trigger engine. Pure data-in / data-out — consumed by the
/// wiring layer which assembles a `CueSnapshot` each guidance tick and
/// routes resulting events to a `SpeechPort`.
///
/// Spec: docs/ux-specs.md lines 133-143.

enum ManeuverKind {
    case left, right
    case bearLeft, bearRight
    case exitLeft, exitRight
    case uturn
    case roundabout, merge, ramp
    case generic
}

struct CueManeuver: Equatable {
    let id: String
    let kind: ManeuverKind
    let distanceFromStartM: Double
    let isMinorKeep: Bool

    init(id: String, kind: ManeuverKind, distanceFromStartM: Double, isMinorKeep: Bool = false) {
        self.id = id
        self.kind = kind
        self.distanceFromStartM = distanceFromStartM
        self.isMinorKeep = isMinorKeep
    }
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
    case turn50m(ManeuverKind, distanceM: Double, followUpKind: ManeuverKind? = nil)
    /// Immediate-action 10 m cue. `followUpKind` is set when the next
    /// maneuver is within `backToBackThresholdM` of this one — covers the
    /// sparse-GPS / fast-cycling case where the 50 m combined cue was
    /// missed because the first in-range tick already landed inside 15 m
    /// of M1. Without this fold, the rider hears only "turn <first>" with
    /// no mention of the immediately-following turn.
    case turn10m(ManeuverKind, followUpKind: ManeuverKind? = nil)
    case bearRange(turnKind: ManeuverKind, distanceM: Double)
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
    /// Number of rerouting cues that have fired (counts rising edges).
    /// Persists across route id changes — every successful reroute issues a
    /// new route id, so resetting on route id would defeat the cue cap.
    /// Reset only when the rider is confirmed on-track for
    /// `onTrackConfirmSamples` consecutive ticks.
    var reroutingEpisodeCount: Int = 0
}

enum CueEngine {
    private static let approach50M = 50.0
    private static let approach10M = 15.0
    /// When the next turn is closer than this at the time of nextTurnInAbout,
    /// pre-latch turn50m so it never fires. The rider has already been told
    /// the turn is near; a redundant "in 50 m" before they can react is jarring.
    private static let skip50mBelowDistanceM = 100.0
    private static let passedTurnM = 10.0
    private static let onTrackConfirmSamples = 5
    private static let onTrackCorridorM = 22.0
    private static let repeatOffTrackSilenceThreshold = 2
    /// Cap on rerouting audio cues per "off-route session". After this many
    /// fires, stay silent until the rider is confirmed on-track.
    private static let reroutingCueCap = 2
    /// Two maneuvers separated by less than this fold into a single
    /// "turn X then quickly Y" cue. Mirrors web's BACK_TO_BACK_THRESHOLD_M.
    /// 30 m matches the spec phrase "then quickly" — at cycling speeds
    /// that's ~4-7 s apart, the only window where coalescing two turns
    /// into one cue actually feels natural. 50 m / 80 m both let
    /// genuinely separate maneuvers ride along on a combined cue, which
    /// the rider then misperceives as the routing engine inventing
    /// turns that aren't really there.
    private static let backToBackThresholdM = 30.0
    /// If the last cue maneuver sits within this distance of the route
    /// end, approaching it is indistinguishable from arriving: substitute
    /// `arrivingInM` for any `nextTurnInAbout` or approach cues so the
    /// rider hears "arriving in Xm" rather than a phantom turn command.
    private static let closeToDestinationM = 30.0

    private static func isBearKind(_ k: ManeuverKind) -> Bool {
        k == .bearLeft || k == .bearRight
    }

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
            // Persist rerouting silence across route id changes — every
            // successful reroute issues a new route id, so resetting here
            // would defeat the cue cap.
            s = CueEngineState(lastRouteId: snapshot.routeId)
            s.reroutingEpisodeCount = state.reroutingEpisodeCount
        } else {
            s = state
        }

        var events: [CueEvent] = []

        // First-tick announcement (replaces "Route started"). User-feedback:
        // "Route started" was useless — replace with the actual next-turn
        // announcement so the first sound the rider hears is what they need
        // to plan for.
        //
        // Three sub-cases on this tick when the route just started:
        //   A) First turn is FAR (> 50 m): emit `nextTurnInAbout` as an
        //      orientation cue ("Next turn left in about 200 meters").
        //   B) First turn is IMMINENT and stands alone (no back-to-back
        //      follow-up within ~30 m): SKIP every announce; let the
        //      10 m approach block speak the single "Turn left" cue
        //      when the rider actually reaches it. User feedback: a
        //      route starting 15 m from a turn used to fire next-turn
        //      + 50 m + 10 m back-to-back — three cues for one turn,
        //      with disagreeing distances.
        //   C) First turn is IMMINENT and has a back-to-back companion
        //      within ~30 m: skip the orientation cue, let the 50 m
        //      block emit the combined "in X meters turn left then
        //      quickly right" cue with the ACTUAL distance. That's
        //      the only way to warn the rider about TWO close turns
        //      in one breath, so it stays.
        if snapshot.routeId != nil, !s.routeStartedAnnounced {
            if let firstNonDepart = snapshot.maneuvers.first(where: {
                $0.distanceFromStartM - snapshot.progressDistanceM >= 0
            }) {
                let distanceM = firstNonDepart.distanceFromStartM - snapshot.progressDistanceM
                if distanceM > Self.approach50M {
                    // Case A — orientation cue.
                    // Bug 1: if firstNonDepart is the last cue maneuver AND very
                    // close to the route end, announce "arriving" instead of a
                    // phantom turn direction.
                    let firstIdx = snapshot.maneuvers.firstIndex { $0.id == firstNonDepart.id } ?? -1
                    let isLastManeuver = firstIdx == snapshot.maneuvers.count - 1
                    let distToEnd = snapshot.routeTotalDistanceM - firstNonDepart.distanceFromStartM
                    if isLastManeuver && distToEnd < Self.closeToDestinationM {
                        if !s.approachingDestinationAnnounced {
                            events.append(.arrivingInM(distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM))
                            s.approachingDestinationAnnounced = true
                            s.announced50m.insert(firstNonDepart.id)
                            s.announced10m.insert(firstNonDepart.id)
                        }
                    } else if !firstNonDepart.isMinorKeep {
                        events.append(.nextTurnInAbout(turnKind: firstNonDepart.kind, distanceM: distanceM))
                        // Pre-latch turn50m when the first turn is already close: the rider
                        // has the orientation cue; a redundant "in 50 m" a few seconds later
                        // would be jarring before they can even react to the first.
                        if distanceM < Self.skip50mBelowDistanceM {
                            s.announced50m.insert(firstNonDepart.id)
                        }
                    }
                } else {
                    // Case B vs. C — peek at the follow-up gap.
                    let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == firstNonDepart.id }) ?? -1
                    let follow = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                        ? snapshot.maneuvers[upcomingIdx + 1] : nil
                    let gap = follow.map { $0.distanceFromStartM - firstNonDepart.distanceFromStartM } ?? .infinity
                    if let follow = follow, gap <= Self.backToBackThresholdM {
                        // Case C: emit the combined cue here directly
                        // with the actual distance. The regular 50 m
                        // block downstream gates on `d > approach10M`
                        // (15 m) and would skip routes starting < 15 m
                        // before a back-to-back pair, leaving the rider
                        // with only `turn10m(first)` and no warning
                        // about the second turn.
                        // Bear kinds are excluded from the combined cue —
                        // their bearRange segment-entry cue handles the
                        // announcement instead.
                        if !firstNonDepart.isMinorKeep && !Self.isBearKind(firstNonDepart.kind) {
                            events.append(.turn50m(firstNonDepart.kind, distanceM: distanceM, followUpKind: follow.kind))
                        }
                        s.announced50m.insert(firstNonDepart.id)
                        s.announced50m.insert(follow.id)
                        s.announced10m.insert(firstNonDepart.id)
                        s.announced10m.insert(follow.id)
                        s.announcedNextTurnAfter.insert(firstNonDepart.id)
                    } else {
                        // Case B: pre-latch the 50 m cue so only the
                        // 10 m action cue fires for this maneuver.
                        s.announced50m.insert(firstNonDepart.id)
                    }
                }
            }
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

        var reroutingEpisodeCount = s.reroutingEpisodeCount

        if silenced && consecutiveOnRouteSamples >= Self.onTrackConfirmSamples && !onTrackAnnounced {
            events.append(.onTrack)
            silenced = false
            onTrackAnnounced = true
            offRouteEpisodeCount = 0
        }
        // Independent reset of the rerouting cue counter: even without an
        // off-route silence event, sustained on-track confirmation means the
        // rider is back on the route and the next reroute episode (if any)
        // deserves a fresh count.
        if consecutiveOnRouteSamples >= Self.onTrackConfirmSamples {
            reroutingEpisodeCount = 0
        }

        if offRouteRose && offRouteEpisodeCount > Self.repeatOffTrackSilenceThreshold && !silenced {
            events.append(.repeatedOffTrackSilence)
            silenced = true
            onTrackAnnounced = false
        } else if offRouteRose && !silenced {
            events.append(.offTrack)
        }

        // Rerouting rising edge — capped at reroutingCueCap per off-route session.
        let reroutingRose = !s.prevRerouting && snapshot.rerouting
        if reroutingRose { reroutingEpisodeCount += 1 }
        if reroutingRose && !silenced && reroutingEpisodeCount <= Self.reroutingCueCap {
            events.append(.rerouting)
        }

        if !silenced && !snapshot.offRoute {
            var announced50m = s.announced50m
            var announced10m = s.announced10m
            var announcedNextTurnAfter = s.announcedNextTurnAfter
            let upcoming = snapshot.maneuvers.first { $0.distanceFromStartM - snapshot.progressDistanceM >= 0 }
            let upcomingDistance = upcoming.map { $0.distanceFromStartM - snapshot.progressDistanceM }

            // Bug 2 fix: run the "after-passing" block FIRST so it can
            // pre-latch announced50m before the 50m approach check. The old
            // ordering let turn50m fire in the same tick as nextTurnInAbout
            // for the identical maneuver, producing a back-to-back double cue.
            let lastPassed = snapshot.maneuvers
                .filter { snapshot.progressDistanceM - $0.distanceFromStartM >= Self.passedTurnM }
                .max { $0.distanceFromStartM < $1.distanceFromStartM }
            if let lastPassed = lastPassed, !announcedNextTurnAfter.contains(lastPassed.id) {
                let indexOfLast = snapshot.maneuvers.firstIndex { $0.id == lastPassed.id } ?? -1
                let nextAfter = (indexOfLast >= 0 && indexOfLast + 1 < snapshot.maneuvers.count)
                    ? snapshot.maneuvers[indexOfLast + 1] : nil
                if let nextAfter = nextAfter {
                    let distanceToNext = nextAfter.distanceFromStartM - snapshot.progressDistanceM
                    // Bug 1 fix: if nextAfter is the last cue maneuver and sits
                    // within closeToDestinationM of the route end, the rider is
                    // effectively arriving — emit arrivingInM and suppress all
                    // approach cues for that maneuver so the phantom "turn X"
                    // never plays.
                    let isLastManeuver = indexOfLast + 1 == snapshot.maneuvers.count - 1
                    let distNextToEnd = snapshot.routeTotalDistanceM - nextAfter.distanceFromStartM
                    if isLastManeuver && distNextToEnd < Self.closeToDestinationM {
                        // Bug 4: when the rider has already crossed the
                        // arrival radius, the dedicated `arrived` cue at
                        // the bottom of this function is the right thing
                        // to speak — emitting `arrivingInM` here too
                        // produces a same-tick double cue with
                        // disagreeing distances ("Arriving in 5 m" →
                        // "You have arrived").
                        if !s.approachingDestinationAnnounced && !snapshot.arrived {
                            events.append(.arrivingInM(
                                distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM
                            ))
                            s.approachingDestinationAnnounced = true
                            announcedNextTurnAfter.insert(lastPassed.id)
                            announced50m.insert(nextAfter.id)
                            announced10m.insert(nextAfter.id)
                        }
                    } else if !nextAfter.isMinorKeep && !Self.isBearKind(nextAfter.kind) {
                        events.append(.nextTurnInAbout(
                            turnKind: nextAfter.kind,
                            distanceM: distanceToNext
                        ))
                        // Pre-latch turn50m whenever the next turn is close enough
                        // that the rider has already been told about it. Below
                        // skip50mBelowDistanceM the "in X m" cue is redundant.
                        if distanceToNext < Self.skip50mBelowDistanceM {
                            announced50m.insert(nextAfter.id)
                        }
                        announcedNextTurnAfter.insert(lastPassed.id)
                    }
                } else if !s.approachingDestinationAnnounced && !snapshot.arrived {
                    // Bug 4: skip arrivingInM when the rider has already
                    // crossed the arrival radius — the `arrived` cue at
                    // the bottom of this function speaks instead.
                    events.append(.arrivingInM(
                        distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM
                    ))
                    announcedNextTurnAfter.insert(lastPassed.id)
                    s.approachingDestinationAnnounced = true
                }
            }

            if let m = upcoming, let d = upcomingDistance,
               d <= Self.approach50M, d > Self.approach10M, !announced50m.contains(m.id),
               !m.isMinorKeep, !Self.isBearKind(m.kind) {
                let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == m.id }) ?? -1
                let followUp = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                    ? snapshot.maneuvers[upcomingIdx + 1] : nil
                let gapToFollowUp = followUp.map { $0.distanceFromStartM - m.distanceFromStartM } ?? .infinity
                if let followUp = followUp, gapToFollowUp <= Self.backToBackThresholdM {
                    // Carry the rider's actual distance into the cue
                    // instead of letting the catalog hardcode "50 m" —
                    // at route start the rider can be 15 m from the
                    // first maneuver, and "in 50 meters" is jarringly
                    // inaccurate.
                    events.append(.turn50m(m.kind, distanceM: d, followUpKind: followUp.kind))
                    announced50m.insert(m.id)
                    announced50m.insert(followUp.id)
                    announced10m.insert(followUp.id)
                    announcedNextTurnAfter.insert(m.id)
                } else {
                    events.append(.turn50m(m.kind, distanceM: d, followUpKind: nil))
                    announced50m.insert(m.id)
                }
            }
            if let m = upcoming, let d = upcomingDistance,
               d <= Self.approach10M, !announced10m.contains(m.id),
               !m.isMinorKeep, !Self.isBearKind(m.kind) {
                let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == m.id }) ?? -1
                let followUp = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                    ? snapshot.maneuvers[upcomingIdx + 1] : nil
                let gapToFollowUp = followUp.map { $0.distanceFromStartM - m.distanceFromStartM } ?? .infinity
                if let followUp = followUp, gapToFollowUp <= Self.backToBackThresholdM {
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

            // Bear-range cue: for bearLeft/bearRight, fire a single
            // range-hold cue when the rider enters the segment. Latched
            // per maneuver id — one cue total.
            if let m = upcoming, let d = upcomingDistance,
               Self.isBearKind(m.kind),
               d <= Self.approach10M, !announced10m.contains(m.id) {
                let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == m.id }) ?? -1
                let nextManeuver = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                    ? snapshot.maneuvers[upcomingIdx + 1] : nil
                let rawSegmentLengthM = nextManeuver.map { $0.distanceFromStartM - m.distanceFromStartM }
                    ?? (snapshot.routeTotalDistanceM - m.distanceFromStartM)
                let segmentLengthM = min(rawSegmentLengthM, 500.0)
                events.append(.bearRange(turnKind: m.kind, distanceM: segmentLengthM))
                announced10m.insert(m.id)
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
        s.reroutingEpisodeCount = reroutingEpisodeCount

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
        case .turn50m(let k, let distanceM, let followUp):
            // Use the rider's actual distance instead of hardcoding 50.
            // The 50 m approach window can be entered with d much
            // smaller (route starting close to a turn), and
            // "In 50 meters turn left" while actually 15 m away is
            // misleading.
            let pair = distanceCueValues(distanceM, mode: distanceMode)
            if let followUp = followUp {
                return CueMessage(
                    key: "cue.turn50mCombined",
                    args: [
                        "distanceUnit": pair.unit,
                        "first": maneuverSlug(k),
                        "second": maneuverSlug(followUp),
                    ],
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
                    args: [
                        "first": maneuverSlug(k),
                        "second": maneuverSlug(followUp),
                    ],
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
        case .bearRange(let k, let d):
            let pair = distanceCueValues(d, mode: distanceMode)
            return CueMessage(
                key: "cue.bearRange.\(maneuverSlug(k))",
                args: ["distanceUnit": pair.unit],
                numericArgs: ["distance": pair.distance]
            )
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
        case .exitLeft: return "exitLeft"
        case .exitRight: return "exitRight"
        case .uturn: return "uturn"
        case .roundabout: return "roundabout"
        case .merge: return "merge"
        case .ramp: return "ramp"
        case .generic: return "generic"
        case .bearLeft: return "bearLeft"
        case .bearRight: return "bearRight"
        }
    }

    /// Collapse maneuver kinds into the slugs the `cue.nextTurnInAbout.*`
    /// catalog supports. Exit ramps fold into their parent direction; the
    /// dedicated kinds keep their own slug.
    private static func nextTurnDirection(_ k: ManeuverKind) -> String {
        switch k {
        case .left, .exitLeft: return "left"
        case .right, .exitRight: return "right"
        case .uturn: return "uturn"
        case .roundabout: return "roundabout"
        case .merge: return "merge"
        case .ramp: return "ramp"
        case .generic: return "generic"
        case .bearLeft: return "bearLeft"
        case .bearRight: return "bearRight"
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
