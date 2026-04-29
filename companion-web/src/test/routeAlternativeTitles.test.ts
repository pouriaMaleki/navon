import { describe, expect, it } from "vitest";
import {
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type RouteAlternative,
} from "../domain/models.js";
import { friendlyAlternativeLabel } from "../stores/PlanningStore.js";

const start: CoordinatePoint = { latitude: 60.17, longitude: 24.94 };
const end: CoordinatePoint = { latitude: 60.18, longitude: 24.95 };

function alt(providerID: "osm" | "hsl" | "gpx", sourceReference: string | undefined): RouteAlternative {
  return {
    id: `id-${sourceReference ?? "anon"}`,
    title: "x",
    subtitle: "y",
    distanceMeters: 1000,
    durationSeconds: 300,
    normalizedPackage: {
      version: CURRENT_ROUTE_PACKAGE_VERSION,
      routeIdentifier: "rid",
      revision: 1,
      geometry: [start, end],
      maneuvers: [],
      summary: { totalDistanceMeters: 1000, estimatedDurationSeconds: 300, destinationLabel: "Park" },
      provenance: { providerID, sourceReference, generatedAtUnixMs: 0 },
    },
  };
}

describe("friendlyAlternativeLabel — iOS-parity rename", () => {
  // Mirrors `RouteAlternativeTitlesTests` on iOS: the suggested-routes
  // card was crowded with redundant "OSM Route 1 / via …" rows. The new
  // scheme drops the per-provider counter and uses the underlying engine
  // name, with no subtitle.

  it("BRouter fastbike → 'BRouter fastbike' with empty subtitle", () => {
    expect(friendlyAlternativeLabel(alt("osm", "BRouter fastbike"))).toEqual({
      title: "BRouter fastbike",
      subtitle: "",
    });
  });

  it("BRouter trekking → 'BRouter trekking'", () => {
    expect(friendlyAlternativeLabel(alt("osm", "BRouter trekking"))).toEqual({
      title: "BRouter trekking",
      subtitle: "",
    });
  });

  it("OSRM bike → 'OSM Route'", () => {
    expect(friendlyAlternativeLabel(alt("osm", "OSRM bike"))).toEqual({
      title: "OSM Route",
      subtitle: "",
    });
  });

  it("HSL Digitransit live / fastest → 'HSL Fastest' (no subtitle)", () => {
    expect(friendlyAlternativeLabel(alt("hsl", "HSL Digitransit live / fastest"))).toEqual({
      title: "HSL Fastest",
      subtitle: "",
    });
  });

  it("HSL Digitransit live / alternative → 'HSL Route'", () => {
    expect(friendlyAlternativeLabel(alt("hsl", "HSL Digitransit live / alternative"))).toEqual({
      title: "HSL Route",
      subtitle: "",
    });
  });
});
