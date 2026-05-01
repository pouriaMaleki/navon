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
    case turn50m(ManeuverKind, distanceM: Double, followUpKind: ManeuverKind? = nil)
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
    /// Two maneuvers separated by less than this fold into a single
    /// "turn X then quickly Y" cue. Mirrors web's BACK_TO_BACK_THRESHOLD_M.
    /// 30 m matches the spec phrase "then quickly" — at cycling speeds
    /// that's ~4-7 s apart, the only window where coalescing two turns
    /// into one cue actually feels natural. 50 m / 80 m both let
    /// genuinely separate maneuvers ride along on a combined cue, which
    /// the rider then misperceives as the routing engine inventing
    /// turns that aren't really there.
    private static let backToBackThresholdM = 30.0

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
                    // Case A.
                    events.append(.nextTurnInAbout(turnKind: firstNonDepart.kind, distanceM: distanceM))
                } else {
                    // Case B vs. C — peek at the follow-up gap.
                    let upcomingIdx = snapshot.maneuvers.firstIndex(where: { $0.id == firstNonDepart.id }) ?? -1
                    let follow = (upcomingIdx >= 0 && upcomingIdx + 1 < snapshot.maneuvers.count)
                        ? snapshot.maneuvers[upcomingIdx + 1] : nil
                    let gap = follow.map { $0.distanceFromStartM - firstNonDepart.distanceFromStartM } ?? .infinity
                    if follow == nil || gap > Self.backToBackThresholdM {
                        // Case B: pre-latch the 50 m cue so only the
                        // 10 m action cue fires for this maneuver.
                        s.announced50m.insert(firstNonDepart.id)
                    }
                    // Case C: do nothing here — the 50 m block in this
                    // same tick will detect the back-to-back pair and
                    // emit the combined cue with actual distance.
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
                    let distanceToNext = nextAfter.distanceFromStartM - snapshot.progressDistanceM
                    events.append(.nextTurnInAbout(
                        turnKind: nextAfter.kind,
                        distanceM: distanceToNext
                    ))
                    announcedNextTurnAfter.insert(lastPassed.id)
                    // If the next maneuver is already within the 50 m
                    // approach window when we announce it, suppress the
                    // 50 m cue for it — the rider was just told. Without
                    // this they'd hear "Next turn left in about 30 m"
                    // and a couple seconds later "In 50 m turn left",
                    // which is both repetitive and factually wrong.
                    // Lets only the 10 m action cue fire later.
                    if distanceToNext <= Self.approach50M {
                        announced50m.insert(nextAfter.id)
                    }
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
