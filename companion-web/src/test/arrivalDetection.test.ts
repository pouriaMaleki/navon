import { beforeEach, describe, expect, it } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../domain/models.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../stores/GuidanceStore.js";
import { LocationStore } from "../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../stores/PlanningStore.js";
import { SettingsStore } from "../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "./fakes/index.js";

const ORIGIN = { latitude: 60.1699, longitude: 24.9384 };
const DEST = { latitude: 60.1721, longitude: 24.9404 }; // ~280 m NE

function buildHarness() {
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
        distanceMeters: 280,
        durationSeconds: 60,
        normalizedPackage: {
          version: CURRENT_ROUTE_PACKAGE_VERSION,
          routeIdentifier: "osm-arrival",
          revision: 1,
          geometry: [ORIGIN, DEST],
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
              distanceFromStartMeters: 280,
            },
          ],
          summary: {
            totalDistanceMeters: 280,
            estimatedDurationSeconds: 60,
            destinationLabel: "Ensi linja 1",
          },
          provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
        },
      },
    ],
    selectedAlternativeID: "a1",
  });
  guidance.startSelectedRoute();
  return { planning, guidance };
}

// Why existing tests didn't cover this: routingSession.test.ts only tested
// stop on user button press and on off-route reroute — never the
// "approached the destination" terminal state. Riders manually closed
// guidance after arriving.
describe("arrival detection (rider close to destination ends guidance)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("snaps the homeMode back to planning when within ARRIVAL_RADIUS_M of the destination", () => {
    const { guidance } = buildHarness();
    expect(guidance.homeMode).toBe("phoneGuidance");
    // Walk the rider to within 10 m of the destination.
    const arrivalPoint = { latitude: DEST.latitude - 0.00005, longitude: DEST.longitude };
    guidance.advanceProgress(arrivalPoint, 1000);
    expect(guidance.homeMode).toBe("planning");
    expect(guidance.arrivalNotice).toBe("Arrived at destination");
  });

  it("stays in guidance when far from the destination", () => {
    const { guidance } = buildHarness();
    // Halfway along the route; not arrived.
    const midpoint = { latitude: 60.171, longitude: 24.9394 };
    guidance.advanceProgress(midpoint, 1000);
    expect(guidance.homeMode).toBe("phoneGuidance");
    expect(guidance.arrivalNotice).toBeUndefined();
  });

  it("clears the search query and route preview on arrival so no phantom polyline or stale destination remains", () => {
    // After arriving the "Where to?" field and route alternatives must be
    // wiped so the map shows a blank planning state. Previously
    // declareArrival() only stopped guidance; planning.query and
    // planning.preview were never cleared.
    const { guidance, planning } = buildHarness();
    void planning.updateQuery("Ensi linja 1");
    expect(planning.preview.alternatives.length).toBeGreaterThan(0);

    const arrivalPoint = { latitude: DEST.latitude - 0.00005, longitude: DEST.longitude };
    guidance.advanceProgress(arrivalPoint, 1000);

    expect(planning.query).toBe("");
    expect(planning.preview.alternatives).toHaveLength(0);
  });
});
