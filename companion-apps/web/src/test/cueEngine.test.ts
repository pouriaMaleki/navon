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
import { distanceCueValues } from "../i18n/formatDistance.js";
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
const M_SLIGHT_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "slightLeft",
  distanceFromStartM: distance,
});
const M_SLIGHT_RIGHT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "slightRight",
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

  // User-reported regression: M1 (left) at ~10 m before M2 (right). The
  // rider heard "turn left" only — no mention of the right turn. The 50 m
  // fusion branch is gated on `upcomingDistance > 15 m`, so when a tick
  // first lands within the 10 m approach window (sparse GPS, foregrounded
  // app, fast cycling), the 50 m combined cue is skipped and the
  // unsuffixed `turn10m` for M1 fires alone. The 10 m branch needs the
  // same back-to-back peek the 50 m branch already performs.
  it("annotates turn10m with the follow-up maneuver when the rider's first in-range tick is already inside 15m and M2 is within 30m", () => {
    const back2back = baseSnapshot({
      maneuvers: [M_RIGHT("m1", 200), M_LEFT("m2", 210)],
      progressDistanceM: 50,
    });
    let s = initialCueEngineState();
    // First tick at 50 m progress — orientation cue path; the 50 m combined
    // path will be considered on subsequent ticks.
    s = tick(s, back2back).next;
    // Sparse-GPS jump straight from 50 m progress (150 m before M1) to
    // 190 m progress (10 m before M1). Skips the 50 m approach window
    // entirely, so the 50 m combined cue cannot fire.
    const close = tick(s, { ...back2back, progressDistanceM: 190 });
    const turn10 = close.events.find((e) => e.kind === "turn10m");
    expect(turn10).toMatchObject({
      kind: "turn10m",
      turnKind: "right",
      followUpKind: "left",
    });
    // No separate turn10m for M2 should follow — the combined cue already
    // mentioned it.
    const past = tick(close.next, { ...back2back, progressDistanceM: 205 });
    expect(past.events.find((e) => e.kind === "turn10m")).toBeUndefined();
  });

  it("does not annotate turn10m when the follow-up is far away", () => {
    // Standard single-turn case: progress jumps from 100 → 192 (8 m before
    // m1 at 200; m2 at 400 is 200 m past — far). The 10 m cue must remain
    // a single-direction event.
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 100 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 192 }));
    const turn10 = t2.events.find((e) => e.kind === "turn10m") as
      | { followUpKind?: string }
      | undefined;
    expect(turn10).toBeDefined();
    expect(turn10?.followUpKind).toBeUndefined();
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

  it("emits 'offtrack' after 3 consecutive off-route ticks", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    let s = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    const t4 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 }));
    expect(t4.events.find((e) => e.kind === "offTrack")).toBeDefined();
  });

  it("emits 'rerouting' on rerouting rising edge", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ offRoute: true }));
    const t2 = tick(t1.next, baseSnapshot({ offRoute: true, rerouting: true }));
    expect(t2.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });

  it("after >2 off-route episodes, says 'off track' once and goes silent until on-track", () => {
    let s = initialCueEngineState();
    // Episode 1: 3 consecutive off-route ticks → offTrack fires
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    // Reset: on-route
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    // Episode 2: 3 consecutive → offTrack
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    // Reset: on-route
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    // Episode 3: 3 consecutive → repeatedOffTrackSilence
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    const t3 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 }));
    expect(t3.events.find((e) => e.kind === "repeatedOffTrackSilence")).toBeDefined();
    s = t3.next;
    const t4 = tick(s, baseSnapshot({ offRoute: true, progressDistanceM: 155 }));
    expect(t4.events).toHaveLength(0);
  });

  // ─── off-track hysteresis: require 3 consecutive off-route ticks ───

  it("single off-route tick does NOT fire offTrack", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    const t2 = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 }));
    expect(t2.events.find((e) => e.kind === "offTrack")).toBeUndefined();
  });

  it("two consecutive off-route ticks do NOT fire offTrack", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    let s = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 })).next;
    const t3 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 }));
    expect(t3.events.find((e) => e.kind === "offTrack")).toBeUndefined();
  });

  it("three consecutive off-route ticks fire offTrack", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    let s = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 })).next;
    const t4 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 }));
    expect(t4.events.find((e) => e.kind === "offTrack")).toBeDefined();
  });

  it("on-route tick resets the off-route consecutive counter", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    // Two off-route ticks, then one on-route, then one off-route → counter reset
    let s = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 })).next;
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 })).next;
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    const t5 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 }));
    // After reset: only 1 consecutive off-route, should not fire
    expect(t5.events.find((e) => e.kind === "offTrack")).toBeUndefined();
    // Two more off-route ticks → total 3 consecutive after reset → fires
    s = tick(t5.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 })).next;
    const t7 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 15 }));
    expect(t7.events.find((e) => e.kind === "offTrack")).toBeDefined();
  });

  it("large distance from route fires offTrack immediately regardless of hysteresis", () => {
    // When distanceFromRouteM > 50m, the rider is genuinely lost — fire immediately.
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    const t2 = tick(t1.next, baseSnapshot({ offRoute: true, distanceFromRouteM: 60 }));
    expect(t2.events.find((e) => e.kind === "offTrack")).toBeDefined();
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

describe("CueEngine — slight turns are first-class cues", () => {
  it("maneuverKindFromType maps slightLeft to slightLeft", () => {
    expect(maneuverKindFromType("slightLeft")).toEqual("slightLeft");
  });

  it("maneuverKindFromType maps slightRight to slightRight", () => {
    expect(maneuverKindFromType("slightRight")).toEqual("slightRight");
  });

  it("sharpLeft still maps to left ManeuverKind (sharp turns are real cues)", () => {
    expect(maneuverKindFromType("sharpLeft")).toEqual("left");
  });

  it("sharpRight still maps to right ManeuverKind", () => {
    expect(maneuverKindFromType("sharpRight")).toEqual("right");
  });
});

describe("CueEngine — slight turn cue policy", () => {
  it("pre-latches turn50m when first tick is within 50m of a slight turn", () => {
    // 240m - 200m = 40m from maneuver. Case B pre-latches 50m since d ≤ 50.
    // On second tick (same progress), turn50m is suppressed by the pre-latch.
    const route = baseSnapshot({
      maneuvers: [M_SLIGHT_LEFT("m1", 240)],
      progressDistanceM: 200,
    });
    const t1 = tick(initialCueEngineState(), route);
    const t2 = tick(t1.next, route);
    expect(t2.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });
});

describe("CueEngine — maneuver-kind filter contract (bugs 1, 2, 3)", () => {
  // Bug 1: .straight is not a turn — filing "Next turn in about X meters" /
  // "Follow the route" while the UI shows nothing is the bug to prevent.
  it("returns undefined for straight (bug 1: not a turn)", () => {
    expect(maneuverKindFromType("straight")).toBeUndefined();
  });

  it("returns undefined for depart and arrive (handled by dedicated cues)", () => {
    expect(maneuverKindFromType("depart")).toBeUndefined();
    expect(maneuverKindFromType("arrive")).toBeUndefined();
  });

  // Bug 2: roundabout / merge / ramp are first-class kinds, not generic.
  it("maps roundabout to its own ManeuverKind (bug 2: was generic)", () => {
    expect(maneuverKindFromType("roundabout")).toEqual("roundabout");
  });

  it("maps merge to its own ManeuverKind (bug 2: was generic)", () => {
    expect(maneuverKindFromType("merge")).toEqual("merge");
  });

  it("maps ramp to its own ManeuverKind (bug 2: was generic)", () => {
    expect(maneuverKindFromType("ramp")).toEqual("ramp");
  });

  // Bug 3: keepLeft / keepRight removed entirely from ManeuverKind. The
  // TypeScript compiler enforces this — no runtime test needed for the
  // type-level guarantee. Unknown types now silence rather than coerce
  // to "generic" (the old bug-2 noise source).
  it("returns undefined for unknown types (no silent generic coercion)", () => {
    expect(maneuverKindFromType("zigzag")).toBeUndefined();
    expect(maneuverKindFromType("")).toBeUndefined();
  });
});

describe("CueEngine — first-class roundabout / merge / ramp cues (bug 2)", () => {
  it("formats roundabout phrases", () => {
    expect(formatCueEvent({ kind: "turn50m", turnKind: "roundabout", distanceM: 50 })).toBe(
      "In 50 meters, enter the roundabout",
    );
    expect(formatCueEvent({ kind: "turn10m", turnKind: "roundabout" })).toBe(
      "Enter the roundabout",
    );
    expect(
      formatCueEvent({ kind: "nextTurnInAbout", turnKind: "roundabout", distanceM: 200 }),
    ).toBe("Next roundabout in about 200 meters");
  });

  it("formats merge phrases", () => {
    expect(formatCueEvent({ kind: "turn50m", turnKind: "merge", distanceM: 50 })).toBe(
      "In 50 meters, merge",
    );
    expect(formatCueEvent({ kind: "turn10m", turnKind: "merge" })).toBe("Merge");
    expect(formatCueEvent({ kind: "nextTurnInAbout", turnKind: "merge", distanceM: 200 })).toBe(
      "Next merge in about 200 meters",
    );
  });

  it("formats ramp phrases", () => {
    expect(formatCueEvent({ kind: "turn50m", turnKind: "ramp", distanceM: 50 })).toBe(
      "In 50 meters, take the ramp",
    );
    expect(formatCueEvent({ kind: "turn10m", turnKind: "ramp" })).toBe("Take the ramp");
    expect(formatCueEvent({ kind: "nextTurnInAbout", turnKind: "ramp", distanceM: 200 })).toBe(
      "Next ramp in about 200 meters",
    );
  });
});

describe("CueEngine — bug 4: arrived flag suppresses arrivingInM same tick", () => {
  it("does not emit arrivingInM when arrived is true on the same tick", () => {
    // After-passing block would fire arrivingInM (rider passed the only
    // maneuver, no follow-up). On the same tick mark arrived = true —
    // only the dedicated `arrived` cue should fire, not both.
    const m1 = M_LEFT("m1", 400);
    let s = initialCueEngineState();
    s = tick(
      s,
      baseSnapshot({ progressDistanceM: 100, maneuvers: [m1], routeTotalDistanceM: 600 }),
    ).next;
    const r = tick(
      s,
      baseSnapshot({
        progressDistanceM: 595,
        maneuvers: [m1],
        arrived: true,
        routeTotalDistanceM: 600,
      }),
    );
    expect(r.events.find((e) => e.kind === "arrivingInM")).toBeUndefined();
    expect(r.events.find((e) => e.kind === "arrived")).toBeDefined();
  });
});

describe("CueEngine — bug 5: back-to-back pair under 15m at route start emits combined cue", () => {
  it("emits combined turn50m on first tick when first turn < 15m and follow-up within 30m", () => {
    // Route starts 10m before m1; m2 follows 15m later (within 30m back-to-back
    // threshold). Without the fix, the regular 50m block gates on d > 15m and
    // never fires — only `turn10m(m1)` plays and the rider misses m2.
    const m1: CueManeuver = { id: "m1", kind: "right", distanceFromStartM: 10 };
    const m2: CueManeuver = { id: "m2", kind: "left", distanceFromStartM: 25 };
    const r = tick(
      initialCueEngineState(),
      baseSnapshot({ progressDistanceM: 0, maneuvers: [m1, m2], routeTotalDistanceM: 1000 }),
    );
    const combined = r.events.find(
      (e) => e.kind === "turn50m" && (e as { followUpKind?: string }).followUpKind !== undefined,
    ) as
      | { kind: "turn50m"; turnKind: string; distanceM: number; followUpKind?: string }
      | undefined;
    expect(combined).toBeDefined();
    expect(combined?.turnKind).toBe("right");
    expect(combined?.followUpKind).toBe("left");
    expect(combined?.distanceM).toBeCloseTo(10, 1);
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
      s = tick(
        s,
        baseSnapshot({ rerouting: false, offRoute: false, distanceFromRouteM: 0, routeId: "r2" }),
      ).next;
    }
    // After reset, a new rerouting episode should fire again
    const t = tick(s, reroutingSnapshot(true, "r3"));
    expect(t.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });
});

describe("CueEngine — silence during rerouting", () => {
  it("rerouting suppresses turn50m", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 100 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 155, rerouting: true }));
    expect(t2.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });

  it("rerouting suppresses turn10m", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 155 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 192, rerouting: true }));
    expect(t2.events.find((e) => e.kind === "turn10m")).toBeUndefined();
  });

  it("rerouting suppresses nextTurnInAbout", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 200 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 211, rerouting: true }));
    expect(t2.events.find((e) => e.kind === "nextTurnInAbout")).toBeUndefined();
  });

  it("rerouting suppresses arrivingInM", () => {
    const snap = baseSnapshot({
      progressDistanceM: 412,
      maneuvers: [{ id: "m1", kind: "left", distanceFromStartM: 400 }],
      routeTotalDistanceM: 600,
      rerouting: true,
    });
    const t = tick(initialCueEngineState(), snap);
    expect(t.events.find((e) => e.kind === "arrivingInM")).toBeUndefined();
  });

  it("rerouting suppresses turn10m for slight turns", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({
      progressDistanceM: 100,
      maneuvers: [{ id: "m1", kind: "slightLeft", distanceFromStartM: 200 }],
    }));
    const t2 = tick(t1.next, baseSnapshot({
      progressDistanceM: 192,
      maneuvers: [{ id: "m1", kind: "slightLeft", distanceFromStartM: 200 }],
      rerouting: true,
    }));
    expect(t2.events.find((e) => e.kind === "turn50m")).toBeUndefined();
    expect(t2.events.find((e) => e.kind === "turn10m")).toBeUndefined();
  });

  it("rerouting cue itself still fires", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ offRoute: true }));
    const t2 = tick(t1.next, baseSnapshot({ offRoute: true, rerouting: true }));
    expect(t2.events.find((e) => e.kind === "rerouting")).toBeDefined();
  });

  it("rerouting does NOT suppress arrived", () => {
    const t = tick(initialCueEngineState(), baseSnapshot({ arrived: true, rerouting: true }));
    expect(t.events.find((e) => e.kind === "arrived")).toBeDefined();
  });

  it("when rerouting becomes false, cues resume", () => {
    // Rerouting was true, then new route arrives → first-tick announcement fires
    let s = tick(initialCueEngineState(), baseSnapshot({
      offRoute: true,
      rerouting: true,
    })).next;
    // New route, rerouting false → first-tick nextTurnInAbout fires
    const t = tick(s, baseSnapshot({
      routeId: "r2",
      progressDistanceM: 0,
      offRoute: false,
      rerouting: false,
      maneuvers: [{ id: "m1", kind: "left", distanceFromStartM: 200 }],
    }));
    expect(t.events.find((e) => e.kind === "nextTurnInAbout")).toBeDefined();
  });
});

describe("distanceCueValues — km formatting", () => {
  it("returns kilometers for 1310m metric", () => {
    const result = distanceCueValues(1310, "metric");
    expect(result.distance).toBeCloseTo(1.3, 1);
    expect(result.distanceUnit).toBe("kilometers");
  });

  it("returns meters for 500m metric", () => {
    const result = distanceCueValues(500, "metric");
    expect(result.distance).toBe(500);
    expect(result.distanceUnit).toBe("meters");
  });

  it("returns kilometers for 1000m metric", () => {
    const result = distanceCueValues(1000, "metric");
    expect(result.distance).toBeCloseTo(1.0, 1);
    expect(result.distanceUnit).toBe("kilometers");
  });

  it("returns 1.5 km for 1490m metric", () => {
    const result = distanceCueValues(1490, "metric");
    expect(result.distance).toBeCloseTo(1.5, 1);
    expect(result.distanceUnit).toBe("kilometers");
  });

  it("imperial still uses feet for large distances", () => {
    const result = distanceCueValues(2000, "imperial");
    expect(result.distanceUnit).toBe("feet");
  });

  it("formatCueEvent uses km for nextTurnInAbout at 1310m", () => {
    const text = formatCueEvent({ kind: "nextTurnInAbout", turnKind: "left", distanceM: 1310 });
    expect(text).toBe("Next turn left in about 1.3 kilometers");
  });
});
