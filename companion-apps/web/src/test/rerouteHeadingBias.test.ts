import { describe, expect, it, vi } from "vitest";
import type { ActiveRouteSession, CoordinatePoint } from "../domain/models.js";
import { HslRoutingAdapter } from "../integrations/hsl/HslRoutingAdapter.js";
import { OsmCyclingRoutingAdapter } from "../integrations/osm/OsmCyclingRoutingAdapter.js";

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

  it("applies forward-shifted origin for HSL sample reroute when heading is confident", async () => {
    const adapter = new HslRoutingAdapter(() => ({
      preferLiveHslRouting: false,
      hslSubscriptionKey: "",
      hslEndpointURL: "",
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
    const preview = await adapter.replanRoute(
      { ...SESSION, providerID: "hsl", sourceMode: "hsl" },
      RIDER,
      { headingDegrees: 90, speedMps: 4.0 },
    );
    const route = preview.alternatives[0]?.normalizedPackage;
    expect(route).toBeDefined();
    const r = route as NonNullable<typeof route>;
    expect(r.geometry[0]).not.toEqual(RIDER);
  });
});
