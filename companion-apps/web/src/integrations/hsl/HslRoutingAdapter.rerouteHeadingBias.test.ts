import { describe, expect, it, vi } from "vitest";
import type { ActiveRouteSession, CoordinatePoint } from "../../domain/models.js";
import { OsmCyclingRoutingAdapter } from "../osm/OsmCyclingRoutingAdapter.js";
import { HslRoutingAdapter } from "./HslRoutingAdapter.js";

const RIDER: CoordinatePoint = { latitude: 60.17, longitude: 24.94 };
const DEST: CoordinatePoint = { latitude: 60.175, longitude: 24.95 };
const SESSION: ActiveRouteSession = {
  routeIdentifier: "r1",
  routeRevision: 1,
  destinationLabel: "Dest",
  destinationCoordinate: DEST,
  providerID: "osm",
  sourceMode: "osm",
};

const MOCK_HSL_RESPONSE = {
  data: {
    plan: {
      itineraries: [
        {
          duration: 360,
          legs: [
            {
              mode: "BICYCLE",
              distance: 1000,
              from: { lat: RIDER.latitude, lon: RIDER.longitude, name: "Start" },
              to: { lat: DEST.latitude, lon: DEST.longitude, name: "End" },
              legGeometry: {
                points: "oytqH_kjPyGjEoIpDyI|DcNzIw@oEr@",
              },
            },
          ],
        },
      ],
    },
  },
};

function hslAdapter(): HslRoutingAdapter {
  return new HslRoutingAdapter(() => ({
    hslEndpointURL: "http://localhost:3001/api/hsl/routing",
    cyclingSpeedKph: 18,
    speedUnit: "kph",
    ridingZoom: null,
    keepScreenOn: false,
    allowBackgroundGps: false,
    audioCuesEnabled: true,
    audioCuesOnlyInBackground: true,
    liveActivityEnabled: false,
    routingDiagnosticsEnabled: false,
    language: "en",
    distanceUnit: "metric",
  }));
}

describe("reroute heading bias", () => {
  it("applies forward-shifted origin for OSM when heading is confident", async () => {
    const adapter = new OsmCyclingRoutingAdapter();
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    const preview = await adapter.replanRoute(SESSION, RIDER, {
      headingDegrees: 90,
      speedMps: 4.0,
    });
    fetchSpy.mockRestore();
    const route = preview.alternatives[0]?.normalizedPackage;
    expect(route).toBeDefined();
    const r = route as NonNullable<typeof route>;
    expect(r.geometry[0]).not.toEqual(RIDER);
  });

  it("falls back to legacy origin for OSM when speed is low", async () => {
    const adapter = new OsmCyclingRoutingAdapter();
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    const preview = await adapter.replanRoute(SESSION, RIDER, {
      headingDegrees: 90,
      speedMps: 0.5,
    });
    fetchSpy.mockRestore();
    const route = preview.alternatives[0]?.normalizedPackage;
    expect(route).toBeDefined();
    const r = route as NonNullable<typeof route>;
    expect(r.geometry[0]).toEqual(RIDER);
  });

  it("applies forward-shifted origin for HSL reroute when heading is confident", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(MOCK_HSL_RESPONSE)),
    );
    const adapter = hslAdapter();
    const preview = await adapter.replanRoute(
      { ...SESSION, providerID: "hsl", sourceMode: "hsl" },
      RIDER,
      { headingDegrees: 90, speedMps: 4.0 },
    );
    vi.restoreAllMocks();
    const route = preview.alternatives[0]?.normalizedPackage;
    expect(route).toBeDefined();
    const r = route as NonNullable<typeof route>;
    expect(r.geometry[0]).not.toEqual(RIDER);
  });
});
