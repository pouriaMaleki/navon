import { describe, expect, it, vi } from "vitest";

import {
  buildDemoRouteSync,
  buildDemoRerouteGeometry,
  buildDemoRerouteSync,
  computeRejoinGeometryIndex,
  inferTurnManeuverType,
  maybeBuildRouteSync,
  persistSpeedUnit,
  readRouteAlertVerbosity,
  readStoredSpeedUnit,
} from "./wasmProgram";

describe("wasm speed unit persistence", () => {
  it("reads a stored speed unit when present", () => {
    const storage = {
      getItem: vi.fn(() => "mph"),
    };

    expect(readStoredSpeedUnit(storage)).toBe("mph");
  });

  it("ignores invalid stored speed unit values", () => {
    const storage = {
      getItem: vi.fn(() => "knots"),
    };

    expect(readStoredSpeedUnit(storage)).toBeUndefined();
  });

  it("persists the selected speed unit", () => {
    const storage = {
      setItem: vi.fn(),
    };

    persistSpeedUnit(storage, "kph");

    expect(storage.setItem).toHaveBeenCalledWith("esp32-minimap.speed-unit", "kph");
  });
});


describe("route alert verbosity selection", () => {
  it("reads a valid route alert verbosity from query params", () => {
    expect(readRouteAlertVerbosity("?routeAlerts=essential")).toBe("essential");
    expect(readRouteAlertVerbosity("?routeAlerts=detailed")).toBe("detailed");
  });

  it("ignores invalid route alert verbosity values", () => {
    expect(readRouteAlertVerbosity("?routeAlerts=loud")).toBeUndefined();
    expect(readRouteAlertVerbosity("")).toBeUndefined();
  });
});


describe("demo route maneuver generation", () => {
  it("infers the reroute finish bends as left turns", () => {
    const geometry = [
      { latDeg: 60.1751, lonDeg: 24.9420 },
      { latDeg: 60.1753, lonDeg: 24.94212 },
      { latDeg: 60.17542, lonDeg: 24.94306 },
      { latDeg: 60.17592, lonDeg: 24.94308 },
      { latDeg: 60.17616, lonDeg: 24.94278 },
      { latDeg: 60.1761, lonDeg: 24.94248 },
    ];

    expect(inferTurnManeuverType(geometry, 3)).toBe("slight_left");
    expect(inferTurnManeuverType(geometry, 4)).toBe("left");
  });

  it("rejoins from the next route point when the rider is closer to it", () => {
    const routeSync = buildDemoRouteSync();
    if (routeSync.type !== "set") {
      throw new Error(`Expected set route sync, received ${routeSync.type}`);
    }

    const rider = { latDeg: 60.175248, lonDeg: 24.942352 };
    const rejoinIndex = computeRejoinGeometryIndex(routeSync.route.geometry, rider);
    expect(rejoinIndex).toBe(4);

    const rerouteGeometry = buildDemoRerouteGeometry({
      ...rider,
      speedMps: 6,
      courseRad: null,
      horizontalAccuracyM: 3,
    });
    expect(rerouteGeometry[0]).toEqual(rider);
    expect(rerouteGeometry[1]).toEqual(routeSync.route.geometry[4]);

    const reroute = buildDemoRerouteSync({
      ...rider,
      speedMps: 6,
      courseRad: null,
      horizontalAccuracyM: 3,
    });
    if (reroute.type !== "update") {
      throw new Error(`Expected update route sync, received ${reroute.type}`);
    }

    expect(reroute.route.geometry[1]).toEqual(routeSync.route.geometry[4]);
    expect(reroute.route.maneuvers.at(0)?.maneuverType).toBe("depart");
    expect(reroute.route.maneuvers.at(-1)?.maneuverType).toBe("arrive");
  });

  it("keeps the main demo route maneuver coverage aligned to geometry", () => {
    const routeSync = buildDemoRouteSync();
    if (routeSync.type !== "set") {
      throw new Error(`Expected set route sync, received ${routeSync.type}`);
    }

    expect(routeSync.route.maneuvers.map((maneuver) => maneuver.maneuverType)).toEqual([
      "depart",
      "right",
      "left",
      "left",
      "right",
      "right",
      "left",
      "left",
      "right",
      "arrive",
    ]);
  });
});


it("allows a second reroute after the first reroute cycle clears", () => {
  const gps = {
    latDeg: 60.1751,
    lonDeg: 24.9420,
    speedMps: 6,
    courseRad: null,
    horizontalAccuracyM: 3,
  };
  const state: {
    custom: {
      routeSeeded: boolean;
      rerouteApplied: boolean;
      reroutePendingSinceMs: number | null;
      frame: { routeRerouteRequested: boolean } | null;
    };
    time: { totalMs: number };
  } = {
    custom: {
      routeSeeded: true,
      rerouteApplied: false,
      reroutePendingSinceMs: null,
      frame: { routeRerouteRequested: true },
    },
    time: { totalMs: 1_000 },
  };

  expect(maybeBuildRouteSync(state as never, gps)).toBeNull();
  expect(state.custom.reroutePendingSinceMs).toBe(1_000);

  state.time.totalMs = 2_000;
  const firstReroute = maybeBuildRouteSync(state as never, gps);
  if (firstReroute?.type !== "update") {
    throw new Error(`Expected first reroute update, received ${firstReroute?.type ?? "null"}`);
  }
  expect(state.custom.rerouteApplied).toBe(true);

  state.custom.frame = { routeRerouteRequested: false };
  expect(maybeBuildRouteSync(state as never, gps)).toBeNull();
  expect(state.custom.rerouteApplied).toBe(false);
  expect(state.custom.reroutePendingSinceMs).toBeNull();

  state.custom.frame = { routeRerouteRequested: true };
  state.time.totalMs = 3_000;
  expect(maybeBuildRouteSync(state as never, gps)).toBeNull();
  expect(state.custom.reroutePendingSinceMs).toBe(3_000);

  state.time.totalMs = 4_000;
  const secondReroute = maybeBuildRouteSync(state as never, gps);
  if (secondReroute?.type !== "update") {
    throw new Error(`Expected second reroute update, received ${secondReroute?.type ?? "null"}`);
  }
  expect(state.custom.rerouteApplied).toBe(true);
});
