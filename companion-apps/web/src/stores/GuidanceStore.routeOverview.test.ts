import { beforeEach, describe, expect, it } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../domain/models.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "./GuidanceStore.js";
import { LocationStore } from "./LocationStore.js";
import { PlanningStore, type ProvidersMap } from "./PlanningStore.js";
import { SettingsStore } from "./SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../__testlib__/fakes/index.js";

const ORIGIN = { latitude: 60.1699, longitude: 24.9384 };
const QUARTER = { latitude: 60.17545, longitude: 24.93915 };
const HALF = { latitude: 60.181, longitude: 24.9399 };
const DEST = { latitude: 60.1921, longitude: 24.9458 };

function buildGuidance() {
  globalThis.localStorage?.clear();
  const persistence = new LocalStoragePersistence();
  const settings = new SettingsStore(persistence);
  const location = new LocationStore(new FakeLocationService(), persistence);
  const providers: ProvidersMap = {
    hsl: new FakeRoutingAdapter("hsl"),
    osm: new FakeRoutingAdapter("osm"),
    gpxImport: new FakeRoutingAdapter("gpxImport"),
    fitImport: new FakeRoutingAdapter("fitImport"),
    tcxImport: new FakeRoutingAdapter("tcxImport"),
  } as ProvidersMap;
  const planning = new PlanningStore(providers, new FakePlaceSearch(), location, settings);
  const guidance = new GuidanceStore(planning, persistence, location);
  planning.routeRequest = { ...planning.routeRequest, origin: ORIGIN, destination: DEST };
  planning.setPreview({
    alternatives: [
      {
        id: "a1",
        title: "Route 1",
        subtitle: "",
        distanceMeters: 4000,
        durationSeconds: 800,
        normalizedPackage: {
          version: CURRENT_ROUTE_PACKAGE_VERSION,
          routeIdentifier: "osm-overview",
          revision: 1,
          geometry: [ORIGIN, QUARTER, HALF, DEST],
          maneuvers: [
            {
              id: "m1",
              maneuverType: "depart" as const,
              location: ORIGIN,
              distanceFromStartMeters: 0,
            },
            {
              id: "m2",
              maneuverType: "arrive" as const,
              location: DEST,
              distanceFromStartMeters: 4000,
            },
          ],
          summary: { totalDistanceMeters: 4000, estimatedDurationSeconds: 800 },
          provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
        },
      },
    ],
    selectedAlternativeID: "a1",
  });
  guidance.startSelectedRoute();
  return guidance;
}

// Why existing tests didn't cover this: cameraModes.test.ts confirmed that a
// compass-tap fired the fit-route intent, but never inspected which subset
// of the geometry should be fitted. The full polyline was always used, so
// late in a ride the camera zoomed out unnecessarily to include long-completed
// segments behind the rider.
describe("route overview camera fits the remaining route only", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("returns the full geometry before any progress", () => {
    const guidance = buildGuidance();
    expect(guidance.routeOverviewGeometry?.length).toBe(4);
  });

  it("drops the completed prefix once the rider has moved past it", () => {
    const guidance = buildGuidance();
    // Push the rider to roughly the half-way vertex; the overview geometry
    // should now exclude the first quarter and the start.
    guidance.advanceProgress(HALF, 1000);
    const overview = guidance.routeOverviewGeometry ?? [];
    expect(overview.length).toBeLessThanOrEqual(3);
    // The last point is always still the destination.
    const last = overview[overview.length - 1];
    expect(last.latitude).toBeCloseTo(DEST.latitude, 4);
    expect(last.longitude).toBeCloseTo(DEST.longitude, 4);
    // The very first vertex of the original polyline must NOT be there.
    expect(
      overview.some((p) => p.latitude === ORIGIN.latitude && p.longitude === ORIGIN.longitude),
    ).toBe(false);
  });
});
