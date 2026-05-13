import { describe, expect, it } from "vitest";
import {
  type CueEngineState,
  type CueManeuver,
  type CueSnapshot,
  cueMessage,
  formatCueEvent,
  initialCueEngineState,
  tickCueEngine,
} from "../integrations/cues/CueEngine.js";
import { maneuverKindFromType } from "../integrations/cues/RoutingActivityCoordinator.js";
import {
  cumulativeDistances,
  findClosestPointIndex,
  maneuverAngleDegrees,
} from "../integrations/geo.js";
import type { CoordinatePoint } from "../domain/models.js";

const M_BEAR_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "bearLeft",
  distanceFromStartM: distance,
});
const M_BEAR_RIGHT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "bearRight",
  distanceFromStartM: distance,
});
const M_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "left",
  distanceFromStartM: distance,
});
const M_SLIGHT_LEFT = (id: string, distance: number): CueManeuver => ({
  id,
  kind: "left",
  distanceFromStartM: distance,
  isMinorKeep: true,
});

function snap(overrides: Partial<CueSnapshot>): CueSnapshot {
  return {
    routeId: "r1",
    pairedWithDevice: false,
    progressDistanceM: 0,
    maneuvers: [M_BEAR_LEFT("m1", 200)],
    offRoute: false,
    rerouting: false,
    arrived: false,
    distanceFromRouteM: 0,
    routeTotalDistanceM: 1000,
    ...overrides,
  };
}

function tick(state: CueEngineState, s: CueSnapshot) {
  return tickCueEngine(s, state);
}

describe("CueEngine — bear range-hold cues", () => {
  it("emits bearRange for bearLeft when entering the segment", () => {
    // Rider at 195m, bear maneuver at 200m = 5m away (inside APPROACH_10_M)
    const s = snap({
      maneuvers: [M_BEAR_LEFT("m1", 200)],
      progressDistanceM: 195,
    });
    const { events } = tick(initialCueEngineState(), s);
    const bear = events.find((e) => e.kind === "bearRange");
    expect(bear).toBeDefined();
    expect(bear).toMatchObject({ kind: "bearRange", turnKind: "bearLeft" });
    // Should NOT emit approach cues
    expect(events.find((e) => e.kind === "turn50m")).toBeUndefined();
    expect(events.find((e) => e.kind === "turn10m")).toBeUndefined();
  });

  it("emits bearRange for bearRight with correct segment length", () => {
    // Bear from 200m to 350m (segment = 150m)
    const s = snap({
      maneuvers: [M_BEAR_RIGHT("m1", 200), M_LEFT("m2", 350)],
      progressDistanceM: 195,
    });
    const { events } = tick(initialCueEngineState(), s);
    const bear = events.find((e) => e.kind === "bearRange");
    expect(bear).toBeDefined();
    expect(bear).toMatchObject({ turnKind: "bearRight" });
    expect((bear as { distanceM: number }).distanceM).toBeCloseTo(150, 0);
  });

  it("emits bearRange even when followed by a close turn", () => {
    // Bear at 200m, regular left at 220m (gap = 20m). Bear should still fire.
    const s = snap({
      maneuvers: [M_BEAR_LEFT("m1", 200), M_LEFT("m2", 220)],
      // Progress at 100 so we're outside Case C's tight-start window.
      // Case A fires nextTurnInAbout first, then second tick enters the segment.
      progressDistanceM: 100,
      routeTotalDistanceM: 500,
    });
    const t1 = tick(initialCueEngineState(), s);
    // Second tick: rider enters the bear segment. Bear-range should fire
    // even though m2 (regular turn) is only 20m ahead.
    const t2 = tick(t1.nextState, { ...s, progressDistanceM: 195 });
    const bear = t2.events.find((e) => e.kind === "bearRange");
    expect(bear).toBeDefined();
    expect(bear).toMatchObject({ turnKind: "bearLeft" });
  });

  it("emits nextTurnInAbout for bearLeft at route start (Case A)", () => {
    // Route starts 0m, first maneuver is bear at 200m (distance > 50m)
    const s = snap({
      maneuvers: [M_BEAR_LEFT("m1", 200)],
      progressDistanceM: 0,
    });
    const { events } = tick(initialCueEngineState(), s);
    const next = events.find((e) => e.kind === "nextTurnInAbout");
    expect(next).toBeDefined();
    expect(next).toMatchObject({ turnKind: "bearLeft" });
  });

  it("does NOT emit approach cues for bearLeft at close range", () => {
    // Rider at 160m, bear at 200m (in 50m window)
    const s = snap({
      maneuvers: [M_BEAR_LEFT("m1", 200)],
      progressDistanceM: 160,
    });
    const init = tick(initialCueEngineState(), s); // first tick (route start)
    const { events } = tick(init.nextState, { ...s, progressDistanceM: 165 });
    expect(events.find((e) => e.kind === "turn50m")).toBeUndefined();
    expect(events.find((e) => e.kind === "turn10m")).toBeUndefined();
  });

  it("unpromoted slightLeft (isMinorKeep) is still silent", () => {
    const s = snap({
      maneuvers: [M_SLIGHT_LEFT("m1", 200)],
      progressDistanceM: 195,
    });
    const { events } = tick(initialCueEngineState(), s);
    expect(events.filter((e) => e.kind === "bearRange")).toHaveLength(0);
    expect(events.find((e) => e.kind === "turn10m")).toBeUndefined();
    expect(events.find((e) => e.kind === "turn50m")).toBeUndefined();
  });
});

describe("cueMessage — bearRange", () => {
  it("generates correct text for bearRange.bearLeft", () => {
    const event = { kind: "bearRange" as const, turnKind: "bearLeft" as const, distanceM: 200 };
    const text = formatCueEvent(event);
    expect(text).toBe("Bear left for the next 200 meters");
  });

  it("generates correct text for bearRange.bearRight", () => {
    const event = { kind: "bearRange" as const, turnKind: "bearRight" as const, distanceM: 150 };
    const text = formatCueEvent(event);
    expect(text).toBe("Bear right for the next 150 meters");
  });

  it("generates nextTurnInAbout text for bearLeft", () => {
    const event = { kind: "nextTurnInAbout" as const, turnKind: "bearLeft" as const, distanceM: 160 };
    const text = formatCueEvent(event);
    expect(text).toBe("Next bear left in about 160 meters");
  });
});

describe("Promotion pipeline: slightLeft + angle → bearLeft", () => {
  it("maneuverKindFromType still maps slightLeft to left with isMinorKeep", () => {
    expect(maneuverKindFromType("slightLeft")).toEqual({ kind: "left", isMinorKeep: true });
    expect(maneuverKindFromType("slightRight")).toEqual({ kind: "right", isMinorKeep: true });
  });

  it("sharpLeft promotes to regular left (not bear)", () => {
    expect(maneuverKindFromType("sharpLeft")).toEqual({ kind: "left" });
    expect(maneuverKindFromType("sharpRight")).toEqual({ kind: "right" });
  });

  it("maneuverAngleDegrees returns shallow angle for a straight path", () => {
    // Nearly-straight path heading south with a ~1m lateral offset over 10m
    const shallowCurve: CoordinatePoint[] = [
      { latitude: 60.200000, longitude: 24.910000 },
      { latitude: 60.199910, longitude: 24.910000 },
      { latitude: 60.199820, longitude: 24.910005 },
    ];
    const cum = cumulativeDistances(shallowCurve);
    const angle = maneuverAngleDegrees(shallowCurve, cum, 1);
    expect(Math.abs(angle)).toBeLessThan(25);
  });

  it("maneuverAngleDegrees returns sharp angle for a real fork", () => {
    // Tight spacing: ~10m segments, then a 45° right turn
    const fork: CoordinatePoint[] = [
      { latitude: 60.200000, longitude: 24.910000 },
      { latitude: 60.199910, longitude: 24.910000 }, // ~10m south
      { latitude: 60.199910, longitude: 24.910030 }, // ~20m east → ~63° right
    ];
    const cum = cumulativeDistances(fork);
    const angle = maneuverAngleDegrees(fork, cum, 1);
    expect(Math.abs(angle)).toBeGreaterThanOrEqual(25);
  });

  it("findClosestPointIndex returns nearest geometry point", () => {
    const geometry: CoordinatePoint[] = [
      { latitude: 60.200, longitude: 24.910 },
      { latitude: 60.199, longitude: 24.910 },
      { latitude: 60.198, longitude: 24.911 },
    ];
    expect(findClosestPointIndex(geometry, geometry[1])).toBe(1);
  });
});
