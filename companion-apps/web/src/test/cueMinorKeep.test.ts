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

const M_MINOR_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "left",
  distanceFromStartM: distance,
  isMinorKeep: true,
});

const M_REGULAR_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "left",
  distanceFromStartM: distance,
});

function snap(overrides: Partial<CueSnapshot> = {}): CueSnapshot {
  return {
    routeId: "r1",
    pairedWithDevice: false,
    progressDistanceM: 0,
    maneuvers: [M_MINOR_LEFT("m1", 200)],
    offRoute: false,
    rerouting: false,
    arrived: false,
    distanceFromRouteM: 0,
    routeTotalDistanceM: 1000,
    ...overrides,
  };
}

function tick(state: CueEngineState, snapshot: CueSnapshot) {
  return tickCueEngine(snapshot, state);
}

function hasTurnCue(events: CueEvent[]): boolean {
  return events.some((e) => e.kind === "turn10m" || e.kind === "turn50m");
}

describe("CueEngine — minor keep always suppressed", () => {
  it("does NOT emit turn10m for a minor-keep at close range", () => {
    // Rider at 195m, minor-keep maneuver at 200m = 5m away
    // This would previously trigger urgency escalation, now suppressed
    const s = snap({ maneuvers: [M_MINOR_LEFT("m1", 200)], progressDistanceM: 195 });
    const { events } = tick(initialCueEngineState(), s);
    expect(hasTurnCue(events)).toBe(false);
  });

  it("does NOT emit turn50m for a minor-keep at mid range", () => {
    // Rider at 160m, maneuver at 200m = 40m away
    const s = snap({ maneuvers: [M_MINOR_LEFT("m1", 200)], progressDistanceM: 160 });
    const { events } = tick(initialCueEngineState(), s);
    expect(hasTurnCue(events)).toBe(false);
  });

  it("does NOT emit nextTurnInAbout for minor-keep after passing another turn", () => {
    // Rider at 110m, just passed m1 at 100m (10m past it), m2 is minor-keep at 200m
    const s = snap({
      maneuvers: [M_REGULAR_LEFT("m1", 100), M_MINOR_LEFT("m2", 200)],
      progressDistanceM: 110,
    });
    const { events } = tick(initialCueEngineState(), s);
    const nextTurnCues = events.filter((e) => e.kind === "nextTurnInAbout");
    expect(nextTurnCues).toHaveLength(0);
  });

  it("still emits turn cues for regular (non-minor) maneuvers", () => {
    // Rider at 195m, regular maneuver at 200m = 5m
    const s = snap({ maneuvers: [M_REGULAR_LEFT("m1", 200)], progressDistanceM: 195 });
    const { events } = tick(initialCueEngineState(), s);
    expect(hasTurnCue(events)).toBe(true);
  });
});
