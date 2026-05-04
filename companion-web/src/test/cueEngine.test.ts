import { describe, expect, it } from "vitest";
import {
  type CueEngineState,
  type CueEvent,
  type CueManeuver,
  type CueSnapshot,
  formatCueEvent,
  initialCueEngineState,
  tickCueEngine,
} from "../integrations/cues/CueEngine.js";
import { maneuverKindFromType } from "../integrations/cues/RoutingActivityCoordinator.js";

const M_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "left",
  distanceFromStartM: distance,
});
const M_RIGHT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "right",
  distanceFromStartM: distance,
});

const baseSnapshot = (overrides: Partial<CueSnapshot> = {}): CueSnapshot => ({
  routeId: "r1",
  pairedWithDevice: false,
  progressDistanceM: 0,
  maneuvers: [M_LEFT("m1", 200), M_LEFT("m2", 400)],
  offRoute: false,
  rerouting: false,
  arrived: false,
  distanceFromRouteM: 0,
  routeTotalDistanceM: 1000,
  ...overrides,
});

function tick(
  state: CueEngineState,
  snapshot: CueSnapshot,
): { events: CueEvent[]; next: CueEngineState } {
  const result = tickCueEngine(snapshot, state);
  return { events: result.events, next: result.nextState };
}

describe("CueEngine — first-tick announcement (replaces 'Route started')", () => {
  // User-reported: "Route started" was useless padding the rider already
  // knew. Replace it with the actual next-turn announcement so the first
  // sound the rider hears is "Next turn left in about 200 meters" — i.e.,
  // what they actually need to plan for.

  it("does NOT emit 'routeStarted' on the first tick of a new route", () => {
    const { events } = tick(initialCueEngineState(), baseSnapshot());
    expect(events.map((e) => e.kind)).not.toContain("routeStarted");
  });

  it("announces the next turn with distance on the first tick instead", () => {
    const { events } = tick(initialCueEngineState(), baseSnapshot());
    const announce = events.find((e) => e.kind === "nextTurnInAbout");
    expect(announce).toBeDefined();
    expect(announce).toMatchObject({ turnKind: "left" });
    expect((announce as { distanceM: number }).distanceM).toBeCloseTo(200, 0);
  });

  it("does not re-announce the next turn on subsequent ticks of the same route", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 5 }));
    expect(t2.events.find((e) => e.kind === "nextTurnInAbout")).toBeUndefined();
  });

  it("does not re-announce after the rider returns from a background gap (route id unchanged)", () => {
    // Bug we're fixing: after a long backgrounded period the GPS gap
    // was producing a `routeStarted` re-emission ("Route started" while
    // the rider had been on the route for ages). With route-id checks
    // and no first-tick re-announcement, this regression can't recur.
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 50 }));
    // Simulate a 30-second background gap with no ticks, then a fix.
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 60 }));
    expect(t2.events.find((e) => e.kind === "nextTurnInAbout")).toBeUndefined();
  });

  it("does announce on a brand new route id (reroute completion)", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 50 }));
    const t2 = tick(t1.next, baseSnapshot({ routeId: "r2", progressDistanceM: 0 }));
    expect(t2.events.find((e) => e.kind === "nextTurnInAbout")).toBeDefined();
  });
});

describe("CueEngine — back-to-back turn coalescing", () => {
  // User-reported: when two maneuvers are very close (e.g., M1 at 200m,
  // M2 at 230m), the engine emits four cues within a few seconds —
  // turn50m(M1), turn10m(M1), nextTurnInAbout(M2), turn50m(M2). The TTS
  // engine cancels the in-flight one but the rider hears half of one
  // phrase chopped off by the next. Solution: when the next maneuver
  // is within 80m of the upcoming one, fold both into a single cue
  // ("In 50 meters, turn right then quickly left") and suppress the
  // duplicate emissions for the second maneuver.
  it("annotates turn50m with the follow-up maneuver when M2 is within 80m of M1", () => {
    const back2back = baseSnapshot({
      maneuvers: [M_RIGHT("m1", 200), M_LEFT("m2", 230)],
      progressDistanceM: 100,
    });
    const t1 = tick(initialCueEngineState(), back2back);
    // Cross 50m threshold for m1 — at 155m progress, distance to m1 = 45m.
    const t2 = tick(t1.next, { ...back2back, progressDistanceM: 155 });
    const turn50 = t2.events.find((e) => e.kind === "turn50m");
    expect(turn50).toMatchObject({
      kind: "turn50m",
      turnKind: "right",
      followUpKind: "left",
    });
  });

  it("does not annotate when the follow-up is far away (no overlap risk)", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 100 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 155 }));
    const turn50 = t2.events.find((e) => e.kind === "turn50m") as
      | { followUpKind?: string }
      | undefined;
    expect(turn50?.followUpKind).toBeUndefined();
  });

  it("suppresses turn50m / turn10m / nextTurnInAbout for the follow-up maneuver after a coalesced cue", () => {
    const back2back = baseSnapshot({
      maneuvers: [M_RIGHT("m1", 200), M_LEFT("m2", 230)],
    });
    let s = initialCueEngineState();
    // 50m before m1 — coalesced cue fires.
    s = tick(s, { ...back2back, progressDistanceM: 155 }).next;
    // Past m1, approaching m2.
    const past = tick(s, { ...back2back, progressDistanceM: 211 });
    expect(past.events.find((e) => e.kind === "nextTurnInAbout")).toBeUndefined();
    // 10m before m2 — already announced via the coalesced cue.
    s = past.next;
    const close = tick(s, { ...back2back, progressDistanceM: 222 });
    expect(close.events.find((e) => e.kind === "turn10m")).toBeUndefined();
    expect(close.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });
});

describe("CueEngine — existing approach + arrival cues unchanged", () => {
  it("emits 'in 50 meters turn left' when crossing the 50m threshold for a maneuver", () => {
    const before = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 100 }));
    const after = tick(before.next, baseSnapshot({ progressDistanceM: 155 }));
    const turn50 = after.events.find((e) => e.kind === "turn50m");
    expect(turn50).toMatchObject({ kind: "turn50m", turnKind: "left" });
  });

  it("does not re-emit the 50m cue for the same maneuver on subsequent ticks", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 155 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 160 }));
    expect(t2.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });

  it("emits 'turn left' when crossing the 10m threshold for a maneuver", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 155 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 192 }));
    const turn10 = t2.events.find((e) => e.kind === "turn10m");
    expect(turn10).toMatchObject({ kind: "turn10m", turnKind: "left" });
  });

  it("emits 'next turn left in about X meters' 10m past a maneuver, when there is another after", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 200 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 211 }));
    const next = t2.events.find((e) => e.kind === "nextTurnInAbout");
    expect(next).toMatchObject({ turnKind: "left" });
    expect((next as { distanceM: number }).distanceM).toBeCloseTo(189, 0);
  });

  it("emits 'arriving at your destination in X meters' past the last maneuver when no more maneuvers follow", () => {
    const snap = baseSnapshot({
      progressDistanceM: 412,
      maneuvers: [M_LEFT("m1", 400)],
      routeTotalDistanceM: 600,
    });
    const t = tick(initialCueEngineState(), snap);
    const arr = t.events.find((e) => e.kind === "arrivingInM");
    expect(arr).toBeDefined();
  });

  it("emits 'arrived' when arrived flag is true", () => {
    const t = tick(initialCueEngineState(), baseSnapshot({ arrived: true }));
    expect(t.events.find((e) => e.kind === "arrived")).toBeDefined();
  });

  it("emits 'offtrack' on the first off-route episode rising edge", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    const t2 = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 }));
    expect(t2.events.find((e) => e.kind === "offTrack")).toBeDefined();
  });

  it("emits 'rerouting' on rerouting rising edge", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ offRoute: true }));
    const t2 = tick(t1.next, baseSnapshot({ offRoute: true, rerouting: true }));
    expect(t2.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });

  it("after >2 off-route episodes, says 'off track' once and goes silent until on-track", () => {
    let s = initialCueEngineState();
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    const t3 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 }));
    expect(t3.events.find((e) => e.kind === "repeatedOffTrackSilence")).toBeDefined();
    s = t3.next;
    const t4 = tick(s, baseSnapshot({ offRoute: true, progressDistanceM: 155 }));
    expect(t4.events).toHaveLength(0);
  });

  it("after silence, emits 'on track' when 5 consecutive samples have distanceFromRouteM < 22", () => {
    let s: CueEngineState = {
      ...initialCueEngineState(),
      lastRouteId: "r1",
      routeStartedAnnounced: true,
      offRouteEpisodeCount: 3,
      silenced: true,
      prevOffRoute: true,
    };
    for (let i = 0; i < 4; i++) {
      const t = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 }));
      expect(t.events.find((e) => e.kind === "onTrack")).toBeUndefined();
      s = t.next;
    }
    const t5 = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 }));
    expect(t5.events.find((e) => e.kind === "onTrack")).toBeDefined();
  });

  it("paired-with-device suppresses every cue", () => {
    const snap = baseSnapshot({
      pairedWithDevice: true,
      progressDistanceM: 192,
      arrived: false,
      offRoute: true,
    });
    const t = tick(initialCueEngineState(), snap);
    expect(t.events).toHaveLength(0);
  });

  it("formatCueEvent renders spec phrases (no 'Route started' anymore)", () => {
    expect(formatCueEvent({ kind: "turn50m", turnKind: "left", distanceM: 50 })).toBe(
      "In 50 meters, turn left",
    );
    expect(
      formatCueEvent({
        kind: "turn50m",
        turnKind: "right",
        distanceM: 50,
        followUpKind: "left",
      }),
    ).toBe("In 50 meters, turn right then quickly left");
    // Actual-distance rendering: route-start scenarios where the cue
    // fires while the rider is already 15 m from the maneuver should
    // speak "20 meters" (rounded), not the legacy hardcoded "50".
    expect(formatCueEvent({ kind: "turn50m", turnKind: "left", distanceM: 15 })).toBe(
      "In 20 meters, turn left",
    );
    expect(formatCueEvent({ kind: "turn10m", turnKind: "right" })).toBe("Turn right");
    expect(formatCueEvent({ kind: "nextTurnInAbout", turnKind: "left", distanceM: 187 })).toBe(
      "Next turn left in about 190 meters",
    );
    expect(formatCueEvent({ kind: "arrived" })).toBe("You have arrived at your destination");
  });
});

describe("CueEngine — Bug 1: last maneuver close to destination emits arrivingInM", () => {
  // Bug: when the last cue maneuver sits within ~30 m of the route end,
  // the engine emitted nextTurnInAbout / turn50m for that phantom turn
  // instead of arrivingInM.  The rider heard "next turn right in 80 m"
  // while actually approaching the destination.

  it("first-tick: emits arrivingInM (not nextTurnInAbout) when sole maneuver is within 30 m of route end", () => {
    // Single maneuver at 975 m; route ends at 1000 m — 25 m gap (< 30 m threshold).
    const snap = baseSnapshot({
      maneuvers: [{ id: "m1", kind: "right" as const, distanceFromStartM: 975 }],
      routeTotalDistanceM: 1000,
      progressDistanceM: 0,
    });
    const { events } = tick(initialCueEngineState(), snap);
    expect(events.find((e) => e.kind === "arrivingInM")).toBeDefined();
    expect(events.find((e) => e.kind === "nextTurnInAbout")).toBeUndefined();
  });

  it("first-tick: emits nextTurnInAbout when sole maneuver is far from route end (> 30 m)", () => {
    const snap = baseSnapshot({
      maneuvers: [{ id: "m1", kind: "right" as const, distanceFromStartM: 200 }],
      routeTotalDistanceM: 1000,
      progressDistanceM: 0,
    });
    const { events } = tick(initialCueEngineState(), snap);
    expect(events.find((e) => e.kind === "nextTurnInAbout")).toBeDefined();
    expect(events.find((e) => e.kind === "arrivingInM")).toBeUndefined();
  });

  it("after-passing block: emits arrivingInM (not nextTurnInAbout) when next maneuver is within 30 m of route end", () => {
    // m1 at 400 m, m2 at 975 m of 1000 m (25 m gap < 30 m threshold); rider passes m1.
    const snap = baseSnapshot({
      maneuvers: [
        { id: "m1", kind: "left" as const, distanceFromStartM: 400 },
        { id: "m2", kind: "right" as const, distanceFromStartM: 975 },
      ],
      routeTotalDistanceM: 1000,
      progressDistanceM: 415,
    });
    const { events } = tick(initialCueEngineState(), snap);
    expect(events.find((e) => e.kind === "arrivingInM")).toBeDefined();
    expect(events.find((e) => e.kind === "nextTurnInAbout")).toBeUndefined();
  });

  it("after-passing block: suppresses turn50m for the last maneuver within 30 m of route end", () => {
    // Rider at 930 m; m2 at 975 m of 1000 m — within 50 m approach AND
    // last maneuver within 30 m of end. turn50m must not fire.
    const snap = baseSnapshot({
      maneuvers: [
        { id: "m1", kind: "left" as const, distanceFromStartM: 400 },
        { id: "m2", kind: "right" as const, distanceFromStartM: 975 },
      ],
      routeTotalDistanceM: 1000,
      progressDistanceM: 930,
    });
    const { events } = tick(initialCueEngineState(), snap);
    expect(events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });
});

describe("CueEngine — Bug 3: same routeId with bumped revision resets engine state", () => {
  // Bug: the engine used only routeId (the identifier string) for its
  // change-detection guard. When a reroute returned the same identifier
  // but a higher revision, CueEngineState was not reset, so stale
  // progressDistanceM made all new-route maneuvers appear "already passed"
  // and a ghost arrivingInM fired as the first cue of the new route.
  //
  // Fix: the caller must pass a composite key "routeId-revN" so the engine
  // detects the revision bump as a genuine route change.

  it("resets engine state when routeId changes to same-identifier-but-new-revision composite key", () => {
    // Simulate 300 m of progress on route r1-rev1.
    const s1 = tick(
      initialCueEngineState(),
      baseSnapshot({ routeId: "lshape-rev1", progressDistanceM: 300 }),
    ).next;

    // Reroute: same base identifier, revision bumped to 2.
    // New 200 m route with a right turn at 100 m.
    const rerouteSnap: CueSnapshot = {
      routeId: "lshape-rev2",
      pairedWithDevice: false,
      progressDistanceM: 0,
      maneuvers: [{ id: "r-m1", kind: "right" as const, distanceFromStartM: 100 }],
      offRoute: false,
      rerouting: false,
      arrived: false,
      distanceFromRouteM: 0,
      routeTotalDistanceM: 200,
    };
    const { events } = tick(s1, rerouteSnap);

    const arrivingCues = events.filter((e) => e.kind === "arrivingInM");
    expect(arrivingCues).toHaveLength(0);

    const nextTurnCues = events.filter(
      (e) => e.kind === "nextTurnInAbout" && (e as { turnKind: string }).turnKind === "right",
    );
    expect(nextTurnCues).toHaveLength(1);
  });
});

describe("CueEngine — turn10m fires 15m before the maneuver (5m earlier threshold)", () => {
  it("fires turn10m when 14m before the turn (within the 15m window)", () => {
    const m = M_LEFT("m1", 200);
    const s1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 100 }));
    // 200 - 186 = 14 m remaining — should fire with the new 15m threshold
    const s2 = tick(s1.next, baseSnapshot({ progressDistanceM: 186 }));
    const t10 = s2.events.find((e) => e.kind === "turn10m");
    expect(t10).toBeDefined();
    expect(t10).toMatchObject({ kind: "turn10m", turnKind: "left" });
  });

  it("does not fire turn10m when 16m before the turn (just outside the 15m window)", () => {
    const s1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 100 }));
    // 200 - 184 = 16 m remaining — should NOT fire
    const s2 = tick(s1.next, baseSnapshot({ progressDistanceM: 184 }));
    expect(s2.events.find((e) => e.kind === "turn10m")).toBeUndefined();
  });
});

describe("CueEngine — skip turn50m cue when next turn is < 100m away", () => {
  it("skips turn50m when route starts 80m from the first turn", () => {
    // 80m > 50m so Case A fires nextTurnInAbout; but < 100m so turn50m must be pre-latched.
    const m1: CueManeuver = { id: "m1", kind: "left", distanceFromStartM: 80 };
    const snap = baseSnapshot({ maneuvers: [m1], routeTotalDistanceM: 500 });
    const s1 = tick(initialCueEngineState(), snap);
    // nextTurnInAbout fires on the first tick (80m away)
    expect(s1.events.find((e) => e.kind === "nextTurnInAbout")).toBeDefined();
    // Advance into the 50m window — turn50m must NOT fire (pre-latched)
    const s2 = tick(s1.next, { ...snap, progressDistanceM: 35 }); // 80-35=45m < 50m
    expect(s2.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });

  it("fires turn50m when route starts 120m from the first turn (above the 100m threshold)", () => {
    const m1: CueManeuver = { id: "m1", kind: "left", distanceFromStartM: 120 };
    const snap = baseSnapshot({ maneuvers: [m1], routeTotalDistanceM: 500 });
    const s1 = tick(initialCueEngineState(), snap);
    // Advance into 50m window — turn50m SHOULD fire (not pre-latched)
    const s2 = tick(s1.next, { ...snap, progressDistanceM: 75 }); // 120-75=45m < 50m
    expect(s2.events.find((e) => e.kind === "turn50m")).toBeDefined();
  });

  it("skips turn50m after passing a maneuver when the next turn is 70m away", () => {
    // m1 at 100m, m2 at 170m. After passing m1 at rider=110m, m2 is 60m away (<100m).
    const m1 = M_LEFT("m1", 100);
    const m2 = M_LEFT("m2", 170);
    const snap = (progress: number) =>
      baseSnapshot({ maneuvers: [m1, m2], progressDistanceM: progress, routeTotalDistanceM: 1000 });
    const s1 = tick(initialCueEngineState(), snap(0));
    const s2 = tick(s1.next, snap(110)); // 10m past m1 → nextTurnInAbout(m2) at 60m
    expect(s2.events.find((e) => e.kind === "nextTurnInAbout")).toBeDefined();
    // Advance into m2's 50m window — turn50m must NOT fire (pre-latched)
    const s3 = tick(s2.next, snap(125)); // 170-125=45m
    expect(s3.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });
});

describe("CueEngine — bear left/right (slight turns) produce NO audio cues", () => {
  // User feedback: "Bear left/right" was being spoken on every minor curve in
  // the road. There's no clear split — the rider would just naturally follow
  // the road. The cue is noise. Suppress slight* maneuvers entirely.
  it("maneuverKindFromType returns undefined for slightLeft (silent)", () => {
    expect(maneuverKindFromType("slightLeft")).toBeUndefined();
  });

  it("maneuverKindFromType returns undefined for slightRight (silent)", () => {
    expect(maneuverKindFromType("slightRight")).toBeUndefined();
  });

  it("sharpLeft still maps to left ManeuverKind (sharp turns are real cues)", () => {
    expect(maneuverKindFromType("sharpLeft")).toBe("left");
  });

  it("sharpRight still maps to right ManeuverKind", () => {
    expect(maneuverKindFromType("sharpRight")).toBe("right");
  });
});

describe("CueEngine — rerouting cue silences after 2 episodes (across reroutes)", () => {
  // User feedback: "rerouting" was firing every time off-route → reroute
  // cycle completed. With repeated drift, the rider hears "Rerouting" over
  // and over. Cap to 2; after that, stay silent until a confirmed on-track.
  // Episode count must persist across route id changes (every successful
  // reroute issues a new route id, so resetting on route id would defeat
  // the cap).

  function reroutingSnapshot(rerouting: boolean, routeId = "r1"): CueSnapshot {
    return baseSnapshot({ rerouting, offRoute: false, routeId, distanceFromRouteM: 0 });
  }

  it("fires rerouting cue on the 1st episode", () => {
    const t1 = tick(initialCueEngineState(), reroutingSnapshot(false));
    const t2 = tick(t1.next, reroutingSnapshot(true));
    expect(t2.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });

  it("fires rerouting cue on the 2nd episode (different route id, after the first reroute)", () => {
    let s = initialCueEngineState();
    s = tick(s, reroutingSnapshot(false, "r1")).next;
    s = tick(s, reroutingSnapshot(true, "r1")).next; // 1st rises
    s = tick(s, reroutingSnapshot(false, "r2")).next; // new route after reroute
    const t = tick(s, reroutingSnapshot(true, "r2")); // 2nd rises
    expect(t.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });

  it("does NOT fire rerouting cue on the 3rd episode (silenced)", () => {
    let s = initialCueEngineState();
    s = tick(s, reroutingSnapshot(false, "r1")).next;
    s = tick(s, reroutingSnapshot(true, "r1")).next; // 1st
    s = tick(s, reroutingSnapshot(false, "r2")).next;
    s = tick(s, reroutingSnapshot(true, "r2")).next; // 2nd
    s = tick(s, reroutingSnapshot(false, "r3")).next;
    const t = tick(s, reroutingSnapshot(true, "r3")); // 3rd → silenced
    expect(t.events.find((e) => e.kind === "rerouting")).toBeUndefined();
  });

  it("resets the rerouting cue counter after a confirmed on-track (5 corridor samples)", () => {
    let s = initialCueEngineState();
    // Fire the 2-cue cap then exhaust it
    s = tick(s, reroutingSnapshot(false, "r1")).next;
    s = tick(s, reroutingSnapshot(true, "r1")).next; // 1st
    s = tick(s, reroutingSnapshot(false, "r2")).next;
    s = tick(s, reroutingSnapshot(true, "r2")).next; // 2nd
    // Now drive 5 confirmed-on-track samples to clear silence
    for (let i = 0; i < 5; i += 1) {
      s = tick(s, baseSnapshot({ rerouting: false, offRoute: false, distanceFromRouteM: 0, routeId: "r2" })).next;
    }
    // After reset, a new rerouting episode should fire again
    const t = tick(s, reroutingSnapshot(true, "r3"));
    expect(t.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });
});
