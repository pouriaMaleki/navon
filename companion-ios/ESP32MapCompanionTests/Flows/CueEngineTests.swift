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

    func test_formatsSpecPhrases() {
        XCTAssertEqual(CueEngine.format(.turn50m(.left, distanceM: 50)), "In 50 meters, turn left")
        XCTAssertEqual(CueEngine.format(.turn50m(.keepRight, distanceM: 50)), "In 50 meters, keep right")
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
}
