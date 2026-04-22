import { beforeEach, describe, expect, it } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import {
  FakeLocationService,
  FakePlaceSearch,
  FakeRoutingAdapter,
} from "../fakes/index.js";

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };
// A point roughly 350 m south of HELSINKI — far enough to clear the
// 35 m off-route ENTER distance used by GuidanceStore.
const OFF_ROUTE_POINT = { latitude: 60.1668, longitude: 24.9384 };

function straightLinePackage() {
  return {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: "osm-straight",
    revision: 1,
    geometry: [HELSINKI, HELSINKI_DEST],
    maneuvers: [
      {
        id: "m1",
        maneuverType: "depart" as const,
        location: HELSINKI,
        distanceFromStartMeters: 0,
      },
      {
        id: "m2",
        maneuverType: "arrive" as const,
        location: HELSINKI_DEST,
        distanceFromStartMeters: 2500,
      },
    ],
    summary: { totalDistanceMeters: 2500, estimatedDurationSeconds: 600 },
    provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
  };
}

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
  planning.routeRequest = { ...planning.routeRequest, origin: HELSINKI, destination: HELSINKI_DEST };
  return { planning, guidance };
}

describe("routing session (plan flows #43, #44, #61, #62)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("select_route_then_start_enters_routing (flow #43)", () => {
    const { planning, guidance } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "Route 1",
          subtitle: "",
          distanceMeters: 2500,
          durationSeconds: 600,
          normalizedPackage: straightLinePackage(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    expect(guidance.homeMode).toBe("phoneGuidance");
    expect(guidance.activeSession.routeIdentifier).toBe("osm-straight");
  });

  it("stop_returns_to_planning (flow #44)", () => {
    const { planning, guidance } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "Route 1",
          subtitle: "",
          distanceMeters: 2500,
          durationSeconds: 600,
          normalizedPackage: straightLinePackage(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    guidance.stopGuidance();
    expect(guidance.homeMode).toBe("planning");
  });

  it("off_route_detection_reroute_trigger (flow #61): sustained deviation flips rerouteRequested", () => {
    const { planning, guidance } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "Route 1",
          subtitle: "",
          distanceMeters: 2500,
          durationSeconds: 600,
          normalizedPackage: straightLinePackage(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    // Drive advanceProgress from an on-route start, then jump off.
    let t = 0;
    guidance.advanceProgress(HELSINKI, t);
    t += 500;
    guidance.advanceProgress(OFF_ROUTE_POINT, t);
    expect(guidance.offRoute).toBe(true);
    // Sustain the off-route condition past REROUTE_REQUEST_DELAY_MS (2000 ms).
    t += 2500;
    guidance.advanceProgress(OFF_ROUTE_POINT, t);
    expect(guidance.rerouteRequested).toBe(true);
  });

  it("rerouting_loading_indicator (flow #62): offRouteLabel surfaces 'Rerouting…' while requested", () => {
    const { planning, guidance } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "Route 1",
          subtitle: "",
          distanceMeters: 2500,
          durationSeconds: 600,
          normalizedPackage: straightLinePackage(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    let t = 0;
    guidance.advanceProgress(HELSINKI, t);
    t += 500;
    guidance.advanceProgress(OFF_ROUTE_POINT, t);
    t += 2500;
    guidance.advanceProgress(OFF_ROUTE_POINT, t);
    expect(guidance.offRouteLabel).toBe("Rerouting…");
  });

  it("returning to route clears rerouteRequested", () => {
    const { planning, guidance } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "Route 1",
          subtitle: "",
          distanceMeters: 2500,
          durationSeconds: 600,
          normalizedPackage: straightLinePackage(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    let t = 0;
    guidance.advanceProgress(HELSINKI, t);
    t += 500;
    guidance.advanceProgress(OFF_ROUTE_POINT, t);
    t += 2500;
    guidance.advanceProgress(OFF_ROUTE_POINT, t);
    expect(guidance.rerouteRequested).toBe(true);
    t += 500;
    guidance.advanceProgress(HELSINKI, t);
    expect(guidance.rerouteRequested).toBe(false);
    expect(guidance.offRoute).toBe(false);
  });
});
