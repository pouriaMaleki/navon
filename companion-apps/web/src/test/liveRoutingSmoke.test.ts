import { describe, expect, it } from "vitest";
import type { ActiveRouteSession, CoordinatePoint } from "../domain/models.js";
import { HslRoutingAdapter } from "../integrations/hsl/HslRoutingAdapter.js";
import { OsmCyclingRoutingAdapter } from "../integrations/osm/OsmCyclingRoutingAdapter.js";

const env =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env ?? {};
const LIVE_ENABLED = env.LIVE_ROUTING_SMOKE === "1";

const ORIGIN: CoordinatePoint = { latitude: 60.1708, longitude: 24.9375 };
const DEST: CoordinatePoint = { latitude: 60.1771, longitude: 24.9506 };

const OSM_SESSION: ActiveRouteSession = {
  routeIdentifier: "live-osm",
  routeRevision: 1,
  destinationLabel: "Helsinki center",
  destinationCoordinate: DEST,
  providerID: "osm",
  sourceMode: "osm",
};

const HSL_SESSION: ActiveRouteSession = {
  ...OSM_SESSION,
  routeIdentifier: "live-hsl",
  providerID: "hsl",
  sourceMode: "hsl",
};

const maybeDescribe = LIVE_ENABLED ? describe : describe.skip;

maybeDescribe("live routing smoke (opt-in)", () => {
  it("OSM reroute with heading context returns at least one alternative", async () => {
    const adapter = new OsmCyclingRoutingAdapter();
    const preview = await adapter.replanRoute(OSM_SESSION, ORIGIN, { headingDegrees: 90, speedMps: 4.0 });
    expect(preview.alternatives.length).toBeGreaterThan(0);
    expect(preview.alternatives[0]?.normalizedPackage.geometry.length ?? 0).toBeGreaterThan(1);
  });

  it("HSL reroute with heading context returns at least one alternative", async () => {
    const hslSubscriptionKey = env.HSL_SUBSCRIPTION_KEY ?? "";
    const hslEndpointURL = env.HSL_ENDPOINT_URL ?? "";
    expect(hslSubscriptionKey.length).toBeGreaterThan(0);
    expect(hslEndpointURL.length).toBeGreaterThan(0);
    const adapter = new HslRoutingAdapter(() => ({
      preferLiveHslRouting: true,
      hslSubscriptionKey,
      hslEndpointURL,
      cyclingSpeedKph: 18,
      speedUnit: "kph",
      ridingZoom: null,
      keepScreenOn: false,
      allowBackgroundGps: false,
      audioCuesEnabled: true,
      audioCuesOnlyInBackground: true,
      liveActivityEnabled: false,
      language: "en",
      distanceUnit: "metric",
    }));
    const preview = await adapter.replanRoute(HSL_SESSION, ORIGIN, { headingDegrees: 90, speedMps: 4.0 });
    expect(preview.alternatives.length).toBeGreaterThan(0);
    expect(preview.alternatives[0]?.normalizedPackage.geometry.length ?? 0).toBeGreaterThan(1);
  });
});
