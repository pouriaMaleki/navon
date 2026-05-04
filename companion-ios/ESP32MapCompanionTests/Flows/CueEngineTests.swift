import XCTest
@testable import ESP32MapCompanion

final class CueEngineTests: XCTestCase {

    private func mLeft(_ id: String, _ distance: Double) -> CueManeuver {
        CueManeuver(id: id, kind: .left, distanceFromStartM: distance)
    }

    private func base(
        routeId: String? = "r1",
        progressDistanceM: Double = 0,
        maneuvers: [CueManeuver]? = nil,
        offRoute: Bool = false,
        rerouting: Bool = false,
        arrived: Bool = false,
        distanceFromRouteM: Double = 0,
        routeTotalDistanceM: Double = 1000,
        pairedWithDevice: Bool = false
    ) -> CueSnapshot {
        CueSnapshot(
            routeId: routeId,
            pairedWithDevice: pairedWithDevice,
            progressDistanceM: progressDistanceM,
            maneuvers: maneuvers ?? [mLeft("m1", 200), mLeft("m2", 400)],
            offRoute: offRoute,
            rerouting: rerouting,
            arrived: arrived,
            distanceFromRouteM: distanceFromRouteM,
            routeTotalDistanceM: routeTotalDistanceM
        )
    }

    func test_doesNotEmitRouteStartedOnFirstTick() {
        // User-feedback: "Route started" was useless padding. Replace with
        // first-tick `nextTurnInAbout` so the rider hears the actual next-
        // turn announcement on Start.
        let r = CueEngine.tick(snapshot: base(), state: CueEngineState())
        let firstTickAnnounce = r.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(firstTickAnnounce, "first tick must announce the next turn with distance")
        if case .nextTurnInAbout(let kind, let distance) = firstTickAnnounce! {
            XCTAssertEqual(kind, .left)
            XCTAssertEqual(distance, 200, accuracy: 0.5)
        }
    }

    func test_doesNotReAnnounceFirstTickAfterBackgroundGap() {
        // Bug we're fixing: after a long backgrounded period the GPS gap
        // produced a "Route started" re-emission once a fix arrived. With
        // route-id checks and no first-tick re-announcement, this can't
        // recur.
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 5), state: s1)
        let again = r2.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNil(again)
    }

    func test_emitsTurn50mWhenCrossingThreshold() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 100), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 155), state: s1)
        let turn50 = r2.events.first { if case .turn50m = $0 { return true } else { return false } }
        XCTAssertNotNil(turn50)
        if case .turn50m(let k, _, _) = turn50! { XCTAssertEqual(k, .left) }
    }

    func test_back2backTurnsCoalesceIntoCombinedCue() {
        let m1 = CueManeuver(id: "m1", kind: .right, distanceFromStartM: 200)
        let m2 = CueManeuver(id: "m2", kind: .left, distanceFromStartM: 230)
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [m1, m2]),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 155, maneuvers: [m1, m2]),
            state: s1
        )
        let combined = r2.events.first {
            if case .turn50m(_, _, let f) = $0 { return f != nil } else { return false }
        }
        XCTAssertNotNil(combined, "back-to-back turns must coalesce into a single turn50m with followUp")
        if case .turn50m(let k, _, let f) = combined! {
            XCTAssertEqual(k, ManeuverKind.right)
            XCTAssertEqual(f, ManeuverKind.left)
        }
    }

    func test_doesNotReEmit50mForSameManeuver() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 155), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 160), state: s1)
        XCTAssertFalse(r2.events.contains { if case .turn50m = $0 { return true } else { return false } })
    }

    func test_emitsTurn10mWhenCrossingThreshold() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 155), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 192), state: s1)
        XCTAssertTrue(r2.events.contains(.turn10m(.left)))
    }

    func test_emitsNextTurnInAboutAfterPassingManeuver() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 200), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 211), state: s1)
        let next = r2.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(next)
        if case .nextTurnInAbout(let kind, let distance) = next! {
            XCTAssertEqual(kind, .left)
            XCTAssertEqual(distance, 189, accuracy: 0.5)
        }
    }

    func test_emitsArrivingInMWhenPastLastManeuver() {
        let snap = base(progressDistanceM: 412, maneuvers: [mLeft("m1", 400)], routeTotalDistanceM: 600)
        let r = CueEngine.tick(snapshot: snap, state: CueEngineState())
        let arr = r.events.first { if case .arrivingInM = $0 { return true } else { return false } }
        XCTAssertNotNil(arr)
        if case .arrivingInM(let d) = arr! {
            XCTAssertEqual(d, 188, accuracy: 0.5)
        }
    }

    func test_emitsArrivedWhenArrivedFlagIsTrue() {
        let r = CueEngine.tick(snapshot: base(arrived: true), state: CueEngineState())
        XCTAssertTrue(r.events.contains(.arrived))
    }

    func test_emitsOffTrackOnFirstEpisode() {
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s1)
        XCTAssertTrue(r2.events.contains(.offTrack))
    }

    func test_emitsReroutingOnRisingEdge() {
        let s1 = CueEngine.tick(snapshot: base(offRoute: true), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(offRoute: true, rerouting: true), state: s1)
        XCTAssertTrue(r2.events.contains(.rerouting))
    }

    func test_afterMoreThanTwoOffRouteEpisodesGoesSilent() {
        var s = CueEngineState()
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s).nextState
        let r3 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s)
        XCTAssertTrue(r3.events.contains(.repeatedOffTrackSilence))
        s = r3.nextState
        let r4 = CueEngine.tick(snapshot: base(progressDistanceM: 155, offRoute: true), state: s)
        XCTAssertEqual(r4.events.count, 0)
    }

    func test_emitsOnTrackAfterFiveConsecutiveOnRouteSamples() {
        var s = CueEngineState(
            lastRouteId: "r1",
            routeStartedAnnounced: true,
            offRouteEpisodeCount: 3,
            prevOffRoute: true,
            silenced: true
        )
        for _ in 0..<4 {
            let r = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s)
            XCTAssertFalse(r.events.contains(.onTrack))
            s = r.nextState
        }
        let r5 = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s)
        XCTAssertTrue(r5.events.contains(.onTrack))
    }

    func test_pairedWithDeviceSuppressesAllCues() {
        let snap = base(
            progressDistanceM: 192,
            offRoute: true,
            pairedWithDevice: true
        )
        let r = CueEngine.tick(snapshot: snap, state: CueEngineState())
        XCTAssertEqual(r.events.count, 0)
    }

    func test_resetsLatchesOnRouteIdChange() {
        // Use progressM=100 so distance-to-first-turn is 100 m — well
        // outside the 50 m approach window, ensuring the first-tick
        // `nextTurnInAbout` block is not suppressed by the imminent-cue
        // skip rule (which exists to prevent route-start double-firing).
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 100), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(routeId: "r2", progressDistanceM: 100), state: s1)
        let firstTick = r2.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(firstTick, "new route id should re-announce next turn on first tick")
    }

    // MARK: - Bug 4: arrived flag suppresses arrivingInM in same tick

    func test_arrivingInM_suppressedWhenArrivedFlagTrueSameTick() {
        // Set up a state where the after-passing block would fire arrivingInM
        // (rider passed the only maneuver, no follow-up). Then on the same tick
        // mark `arrived = true` — only the dedicated `arrived` cue should fire.
        // The double cue ("Arriving at your destination in 5 meters" → "You have
        // arrived") with disagreeing distances has been a long-standing complaint.
        let m1 = mLeft("m1", 400)
        // First tick to consume route-started announcement.
        var s = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [m1], routeTotalDistanceM: 600),
            state: CueEngineState()
        ).nextState
        // Rider crosses arrival radius before the after-passing block has run.
        let r = CueEngine.tick(
            snapshot: base(
                progressDistanceM: 595,
                maneuvers: [m1],
                arrived: true,
                routeTotalDistanceM: 600
            ),
            state: s
        )
        let hasArriving = r.events.contains { if case .arrivingInM = $0 { return true } else { return false } }
        let hasArrived = r.events.contains(.arrived)
        XCTAssertFalse(hasArriving, "arrivingInM must be suppressed when arrived flag is true on the same tick — got \(r.events)")
        XCTAssertTrue(hasArrived, "arrived cue must fire — got \(r.events)")
    }

    // MARK: - Bug 5: back-to-back pair with first turn under 15m at route start

    func test_firstTick_backToBackPair_under15mFromStart_emitsCombinedCue() {
        // Route starts 10m before m1; m2 follows 15m later (within 30m back-to-back
        // threshold). Without the fix, only `turn10m(m1.kind)` fires and the rider
        // never hears about m2. The fix: Case C emits the combined cue with the
        // actual distance even when distance < APPROACH_10_M.
        let m1 = CueManeuver(id: "m1", kind: .right, distanceFromStartM: 10)
        let m2 = CueManeuver(id: "m2", kind: .left, distanceFromStartM: 25)
        let r = CueEngine.tick(
            snapshot: base(progressDistanceM: 0, maneuvers: [m1, m2], routeTotalDistanceM: 1000),
            state: CueEngineState()
        )
        let combined = r.events.first {
            if case .turn50m(_, _, let f) = $0 { return f != nil } else { return false }
        }
        XCTAssertNotNil(combined, "first-tick back-to-back pair under 15m must emit combined turn50m cue — got \(r.events)")
        if case .turn50m(let k, let d, let f) = combined! {
            XCTAssertEqual(k, ManeuverKind.right)
            XCTAssertEqual(f, ManeuverKind.left)
            XCTAssertEqual(d, 10, accuracy: 0.5, "combined cue must carry the actual distance, not 50")
        }
    }

    // MARK: - Roundabout / merge / ramp first-class cues

    func test_formatsRoundaboutPhrases() {
        XCTAssertEqual(CueEngine.format(.turn50m(.roundabout, distanceM: 50)), "In 50 meters, enter the roundabout")
        XCTAssertEqual(CueEngine.format(.turn10m(.roundabout)), "Enter the roundabout")
        XCTAssertEqual(
            CueEngine.format(.nextTurnInAbout(turnKind: .roundabout, distanceM: 200)),
            "Next roundabout in about 200 meters"
        )
    }

    func test_formatsMergePhrases() {
        XCTAssertEqual(CueEngine.format(.turn50m(.merge, distanceM: 50)), "In 50 meters, merge")
        XCTAssertEqual(CueEngine.format(.turn10m(.merge)), "Merge")
        XCTAssertEqual(
            CueEngine.format(.nextTurnInAbout(turnKind: .merge, distanceM: 200)),
            "Next merge in about 200 meters"
        )
    }

    func test_formatsRampPhrases() {
        XCTAssertEqual(CueEngine.format(.turn50m(.ramp, distanceM: 50)), "In 50 meters, take the ramp")
        XCTAssertEqual(CueEngine.format(.turn10m(.ramp)), "Take the ramp")
        XCTAssertEqual(
            CueEngine.format(.nextTurnInAbout(turnKind: .ramp, distanceM: 200)),
            "Next ramp in about 200 meters"
        )
    }

    // MARK: - Bug 1: last maneuver close to destination

    func test_lastManeuver_closeToDestination_emitsArrivingNotNextTurn() {
        // Route: m0 (left) at 500m, m1 (right) at 995m, route ends at 1000m.
        // m1 is only 5m before the endpoint — treating it as a turn cue is
        // misleading; "arriving" is the correct announcement.
        let m0 = CueManeuver(id: "m0", kind: .left, distanceFromStartM: 500)
        let m1 = CueManeuver(id: "m1", kind: .right, distanceFromStartM: 995)
        // Tick once to build up routeStartedAnnounced state (rider far away).
        var s = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [m0, m1], routeTotalDistanceM: 1000),
            state: CueEngineState()
        ).nextState
        // Advance past m0; now m1 is the next upcoming but it's 5m from route end.
        let r = CueEngine.tick(
            snapshot: base(progressDistanceM: 511, maneuvers: [m0, m1], routeTotalDistanceM: 1000),
            state: s
        )
        let hasNextTurn = r.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } }
        let hasArriving = r.events.contains { if case .arrivingInM = $0 { return true } else { return false } }
        XCTAssertFalse(hasNextTurn, "last maneuver within 30m of route end must not emit nextTurnInAbout — got \(r.events)")
        XCTAssertTrue(hasArriving, "last maneuver within 30m of route end must emit arrivingInM — got \(r.events)")
    }

    func test_lastManeuver_closeToDestination_suppressesTurn50m() {
        // Same setup: m1 at 995m of 1000m route. Rider advances into the 50m
        // window (distance = 30m). Must NOT fire turn50m — should have already
        // fired arrivingInM when passing m0.
        let m0 = CueManeuver(id: "m0", kind: .left, distanceFromStartM: 500)
        let m1 = CueManeuver(id: "m1", kind: .right, distanceFromStartM: 995)
        var s = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [m0, m1], routeTotalDistanceM: 1000),
            state: CueEngineState()
        ).nextState
        // Pass m0 — this fires arrivingInM and latches announced50m/10m for m1.
        s = CueEngine.tick(
            snapshot: base(progressDistanceM: 511, maneuvers: [m0, m1], routeTotalDistanceM: 1000),
            state: s
        ).nextState
        // Now enter the 50m window for m1.
        let r = CueEngine.tick(
            snapshot: base(progressDistanceM: 965, maneuvers: [m0, m1], routeTotalDistanceM: 1000),
            state: s
        )
        let hasTurn50m = r.events.contains { if case .turn50m = $0 { return true } else { return false } }
        XCTAssertFalse(hasTurn50m, "turn50m must be suppressed for the last maneuver when arrivingInM was already announced — got \(r.events)")
    }

    func test_firstTick_singleManeuver_closeToDestination_emitsArriving() {
        // Route with only one cue maneuver (right) at 995m of 1000m route.
        // First tick must emit arrivingInM, not nextTurnInAbout.
        let m1 = CueManeuver(id: "m1", kind: .right, distanceFromStartM: 995)
        let r = CueEngine.tick(
            snapshot: base(progressDistanceM: 0, maneuvers: [m1], routeTotalDistanceM: 1000),
            state: CueEngineState()
        )
        let hasNextTurn = r.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } }
        let hasArriving = r.events.contains { if case .arrivingInM = $0 { return true } else { return false } }
        XCTAssertFalse(hasNextTurn, "single last maneuver close to destination must not fire nextTurnInAbout on first tick — got \(r.events)")
        XCTAssertTrue(hasArriving, "single last maneuver close to destination must fire arrivingInM on first tick — got \(r.events)")
    }

    // MARK: - Bug 2: nextTurnInAbout + turn50m back-to-back for same maneuver

    func test_nextTurnInAbout_suppressesTurn50m_whenNextManeuverWithin50m() {
        // m1 at 500m, m2 at 545m. When the rider passes m1 (progress = 511m),
        // m2 is 34m away — within the 50m window. The after-passing block must
        // fire nextTurnInAbout AND pre-latch announced50m so turn50m does NOT
        // also fire in the same tick.
        let m1 = CueManeuver(id: "m1", kind: .left, distanceFromStartM: 500)
        let m2 = CueManeuver(id: "m2", kind: .right, distanceFromStartM: 545)
        var s = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [m1, m2], routeTotalDistanceM: 1000),
            state: CueEngineState()
        ).nextState
        s = CueEngine.tick(
            snapshot: base(progressDistanceM: 450, maneuvers: [m1, m2], routeTotalDistanceM: 1000),
            state: s
        ).nextState
        let r = CueEngine.tick(
            snapshot: base(progressDistanceM: 511, maneuvers: [m1, m2], routeTotalDistanceM: 1000),
            state: s
        )
        let hasNextTurn = r.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } }
        let hasTurn50m = r.events.contains { if case .turn50m = $0 { return true } else { return false } }
        XCTAssertTrue(hasNextTurn, "nextTurnInAbout must fire when passing m1 with m2 within 50m — got \(r.events)")
        XCTAssertFalse(hasTurn50m, "turn50m must be suppressed when nextTurnInAbout already announces the same maneuver — got \(r.events)")
    }

    func test_formatsSpecPhrases() {
        XCTAssertEqual(CueEngine.format(.turn50m(.left, distanceM: 50)), "In 50 meters, turn left")
        XCTAssertEqual(CueEngine.format(.turn50m(.exitLeft, distanceM: 50)), "In 50 meters, take the left exit")
        XCTAssertEqual(
            CueEngine.format(.turn50m(.right, distanceM: 50, followUpKind: .left)),
            "In 50 meters, turn right then quickly left"
        )
        // Actual-distance rendering: route-start scenarios where the cue
        // fires while the rider is already 15 m from the maneuver should
        // speak "20 meters" (rounded to nearest 10), not the legacy
        // hardcoded 50.
        XCTAssertEqual(
            CueEngine.format(.turn50m(.left, distanceM: 15)),
            "In 20 meters, turn left"
        )
        XCTAssertEqual(CueEngine.format(.turn10m(.right)), "Turn right")
        XCTAssertEqual(
            CueEngine.format(.nextTurnInAbout(turnKind: .left, distanceM: 187)),
            "Next turn left in about 190 meters"
        )
        XCTAssertEqual(
            CueEngine.format(.arrivingInM(distanceM: 184)),
            "Arriving at your destination in 180 meters"
        )
        XCTAssertEqual(CueEngine.format(.arrived), "You have arrived at your destination")
        XCTAssertEqual(CueEngine.format(.offTrack), "Off track")
        XCTAssertEqual(CueEngine.format(.repeatedOffTrackSilence), "Off track")
        XCTAssertEqual(CueEngine.format(.rerouting), "Rerouting")
        XCTAssertEqual(CueEngine.format(.onTrack), "On track")
    }

    // MARK: — turn10m fires 15m before the maneuver (5m earlier)

    func test_turn10mFiresAt14m_withinNew15mThreshold() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 100), state: CueEngineState()).nextState
        // 200 - 186 = 14 m remaining — fires with the new 15m threshold
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 186), state: s1)
        XCTAssertTrue(r2.events.contains(.turn10m(.left)),
            "turn10m must fire when 14m before the turn (new 15m threshold)")
    }

    func test_turn10mDoesNotFireAt16m_outsideThreshold() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 100), state: CueEngineState()).nextState
        // 200 - 184 = 16 m remaining — must NOT fire
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 184), state: s1)
        XCTAssertFalse(r2.events.contains(.turn10m(.left)),
            "turn10m must not fire when 16m before the turn (outside 15m threshold)")
    }

    // MARK: — skip turn50m when next turn is < 100m away

    func test_skipsTurn50m_whenRouteStarts80mFromFirstTurn() {
        // 80m is Case A (> 50m) → nextTurnInAbout fires. But 80m < 100m so
        // turn50m must be pre-latched on the same first tick.
        let m = CueManeuver(id: "m1", kind: .left, distanceFromStartM: 80)
        let snap = base(maneuvers: [m], routeTotalDistanceM: 500)
        let s1 = CueEngine.tick(snapshot: snap, state: CueEngineState())
        XCTAssertTrue(s1.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } },
            "nextTurnInAbout must still fire")
        // Advance into the 50m window — turn50m must NOT fire (pre-latched)
        var snap2 = snap; snap2 = base(progressDistanceM: 35, maneuvers: [m], routeTotalDistanceM: 500)
        let s2 = CueEngine.tick(snapshot: snap2, state: s1.nextState)
        XCTAssertFalse(s2.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "turn50m must not fire when first turn was < 100m at route start")
    }

    func test_firesTurn50m_whenRouteStarts120mFromFirstTurn() {
        // 120m > 100m threshold → turn50m is NOT pre-latched and fires normally
        let m = CueManeuver(id: "m1", kind: .left, distanceFromStartM: 120)
        let s1 = CueEngine.tick(snapshot: base(maneuvers: [m], routeTotalDistanceM: 500),
                                state: CueEngineState()).nextState
        // Advance to 45m from m1 (120-75=45m) → inside 50m window
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 75, maneuvers: [m], routeTotalDistanceM: 500),
                                state: s1)
        XCTAssertTrue(r2.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "turn50m must fire when turn was >= 100m at route start")
    }

    func test_skipsTurn50m_afterPassingManeuver_whenNextIs70mAway() {
        // m1 at 100m, m2 at 170m. After passing m1 (rider at 110m), m2 is 60m away (<100m).
        let m1 = CueManeuver(id: "m1", kind: .left, distanceFromStartM: 100)
        let m2 = CueManeuver(id: "m2", kind: .left, distanceFromStartM: 170)
        let maneuvers = [m1, m2]
        let s1 = CueEngine.tick(snapshot: base(maneuvers: maneuvers), state: CueEngineState()).nextState
        let s2 = CueEngine.tick(snapshot: base(progressDistanceM: 110, maneuvers: maneuvers), state: s1)
        XCTAssertTrue(s2.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } },
            "nextTurnInAbout must fire after passing m1")
        // Advance into m2's 50m window (170-125=45m) — turn50m must NOT fire
        let s3 = CueEngine.tick(snapshot: base(progressDistanceM: 125, maneuvers: maneuvers), state: s2.nextState)
        XCTAssertFalse(s3.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "turn50m must not fire when next turn was < 100m at time of nextTurnInAbout")
    }

    // MARK: — rerouting cue silences after 2 episodes

    private func reroutingSnap(_ rerouting: Bool, routeId: String = "r1") -> CueSnapshot {
        CueSnapshot(
            routeId: routeId,
            pairedWithDevice: false,
            progressDistanceM: 0,
            maneuvers: [CueManeuver(id: "m1", kind: .left, distanceFromStartM: 200)],
            offRoute: false,
            rerouting: rerouting,
            arrived: false,
            distanceFromRouteM: 0,
            routeTotalDistanceM: 1000
        )
    }

    func test_rerouting_firesOn1stEpisode() {
        let s1 = CueEngine.tick(snapshot: reroutingSnap(false), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: reroutingSnap(true), state: s1)
        XCTAssertTrue(r2.events.contains(.rerouting),
            "1st rerouting episode must fire the cue")
    }

    func test_rerouting_firesOn2ndEpisode_acrossRouteIdChange() {
        var s = CueEngineState()
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r1"), state: s).nextState
        s = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r1"), state: s).nextState // 1st
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r2"), state: s).nextState // new route
        let r = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r2"), state: s)        // 2nd
        XCTAssertTrue(r.events.contains(.rerouting),
            "2nd rerouting episode must still fire — cap is 2")
    }

    func test_rerouting_silencedOn3rdEpisode() {
        var s = CueEngineState()
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r1"), state: s).nextState
        s = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r1"), state: s).nextState // 1st
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r2"), state: s).nextState
        s = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r2"), state: s).nextState // 2nd
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r3"), state: s).nextState
        let r = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r3"), state: s)       // 3rd
        XCTAssertFalse(r.events.contains(.rerouting),
            "3rd rerouting episode must be silenced — episode count > 2")
    }

    func test_rerouting_resetsAfterConfirmedOnTrack() {
        var s = CueEngineState()
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r1"), state: s).nextState
        s = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r1"), state: s).nextState
        s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r2"), state: s).nextState
        s = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r2"), state: s).nextState
        // 5 confirmed-on-track ticks reset rerouting episode count
        for _ in 0..<5 {
            s = CueEngine.tick(snapshot: reroutingSnap(false, routeId: "r2"), state: s).nextState
        }
        let r = CueEngine.tick(snapshot: reroutingSnap(true, routeId: "r3"), state: s)
        XCTAssertTrue(r.events.contains(.rerouting),
            "After confirmed on-track the rerouting cue counter must reset")
    }
}
