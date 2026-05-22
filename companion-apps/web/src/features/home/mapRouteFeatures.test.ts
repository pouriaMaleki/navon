/**
 * Tests for buildRouteFeatures — the function that converts store state into
 * GeoJSON features for the map. These tests close the gap between "the store
 * exposes guidanceAlternatives" and "the map actually renders them".
 */
import { describe, expect, it } from "vitest";
import type { RootStore } from "../../app/RootStore.js";
import type { CoordinatePoint } from "../../domain/models.js";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import { buildRouteFeatures } from "./mapRouteFeatures.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../../__testlib__/fakes/index.js";

const HELSINKI: CoordinatePoint = { latitude: 60.1699, longitude: 24.9384 };
const ESPOO: CoordinatePoint = { latitude: 60.2055, longitude: 24.6559 };

function pkg(routeIdentifier = "osm-straight") {
  return {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier,
    revision: 1,
    geometry: [HELSINKI, ESPOO],
    maneuvers: [
      { id: "m1", maneuverType: "depart" as const, location: HELSINKI, distanceFromStartMeters: 0 },
      { id: "m2", maneuverType: "arrive" as const, location: ESPOO, distanceFromStartMeters: 5000 },
    ],
    summary: { totalDistanceMeters: 5000, estimatedDurationSeconds: 900 },
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
  // Minimal stub that satisfies the two fields buildRouteFeatures reads from RootStore
  const store = { planningStore: planning, guidanceStore: guidance } as unknown as RootStore;
  return { guidance, planning, store };
}

describe("buildRouteFeatures — exploration rendering", () => {
  it("renders the active route as the selected (yellow) feature when no alternative is tapped", () => {
    const { guidance, planning, store } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "R1",
          subtitle: "",
          distanceMeters: 5000,
          durationSeconds: 900,
          normalizedPackage: pkg(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    planning.setPreview({
      alternatives: [
        {
          id: "b1",
          title: "Alt1",
          subtitle: "",
          distanceMeters: 4000,
          durationSeconds: 800,
          normalizedPackage: pkg("alt-1"),
        },
        {
          id: "b2",
          title: "Alt2",
          subtitle: "",
          distanceMeters: 6000,
          durationSeconds: 1000,
          normalizedPackage: pkg("alt-2"),
        },
      ],
      selectedAlternativeID: "b1",
    });
    guidance.enterAlternativesExploration();

    const features = buildRouteFeatures(store);

    // Active route must be selected (yellow); no alternative should be selected
    const selectedFeatures = features.filter((f) => f.properties?.["selected"] === true);
    const altFeatures = features.filter(
      (f) => f.properties?.["selected"] !== true && !f.properties?.["completed"],
    );
    expect(selectedFeatures).toHaveLength(1);
    expect(altFeatures).toHaveLength(2);
    expect(selectedFeatures[0]?.geometry).toMatchObject({
      type: "LineString",
      coordinates: expect.arrayContaining([[HELSINKI.longitude, HELSINKI.latitude]]),
    });
  });

  it("renders the tapped alternative as selected (yellow) and the active route as non-selected (teal)", () => {
    const { guidance, planning, store } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "R1",
          subtitle: "",
          distanceMeters: 5000,
          durationSeconds: 900,
          normalizedPackage: pkg(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    planning.setPreview({
      alternatives: [
        {
          id: "b1",
          title: "Alt1",
          subtitle: "",
          distanceMeters: 4000,
          durationSeconds: 800,
          normalizedPackage: pkg("alt-1"),
        },
        {
          id: "b2",
          title: "Alt2",
          subtitle: "",
          distanceMeters: 6000,
          durationSeconds: 1000,
          normalizedPackage: pkg("alt-2"),
        },
      ],
      selectedAlternativeID: "b1",
    });
    guidance.enterAlternativesExploration();
    guidance.selectAlternativeForExploration("b2");

    const features = buildRouteFeatures(store);

    const selectedFeatures = features.filter((f) => f.properties?.["selected"] === true);
    const nonSelectedFeatures = features.filter(
      (f) => f.properties?.["selected"] !== true && !f.properties?.["completed"],
    );
    // Exactly one feature must be "selected": the tapped alternative b2
    expect(selectedFeatures).toHaveLength(1);
    // Active route + untapped b1 are non-selected
    expect(nonSelectedFeatures).toHaveLength(2);
  });

  it("renders all alternatives during exploration (not just the active route)", () => {
    const { guidance, planning, store } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "R1",
          subtitle: "",
          distanceMeters: 5000,
          durationSeconds: 900,
          normalizedPackage: pkg(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();
    planning.setPreview({
      alternatives: [
        {
          id: "b1",
          title: "Alt1",
          subtitle: "",
          distanceMeters: 4000,
          durationSeconds: 800,
          normalizedPackage: pkg("alt-1"),
        },
        {
          id: "b2",
          title: "Alt2",
          subtitle: "",
          distanceMeters: 6000,
          durationSeconds: 1000,
          normalizedPackage: pkg("alt-2"),
        },
      ],
      selectedAlternativeID: "b1",
    });
    guidance.enterAlternativesExploration();

    const features = buildRouteFeatures(store);

    // 1 active route + 2 alternatives = 3 polyline features
    const lineFeatures = features.filter((f) => !f.properties?.["completed"]);
    expect(lineFeatures).toHaveLength(3);
  });

  it("normal guidance (not exploring) only renders the route split, not alternatives", () => {
    const { guidance, planning, store } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "a1",
          title: "R1",
          subtitle: "",
          distanceMeters: 5000,
          durationSeconds: 900,
          normalizedPackage: pkg(),
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.startSelectedRoute();

    const features = buildRouteFeatures(store);

    // routeSplit exists so should render the remaining segment only (no alternatives)
    const nonCompleted = features.filter((f) => !f.properties?.["completed"]);
    expect(nonCompleted).toHaveLength(1);
    expect(features.some((f) => f.properties?.["id"] === "a1")).toBe(false);
  });
});
