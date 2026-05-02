import { beforeEach, describe, expect, it } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import type { CoordinatePoint } from "../../domain/models.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../fakes/index.js";

const HELSINKI: CoordinatePoint = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST: CoordinatePoint = { latitude: 60.1921, longitude: 24.9458 };

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
  planning.routeRequest = {
    ...planning.routeRequest,
    origin: HELSINKI,
    destination: HELSINKI_DEST,
  };
  const guidance = new GuidanceStore(planning, persistence, location);
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
  return { guidance, planning };
}

describe("explore alternatives from guidance (split-icon UX)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("isExploringAlternativesFromGuidance defaults to false", () => {
    const { guidance } = buildHarness();
    // RED: property doesn't exist yet on GuidanceStore
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
      "GuidanceStore must expose isExploringAlternativesFromGuidance, defaulting to false",
    ).toBe(false);
  });

  it("enterAlternativesExploration sets the flag and does NOT change homeMode", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    expect(guidance.homeMode).toBe("phoneGuidance");

    // RED: method doesn't exist
    (guidance as unknown as { enterAlternativesExploration: () => void })
      .enterAlternativesExploration();

    expect(
      guidance.homeMode,
      "enterAlternativesExploration must NOT drop homeMode to planning — guidance keeps running",
    ).toBe("phoneGuidance");
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
      "enterAlternativesExploration must set isExploringAlternativesFromGuidance = true",
    ).toBe(true);
  });

  it("enterAlternativesExploration outside phoneGuidance is a no-op", () => {
    const { guidance } = buildHarness();
    // In planning mode — should be a no-op
    (guidance as unknown as { enterAlternativesExploration: () => void })
      .enterAlternativesExploration();

    expect(guidance.homeMode).toBe("planning");
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
    ).toBe(false);
  });

  it("cancelAlternativesExploration clears the flag and keeps homeMode = phoneGuidance", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    (guidance as unknown as { enterAlternativesExploration: () => void })
      .enterAlternativesExploration();
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
    ).toBe(true);

    // RED: method doesn't exist
    (guidance as unknown as { cancelAlternativesExploration: () => void })
      .cancelAlternativesExploration();

    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
      "cancel must clear the exploring flag",
    ).toBe(false);
    expect(
      guidance.homeMode,
      "cancel must keep homeMode as phoneGuidance — the original route is still active",
    ).toBe("phoneGuidance");
  });

  it("startSelectedRoute clears isExploringAlternativesFromGuidance", () => {
    const { guidance, planning } = buildHarness();
    guidance.startSelectedRoute();
    (guidance as unknown as { enterAlternativesExploration: () => void })
      .enterAlternativesExploration();
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
    ).toBe(true);

    // Add an alternative to select, then start
    planning.setPreview({
      alternatives: [
        {
          id: "a2",
          title: "Alt Route",
          subtitle: "",
          distanceMeters: 3000,
          durationSeconds: 700,
          normalizedPackage: {
            ...straightLinePackage(),
            routeIdentifier: "osm-alt",
          },
        },
      ],
      selectedAlternativeID: "a2",
    });
    guidance.startSelectedRoute();

    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
      "startSelectedRoute must clear isExploringAlternativesFromGuidance — user committed to a new route",
    ).toBe(false);
  });
});
