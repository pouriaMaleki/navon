import { describe, expect, it } from "vitest";
import type { RouteAlternative } from "../domain/models.js";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../domain/models.js";
import { mergeMixedAlternatives } from "./PlanningStore.js";

function makeAlternative(opts: {
  id: string;
  duration: number;
  distance: number;
  provider: "hsl" | "osm";
  maneuverCount: number;
}): RouteAlternative {
  return {
    id: opts.id,
    title: opts.id,
    subtitle: "",
    distanceMeters: opts.distance,
    durationSeconds: opts.duration,
    normalizedPackage: {
      version: CURRENT_ROUTE_PACKAGE_VERSION,
      routeIdentifier: opts.id,
      revision: 1,
      geometry: [
        { latitude: 60.0, longitude: 24.0 },
        { latitude: 60.1, longitude: 24.1 },
      ],
      maneuvers: new Array(opts.maneuverCount).fill(null).map((_, i) => ({
        id: `m-${i}`,
        maneuverType: "straight" as const,
        location: { latitude: 60.05, longitude: 24.05 },
        distanceFromStartMeters: i * 100,
      })),
      summary: { totalDistanceMeters: opts.distance, estimatedDurationSeconds: opts.duration },
      provenance: {
        providerID: opts.provider,
        sourceReference: "test",
        generatedAtUnixMs: 0,
      },
    },
  };
}

describe("mergeMixedAlternatives", () => {
  it("picks fastest first, prefers OSM next, then any remaining", () => {
    const alternatives = [
      makeAlternative({
        id: "a",
        duration: 600,
        distance: 2000,
        provider: "hsl",
        maneuverCount: 5,
      }),
      makeAlternative({
        id: "b",
        duration: 700,
        distance: 2200,
        provider: "osm",
        maneuverCount: 3,
      }),
      makeAlternative({
        id: "c",
        duration: 800,
        distance: 2400,
        provider: "hsl",
        maneuverCount: 4,
      }),
    ];
    const merged = mergeMixedAlternatives(alternatives);
    expect(merged).toHaveLength(3);
    expect(merged[0].normalizedPackage.routeIdentifier).toBe("a");
    expect(merged[1].normalizedPackage.routeIdentifier).toBe("b");
    // iOS-parity engine-name titles (no per-provider counter). With the
    // fakes' empty `sourceReference`, OSM falls back to "OSM Route" and
    // HSL alternatives that lack the "fastest" hint fall back to
    // "HSL Route".
    expect(merged[0].title).toBe("HSL Route");
    expect(merged[1].title).toBe("OSM Route");
    expect(merged[2].title).toBe("HSL Route");
  });

  it("returns empty list for empty input", () => {
    expect(mergeMixedAlternatives([])).toEqual([]);
  });
});
