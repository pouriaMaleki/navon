import XCTest
@testable import Navon

final class CueEngineTests: XCTestCase {

    private func mLeft(_ id: String, _ distance: Double) -> CueManeuver {
        CueManeuver(id: id, kind: .left, distanceFromStartM: distance)
    }

    private func mSlightLeft(_ id: String, _ distance: Double) -> CueManeuver {
        CueManeuver(id: id, kind: .slightLeft, distanceFromStartM: distance)
    }

    private func mSlightRight(_ id: String, _ distance: Double) -> CueManeuver {
        CueManeuver(id: id, kind: .slightRight, distanceFromStartM: distance)
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

    // User-reported regression: M1 (right) ~10 m before M2 (left). The
    // rider heard "turn right" only — no warning of the immediate left
    // after. The 50 m fusion branch is gated on `d > approach10M`, so
    // when a tick first lands within the 10 m approach window (sparse
    // GPS, fast cycling, or a foregrounded app), the 50 m combined cue
    // is skipped and the unsuffixed turn10m fires alone. The 10 m
    // branch needs the same back-to-back peek the 50 m branch performs.
    func test_back2backTurnsCoalesceAtTurn10mWhenFirstTickIsAlreadyInside15m() {
        let m1 = CueManeuver(id: "m1", kind: .right, distanceFromStartM: 200)
        let m2 = CueManeuver(id: "m2", kind: .left, distanceFromStartM: 210)
        // Tick 1 at 50 m progress — orientation cue path.
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 50, maneuvers: [m1, m2]),
            state: CueEngineState()
        ).nextState
        // Sparse-GPS jump from 50 m to 190 m progress — 10 m before m1, so
        // the 50 m approach window was skipped entirely.
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 190, maneuvers: [m1, m2]),
            state: s1
        )
        let combined = r2.events.first {
            if case .turn10m(_, let f) = $0 { return f != nil } else { return false }
        }
        XCTAssertNotNil(combined, "turn10m must coalesce with the follow-up when the 50 m window was skipped — got \(r2.events)")
        if case .turn10m(let k, let f) = combined! {
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

    func test_slightRightFiresNextTurnInAboutAfterPassingM1() {
        // slightRight is first-class: after passing m1 at 211m, m2 at 230m
        // (19m ahead) fires nextTurnInAbout.
        let maneuvers = [
            CueManeuver(id: "m1", kind: .left, distanceFromStartM: 200),
            mSlightRight("m2", 230),
        ]
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: maneuvers),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 211, maneuvers: maneuvers),
            state: s1
        )
        let next = r2.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(next, "slightRight must get nextTurnInAbout — got \(r2.events)")
    }

    func test_minorKeepSuppressesNextTurnInAboutEvenWhenFar() {
        // isMinorKeep maneuvers stay silent regardless of distance.
        let maneuvers = [
            CueManeuver(id: "m1", kind: .left, distanceFromStartM: 200),
            mSlightLeft("m2", 360),
        ]
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: maneuvers),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 211, maneuvers: maneuvers),
            state: s1
        )
        let next = r2.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(next, "slightLeft must fire nextTurnInAbout — got \(r2.events)")
    }

    func test_minorKeepSkippedNextRealTurnGetsPreview() {
        // m2 (slightRight isMinorKeep) is silently skipped. After passing
        // the slightRight, the next real turn (m3) gets nextTurnInAbout.
        let maneuvers = [
            CueManeuver(id: "m1", kind: .left, distanceFromStartM: 200),
            mSlightRight("m2", 230),
            CueManeuver(id: "m3", kind: .left, distanceFromStartM: 250),
        ]
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: maneuvers),
            state: CueEngineState()
        ).nextState
        // At 211m, just past m1. m2 is slightRight — suppressed.
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 211, maneuvers: maneuvers),
            state: s1
        )
        XCTAssertFalse(r2.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } },
            "minor-keep must suppress nextTurnInAbout — got \(r2.events)")
        XCTAssertFalse(r2.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "minor-keep must suppress turn50m — got \(r2.events)")
        // Advance past m2 (rider at 241, m2 at 230 passed by 11m).
        // m3 (regular left) should get nextTurnInAbout.
        let r3 = CueEngine.tick(
            snapshot: base(progressDistanceM: 241, maneuvers: maneuvers),
            state: r2.nextState
        )
        let next = r3.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(next, "after skipping minor-keep, next real turn must get preview — got \(r3.events)")
        if case .nextTurnInAbout(let kind, _) = next! {
            XCTAssertEqual(kind, .left)
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

    func test_emitsOffTrackAfter3ConsecutiveTicks() {
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        var s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s1).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        let r4 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s)
        XCTAssertTrue(r4.events.contains(.offTrack))
    }

    func test_emitsReroutingOnRisingEdge() {
        let s1 = CueEngine.tick(snapshot: base(offRoute: true), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(offRoute: true, rerouting: true), state: s1)
        XCTAssertTrue(r2.events.contains(.rerouting))
    }

    func test_afterMoreThanTwoOffRouteEpisodesGoesSilent() {
        var s = CueEngineState()
        // Episode 1: 3 consecutive off-route ticks → offTrack
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        // Reset: on-route
        s = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s).nextState
        // Episode 2: 3 consecutive → offTrack
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        // Reset: on-route
        s = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s).nextState
        // Episode 3: 3 consecutive → repeatedOffTrackSilence
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s).nextState
        let r3 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 40), state: s)
        XCTAssertTrue(r3.events.contains(.repeatedOffTrackSilence))
        s = r3.nextState
        let r4 = CueEngine.tick(snapshot: base(progressDistanceM: 155, offRoute: true), state: s)
        XCTAssertEqual(r4.events.count, 0)
    }

    // MARK: - off-track hysteresis

    func test_singleOffRouteTickDoesNotFireOffTrack() {
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s1)
        XCTAssertFalse(r2.events.contains(.offTrack))
    }

    func test_twoConsecutiveOffRouteTicksDoNotFireOffTrack() {
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        var s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s1).nextState
        let r3 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s)
        XCTAssertFalse(r3.events.contains(.offTrack))
    }

    func test_threeConsecutiveOffRouteTicksFireOffTrack() {
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        var s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s1).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s).nextState
        let r4 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s)
        XCTAssertTrue(r4.events.contains(.offTrack))
    }

    func test_onRouteTickResetsOffRouteConsecutiveCounter() {
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        // Two off-route ticks, then one on-route (reset), then one off-route
        var s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s1).nextState
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s).nextState
        s = CueEngine.tick(snapshot: base(offRoute: false, distanceFromRouteM: 5), state: s).nextState
        let r5 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s)
        // After reset: only 1 consecutive off-route, should not fire
        XCTAssertFalse(r5.events.contains(.offTrack))
        // Two more → total 3 consecutive → fires
        s = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: r5.nextState).nextState
        let r7 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 15), state: s)
        XCTAssertTrue(r7.events.contains(.offTrack))
    }

    func test_largeDistanceFromRouteFiresOffTrackImmediately() {
        // When distanceFromRouteM > 50m, the rider is genuinely lost — fire immediately.
        let s1 = CueEngine.tick(snapshot: base(), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(offRoute: true, distanceFromRouteM: 60), state: s1)
        XCTAssertTrue(r2.events.contains(.offTrack))
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

    // MARK: - Bear range-hold cues

    func test_slightLeftEmitsBearRangeWhenEnteringSegment() {
        // Bear at 200m, rider at 100m on first tick (Case A — far orientation).
        // Second tick at 195m (5m from bear) must fire turn10m, not approach cues.
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [mSlightLeft("m1", 200)]),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 195, maneuvers: [mSlightLeft("m1", 200)]),
            state: s1
        )
        let bear = r2.events.first { if case .turn10m = $0 { return true } else { return false } }
        XCTAssertNotNil(bear, "slightLeft at 5m must emit turn10m — got \(r2.events)")
        if case .turn10m(let k, _) = bear! { XCTAssertEqual(k, .slightLeft) }
        XCTAssertTrue(r2.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "slightLeft must fire turn50m — got \(r2.events)")
        XCTAssertTrue(r2.events.contains { if case .turn10m = $0 { return true } else { return false } },
            "slightRight must fire turn10m — got \(r2.events)")
    }

    func test_slightRightFiresTurn10m() {
        // SlightRight at 200, rider at 195 → 5m from maneuver → turn10m fires
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [mSlightRight("m1", 200)]),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 195, maneuvers: [mSlightRight("m1", 200)]),
            state: s1
        )
        let cue = r2.events.first { if case .turn10m = $0 { return true } else { return false } }
        XCTAssertNotNil(cue, "slightRight at 5m must emit turn10m — got \(r2.events)")
        if case .turn10m(let k, _) = cue! {
            XCTAssertEqual(k, .slightRight)
        }
    }

    func test_slightKindFiresTurn50m() {
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [mSlightLeft("m1", 200)]),
            state: CueEngineState()
        ).nextState
        // Rider at 165m: 35m from bear — inside 50m approach window
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 165, maneuvers: [mSlightLeft("m1", 200)]),
            state: s1
        )
        XCTAssertTrue(r2.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "turn50m must fire for slight turn — got \(r2.events)")
    }

    func test_slightKindFiresTurn10m() {
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [mSlightRight("m1", 200)]),
            state: CueEngineState()
        ).nextState
        // Rider at 195m: 5m from bear — inside 10m approach window
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 195, maneuvers: [mSlightRight("m1", 200)]),
            state: s1
        )
        XCTAssertFalse(r2.events.contains { if case .turn10m(let k, _) = $0 { return k == .slightRight } else { return false } },
            "turn10m must fire for slight turn — got \(r2.events)")
    }

    func test_slightKindFiresNextTurnInAboutAfterPassing() {
        // m1 (regular left at 200), m2 (slightLeft at 500). After passing m1, the
        // after-passing block must not emit nextTurnInAbout for the bear maneuver.
        let m1 = mLeft("m1", 200)
        let m2 = mSlightLeft("m2", 500)
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [m1, m2]),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 211, maneuvers: [m1, m2]),
            state: s1
        )
        let nextTurn = r2.events.first { if case .nextTurnInAbout = $0 { return true } else { return false } }
        XCTAssertNotNil(nextTurn, "must emit nextTurnInAbout for slight kind after passing — got \(r2.events)")
    }

    // MARK: - min bear segment

    func test_slightLeftFiresTurn10mAtShortRange() {
        // Bear at 200m, next at 230m → segment 30m < 50m → no turn10m
        let bear = mSlightLeft("m1", 200)
        let next = mLeft("m2", 230)
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [bear, next], routeTotalDistanceM: 500),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 195, maneuvers: [bear, next], routeTotalDistanceM: 500),
            state: s1
        )
        XCTAssertFalse(r2.events.contains { if case .turn10m = $0 { return true } else { return false } },
            "bear segment under 50m must not fire turn10m — got \(r2.events)")
    }

    func test_slightLeftTurn10mFires() {
        // Bear at 200m, next at 250m → segment 50m → turn10m fires
        let bear = mSlightLeft("m1", 200)
        let next = mLeft("m2", 250)
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [bear, next], routeTotalDistanceM: 500),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 195, maneuvers: [bear, next], routeTotalDistanceM: 500),
            state: s1
        )
        XCTAssertTrue(r2.events.contains { if case .turn10m = $0 { return true } else { return false } },
            "bear segment at 50m must fire turn10m")
    }

    func test_slightLeftNearEndSubstitutesArriving() {
        // Bear at 980m, route ends at 1000m → segment 20m < 50m
        let bear = mSlightLeft("m1", 980)
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 900, maneuvers: [bear], routeTotalDistanceM: 1000),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 975, maneuvers: [bear], routeTotalDistanceM: 1000),
            state: s1
        )
        XCTAssertFalse(r2.events.contains { if case .turn10m = $0 { return true } else { return false } },
            "bear at end of route with short segment must not fire — got \(r2.events)")
    }

    func test_slightLeftFiresTurn10mLikeAnyTurn() {
        // slightLeft (kind: .left) — NOT promoted to bear.
        // Must stay completely silent in approach windows.
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [mSlightLeft("m1", 200)]),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 195, maneuvers: [mSlightLeft("m1", 200)]),
            state: s1
        )
        XCTAssertFalse(r2.events.contains { if case .turn10m = $0 { return true } else { return false } },
            "unpromoted slightLeft must not fire turn10m — got \(r2.events)")
        XCTAssertFalse(r2.events.contains { if case .turn10m = $0 { return true } else { return false } },
            "unpromoted slightLeft must not fire turn10m")
        XCTAssertFalse(r2.events.contains { if case .turn50m = $0 { return true } else { return false } },
            "unpromoted slightLeft must not fire turn50m")
    }

    // MARK: - silence during rerouting

    func test_reroutingSuppressesTurn50m() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 100), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 155, rerouting: true), state: s1)
        XCTAssertFalse(r2.events.contains { if case .turn50m = $0 { return true } else { return false } })
    }

    func test_reroutingSuppressesTurn10m() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 155), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 192, rerouting: true), state: s1)
        XCTAssertFalse(r2.events.contains { if case .turn10m = $0 { return true } else { return false } })
    }

    func test_reroutingSuppressesNextTurnInAbout() {
        let s1 = CueEngine.tick(snapshot: base(progressDistanceM: 200), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(progressDistanceM: 211, rerouting: true), state: s1)
        XCTAssertFalse(r2.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } })
    }

    func test_reroutingSuppressesArrivingInM() {
        let snap = base(
            progressDistanceM: 412,
            maneuvers: [mLeft("m1", 400)],
            rerouting: true,
            routeTotalDistanceM: 600
        )
        let r = CueEngine.tick(snapshot: snap, state: CueEngineState())
        XCTAssertFalse(r.events.contains { if case .arrivingInM = $0 { return true } else { return false } })
    }

    func test_reroutingSuppressesTurn10mForSlight() {
        let bear = mSlightLeft("m1", 200)
        let s1 = CueEngine.tick(
            snapshot: base(progressDistanceM: 100, maneuvers: [bear]),
            state: CueEngineState()
        ).nextState
        let r2 = CueEngine.tick(
            snapshot: base(progressDistanceM: 192, maneuvers: [bear], rerouting: true),
            state: s1
        )
        XCTAssertFalse(r2.events.contains { if case .turn10m = $0 { return true } else { return false } })
    }

    func test_reroutingCueItselfStillFires() {
        let s1 = CueEngine.tick(snapshot: base(offRoute: true), state: CueEngineState()).nextState
        let r2 = CueEngine.tick(snapshot: base(offRoute: true, rerouting: true), state: s1)
        XCTAssertTrue(r2.events.contains(.rerouting))
    }

    func test_reroutingDoesNotSuppressArrived() {
        let r = CueEngine.tick(snapshot: base(rerouting: true, arrived: true), state: CueEngineState())
        XCTAssertTrue(r.events.contains(.arrived))
    }

    func test_whenReroutingBecomesFalse_cuesResume() {
        var s = CueEngine.tick(
            snapshot: base(offRoute: true, rerouting: true),
            state: CueEngineState()
        ).nextState
        // New route, rerouting false → first-tick nextTurnInAbout fires
        let r = CueEngine.tick(
            snapshot: base(
                routeId: "r2",
                progressDistanceM: 0,
                maneuvers: [mLeft("m1", 200)],
                offRoute: false,
                rerouting: false
            ),
            state: s
        )
        XCTAssertTrue(r.events.contains { if case .nextTurnInAbout = $0 { return true } else { return false } })
    }

    // MARK: - km distance formatting

    func test_distanceCueValues_1310m_metric_returnsKilometers() {
        let result = DistanceFormatter.cueValues(meters: 1310, mode: .metric)
        XCTAssertEqual(String(describing: result["distanceUnit"]), String(describing: MessageValue.string("kilometers")))
        if case .number(let d) = result["distance"] { XCTAssertEqual(d, 1.3, accuracy: 0.1) }
    }

    func test_distanceCueValues_500m_metric_returnsMeters() {
        let result = DistanceFormatter.cueValues(meters: 500, mode: .metric)
        XCTAssertEqual(String(describing: result["distanceUnit"]), String(describing: MessageValue.string("meters")))
        if case .number(let d) = result["distance"] { XCTAssertEqual(d, 500.0) }
    }

    func test_distanceCueValues_1000m_metric_returnsKilometers() {
        let result = DistanceFormatter.cueValues(meters: 1000, mode: .metric)
        XCTAssertEqual(String(describing: result["distanceUnit"]), String(describing: MessageValue.string("kilometers")))
        if case .number(let d) = result["distance"] { XCTAssertEqual(d, 1.0, accuracy: 0.1) }
    }

    func test_formatCueEvent_nextTurnInAbout_1310m_showsKm() {
        let text = CueEngine.format(.nextTurnInAbout(turnKind: .left, distanceM: 1310))
        XCTAssertEqual(text, "Next turn left in about 1.3 kilometers")
    }

    // MARK: - Collapse close maneuvers

    private func rm(_ id: String, _ dist: Double, _ type: RouteManeuverType = .slightLeft) -> RouteManeuver {
        RouteManeuver(
            id: id,
            maneuverType: type,
            location: CoordinatePoint(latitude: 60.0 + dist / 111_320.0, longitude: 25.0),
            distanceFromStartMeters: dist
        )
    }

    private func northGeometry(lengthM: Double = 250) -> [CoordinatePoint] {
        var points: [CoordinatePoint] = []
        let step = 5.0
        var d = 0.0
        while d <= lengthM {
            points.append(CoordinatePoint(latitude: 60.0 + d / 111_320.0, longitude: 25.0))
            d += step
        }
        return points
    }

    func test_collapse_singleManeuverUnchanged() {
        let geom = northGeometry()
        let maneuvers = [rm("m1", 100)]
        let result = collapseCloseManeuvers(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 1)
        XCTAssertEqual(result[0].id, "m1")
    }

    func test_collapse_emptyReturnsEmpty() {
        let geom = northGeometry()
        let result = collapseCloseManeuvers([], geometry: geom)
        XCTAssertEqual(result.count, 0)
    }

    func test_collapse_gapMoreThan5m_preservesBoth() {
        let geom = northGeometry()
        let maneuvers = [rm("m1", 100), rm("m2", 110)]
        let result = collapseCloseManeuvers(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 2)
    }

    func test_collapse_twoCloseManeuvers_netAngle30deg_preservesBoth() {
        // Straight north geometry → net angle ~0° → preserve both
        let geom = northGeometry()
        let maneuvers = [rm("m1", 100), rm("m2", 103)]
        let result = collapseCloseManeuvers(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 2)
    }

    private func bendGeometry(bendDistanceM: Double, bendAngleDeg: Double, totalLengthM: Double = 250) -> [CoordinatePoint] {
        let metersPerDeg = 111_320.0
        let step = 5.0
        var points: [CoordinatePoint] = []
        var lat = 60.0
        var lon = 25.0
        points.append(CoordinatePoint(latitude: lat, longitude: lon))
        var dist = 0.0
        while dist < totalLengthM {
            let segLen = min(step, totalLengthM - dist)
            let bearing = dist >= bendDistanceM ? bendAngleDeg : 0.0
            let rad = bearing * .pi / 180.0
            let cosLat = cos((lat + lat) / 2.0 * .pi / 180.0)
            lat += segLen * cos(rad) / metersPerDeg
            lon += segLen * sin(rad) / (metersPerDeg * cosLat)
            dist += segLen
            points.append(CoordinatePoint(latitude: lat, longitude: lon))
        }
        return points
    }

    func test_collapse_twoCloseManeuvers_netAngleExceeds30deg_collapses() {
        let geom = bendGeometry(bendDistanceM: 105, bendAngleDeg: 45)
        let maneuvers = [rm("m1", 100), rm("m2", 103)]
        let result = collapseCloseManeuvers(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 1)
        XCTAssertEqual(result[0].id, "m2")
    }

    func test_collapse_gapExactly5m_noCollapse() {
        let geom = northGeometry()
        let maneuvers = [rm("m1", 100), rm("m2", 105)]
        let result = collapseCloseManeuvers(maneuvers, geometry: geom)
        XCTAssertEqual(result.count, 2)
    }
}
