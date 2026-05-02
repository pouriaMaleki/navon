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
