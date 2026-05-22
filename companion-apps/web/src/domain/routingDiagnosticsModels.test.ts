import { describe, expect, it } from "vitest";
import {
  newSessionId,
  type RouteGeometryEntry,
  type RoutingDiagSession,
  sessionDebugPackage,
} from "./routingDiagnosticsModels.js";

function emptySession(): RoutingDiagSession {
  return {
    id: newSessionId(),
    createdAtMs: 1700000000000,
    updatedAtMs: 1700000001000,
    events: [],
  };
}

const sampleGeom: RouteGeometryEntry = {
  routeId: "r1",
  providerName: "digitransit",
  geometry: [
    { latitude: 60.1699, longitude: 24.9384 },
    { latitude: 60.1709, longitude: 24.9484 },
  ],
};

describe("sessionDebugPackage", () => {
  it("includes events and metadata", () => {
    const session = emptySession();
    const json = sessionDebugPackage(session);
    const pkg = JSON.parse(json);
    expect(pkg.formatVersion).toBe(1);
    expect(pkg.sessionId).toBe(session.id);
    expect(pkg.eventCount).toBe(0);
    expect(pkg.events).toEqual([]);
  });

  it("includes routeGeometries when present", () => {
    const session = { ...emptySession(), routeGeometries: [sampleGeom] };
    const json = sessionDebugPackage(session);
    const pkg = JSON.parse(json);
    expect(pkg.routeGeometries).toHaveLength(1);
    expect(pkg.routeGeometries[0].routeId).toBe("r1");
    expect(pkg.routeGeometries[0].geometry).toHaveLength(2);
  });

  it("omits routeGeometries when empty", () => {
    const session = { ...emptySession(), routeGeometries: [] };
    const json = sessionDebugPackage(session);
    const pkg = JSON.parse(json);
    expect(pkg.routeGeometries).toEqual([]);
  });

  it("serializes geometry coordinates", () => {
    const session = { ...emptySession(), routeGeometries: [sampleGeom] };
    const json = sessionDebugPackage(session);
    const pkg = JSON.parse(json);
    expect(pkg.routeGeometries[0].geometry[0].latitude).toBeCloseTo(60.1699);
    expect(pkg.routeGeometries[0].geometry[0].longitude).toBeCloseTo(24.9384);
  });
});
