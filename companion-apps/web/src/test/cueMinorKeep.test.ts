import { describe, expect, it } from "vitest";
import {
  type CueEngineState,
  type CueEvent,
  type CueManeuver,
  type CueSnapshot,
  initialCueEngineState,
  tickCueEngine,
} from "../integrations/cues/CueEngine.js";

const M_SLIGHT_LEFT = (id: string, distance: number): CueManeuver => ({
  id, kind: "slightLeft", distanceFromStartM: distance,
});
const M_SLIGHT_RIGHT = (id: string, distance: number): CueManeuver => ({
  id, kind: "slightRight", distanceFromStartM: distance,
});
const M_LEFT = (id: string, distance: number): CueManeuver => ({
  id, kind: "left", distanceFromStartM: distance,
});

function base(overrides: Partial<CueSnapshot> = {}): CueSnapshot {
  return {
    routeId: "r1", pairedWithDevice: false, progressDistanceM: 0,
    maneuvers: [M_LEFT("m1", 200)], offRoute: false, rerouting: false,
    arrived: false, distanceFromRouteM: 0, routeTotalDistanceM: 1000, ...overrides,
  };
}
function tick(s: CueEngineState, snap: CueSnapshot): { events: CueEvent[]; next: CueEngineState } {
  const r = tickCueEngine(snap, s);
  return { events: r.events, next: r.nextState };
}

describe("CueEngine — slight turns produce standard approach cues", () => {
  it("slightLeft fires turn50m at 40m distance (no pre-latch, distance > 100m on first tick)", () => {
    const route = base({ maneuvers: [M_SLIGHT_LEFT("m1", 240)] });
    // First tick at 0: nextTurnInAbout at 240m (>100m, no skip-50m pre-latch)
    const t1 = tick(initialCueEngineState(), route);
    // Second tick at 200m: 40m from maneuver → in 50m window → turn50m fires
    const t2 = tick(t1.next, { ...route, progressDistanceM: 200 });
    expect(t2.events.find((e) => e.kind === "turn50m")).toBeDefined();
  });

  it("slightRight fires turn10m at 8m distance", () => {
    const route = base({ maneuvers: [M_SLIGHT_RIGHT("m1", 108)] });
    // First tick: 108m → Case A, no skip-50m pre-latch (108 > 100)
    const t1 = tick(initialCueEngineState(), route);
    // Second tick at 100m: 8m from maneuver → turn10m fires
    const t2 = tick(t1.next, { ...route, progressDistanceM: 100 });
    expect(t2.events.find((e) => e.kind === "turn10m")).toBeDefined();
  });
});
