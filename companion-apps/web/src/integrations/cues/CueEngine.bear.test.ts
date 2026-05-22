import { describe, expect, it } from "vitest";
import {
  type CueEngineState,
  type CueEvent,
  type CueManeuver,
  type CueSnapshot,
  initialCueEngineState,
  tickCueEngine,
} from "./CueEngine.js";

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
const M_RIGHT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "right",
  distanceFromStartM: distance,
});

const baseSnapshot = (overrides: Partial<CueSnapshot> = {}): CueSnapshot => ({
  routeId: "r1",
  pairedWithDevice: false,
  progressDistanceM: 0,
  maneuvers: [M_SLIGHT_LEFT("m1", 200)],
  offRoute: false,
  rerouting: false,
  arrived: false,
  distanceFromRouteM: 0,
  routeTotalDistanceM: 1000,
  ...overrides,
});

function tick(s: CueEngineState, snap: CueSnapshot): { events: CueEvent[]; next: CueEngineState } {
  const r = tickCueEngine(snap, s);
  return { events: r.events, next: r.nextState };
}

describe("CueEngine — slight turns are first-class cues", () => {
  it("slightLeft fires nextTurnInAbout on route start when >50m away", () => {
    const t = tick(
      initialCueEngineState(),
      baseSnapshot({
        maneuvers: [M_SLIGHT_LEFT("m1", 200)],
        progressDistanceM: 0,
      }),
    );
    const a = t.events.find((e) => e.kind === "nextTurnInAbout");
    expect(a).toBeDefined();
    expect(a).toMatchObject({ turnKind: "slightLeft" });
  });

  it("slightRight fires turn50m approach when rider enters 50m window", () => {
    const route = baseSnapshot({
      maneuvers: [M_SLIGHT_RIGHT("m2", 300)],
      routeTotalDistanceM: 500,
    });
    const t1 = tick(initialCueEngineState(), route);
    const t2 = tick(t1.next, { ...route, progressDistanceM: 255 });
    const turn50 = t2.events.find((e) => e.kind === "turn50m");
    expect(turn50).toBeDefined();
    expect(turn50).toMatchObject({ turnKind: "slightRight" });
  });

  it("slightLeft fires turn10m when rider is within 15m", () => {
    const route = baseSnapshot({ maneuvers: [M_SLIGHT_LEFT("m3", 100)], routeTotalDistanceM: 500 });
    const t1 = tick(initialCueEngineState(), route);
    const t2 = tick(t1.next, { ...route, progressDistanceM: 90 });
    const turn10 = t2.events.find((e) => e.kind === "turn10m");
    expect(turn10).toBeDefined();
    expect(turn10).toMatchObject({ turnKind: "slightLeft" });
  });

  it("slightLeft back-to-back pair gets combined turn50m", () => {
    // M1 at 40m, M2 at 60m (gap 20m < 30m), rider at 0 → distance 40 ≤ 50 → Case C
    const t = tick(
      initialCueEngineState(),
      baseSnapshot({
        maneuvers: [M_SLIGHT_LEFT("m1", 40), M_RIGHT("m2", 60)],
        progressDistanceM: 0,
      }),
    );
    const turn50 = t.events.find((e) => e.kind === "turn50m");
    expect(turn50).toBeDefined();
    expect(turn50).toMatchObject({ turnKind: "slightLeft", followUpKind: "right" });
  });

  it("slightLeft near route end substitutes arrivingInM", () => {
    const t = tick(
      initialCueEngineState(),
      baseSnapshot({
        maneuvers: [M_SLIGHT_LEFT("last", 480)],
        progressDistanceM: 0,
        routeTotalDistanceM: 500,
      }),
    );
    expect(t.events.find((e) => e.kind === "arrivingInM")).toBeDefined();
  });

  it("slightRight turn10m is suppressed when arrived", () => {
    const t = tick(
      initialCueEngineState(),
      baseSnapshot({
        maneuvers: [M_SLIGHT_RIGHT("m1", 50)],
        progressDistanceM: 40,
        arrived: true,
      }),
    );
    expect(t.events.find((e) => e.kind === "turn10m")).toBeUndefined();
    expect(t.events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });
});
