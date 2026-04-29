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

describe("CueEngine — spec lines 133-143", () => {
  it("emits 'route started' on the first tick of a new route", () => {
    const { events } = tick(initialCueEngineState(), baseSnapshot());
    expect(events.map((e) => e.kind)).toContain("routeStarted");
  });

  it("does not emit 'route started' twice for the same route", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot());
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 5 }));
    expect(t2.events.map((e) => e.kind)).not.toContain("routeStarted");
  });

  it("emits 'in 50 meters turn left' when crossing the 50m threshold for a maneuver", () => {
    const before = tick(
      initialCueEngineState(),
      baseSnapshot({ progressDistanceM: 100 }), // distance to m1 = 100
    );
    const after = tick(before.next, baseSnapshot({ progressDistanceM: 155 })); // dist to m1 = 45m
    const turn50 = after.events.find((e) => e.kind === "turn50m");
    expect(turn50).toBeDefined();
    expect(turn50).toMatchObject({ kind: "turn50m", turnKind: "left" });
  });

  it("does not re-emit the 50m cue for the same maneuver on subsequent ticks", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 155 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 160 }));
    expect(t2.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });

  it("emits 'turn left' when crossing the 10m threshold for a maneuver", () => {
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 155 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 192 })); // 8m to m1
    const turn10 = t2.events.find((e) => e.kind === "turn10m");
    expect(turn10).toMatchObject({ kind: "turn10m", turnKind: "left" });
  });

  it("emits 'next turn left in about X meters' 10m past a maneuver, when there is another after", () => {
    // Pass m1 (at 200m). After 10m past it (210m), with m2 still ahead.
    const t1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 200 }));
    const t2 = tick(t1.next, baseSnapshot({ progressDistanceM: 211 })); // 11m past m1
    const next = t2.events.find((e) => e.kind === "nextTurnInAbout");
    expect(next).toBeDefined();
    expect(next).toMatchObject({ turnKind: "left" });
    expect((next as { distanceM: number }).distanceM).toBeCloseTo(189, 0); // 400-211
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
    expect((arr as { distanceM: number }).distanceM).toBeCloseTo(188, 0);
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
    // Episode 1
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    // Episode 2
    s = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 })).next;
    s = tick(s, baseSnapshot({ offRoute: false, distanceFromRouteM: 5 })).next;
    // Episode 3 — silence kicks in
    const t3 = tick(s, baseSnapshot({ offRoute: true, distanceFromRouteM: 40 }));
    expect(t3.events.find((e) => e.kind === "repeatedOffTrackSilence")).toBeDefined();
    s = t3.next;
    // Subsequent ticks while silenced: no cues even if maneuver thresholds would normally fire.
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
    // 4 close samples — not enough yet.
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

  it("resets latches on route id change so cues fire again on a new route", () => {
    const s1 = tick(initialCueEngineState(), baseSnapshot({ progressDistanceM: 155 })).next;
    const s2 = tick(s1, baseSnapshot({ routeId: "r2", progressDistanceM: 155 }));
    expect(s2.events.find((e) => e.kind === "routeStarted")).toBeDefined();
    expect(s2.events.find((e) => e.kind === "turn50m")).toBeDefined();
  });

  it("formatCueEvent renders spec phrases", () => {
    expect(formatCueEvent({ kind: "routeStarted" })).toBe("Route started");
    expect(formatCueEvent({ kind: "turn50m", turnKind: "left" })).toBe("In 50 meters, turn left");
    expect(formatCueEvent({ kind: "turn50m", turnKind: "keepRight" })).toBe(
      "In 50 meters, keep right",
    );
    expect(formatCueEvent({ kind: "turn50m", turnKind: "exitLeft" })).toBe(
      "In 50 meters, take the left exit",
    );
    expect(formatCueEvent({ kind: "turn10m", turnKind: "right" })).toBe("Turn right");
    expect(formatCueEvent({ kind: "nextTurnInAbout", turnKind: "left", distanceM: 187 })).toBe(
      "Next turn left in about 190 meters",
    );
    expect(formatCueEvent({ kind: "arrivingInM", distanceM: 184 })).toBe(
      "Arriving at your destination in 180 meters",
    );
    expect(formatCueEvent({ kind: "arrived" })).toBe("You have arrived at your destination");
    expect(formatCueEvent({ kind: "offTrack" })).toBe("Off track");
    expect(formatCueEvent({ kind: "rerouting" })).toBe("Rerouting");
    expect(formatCueEvent({ kind: "repeatedOffTrackSilence" })).toBe("Off track");
    expect(formatCueEvent({ kind: "onTrack" })).toBe("On track");
  });
});
