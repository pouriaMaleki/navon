import { beforeEach, describe, expect, it } from "vitest";
import type { CoordinatePoint } from "../../domain/models.js";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
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
    (
      guidance as unknown as { enterAlternativesExploration: () => void }
    ).enterAlternativesExploration();

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
    (
      guidance as unknown as { enterAlternativesExploration: () => void }
    ).enterAlternativesExploration();

    expect(guidance.homeMode).toBe("planning");
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
    ).toBe(false);
  });

  it("cancelAlternativesExploration clears the flag and keeps homeMode = phoneGuidance", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    (
      guidance as unknown as { enterAlternativesExploration: () => void }
    ).enterAlternativesExploration();
    expect(
      (guidance as unknown as { isExploringAlternativesFromGuidance?: boolean })
        .isExploringAlternativesFromGuidance,
    ).toBe(true);

    // RED: method doesn't exist
    (
      guidance as unknown as { cancelAlternativesExploration: () => void }
    ).cancelAlternativesExploration();

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

  it("enterAlternativesExploration sets compassMode to northLocked", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    expect(guidance.compassMode).toBe("autoFollow");

    guidance.enterAlternativesExploration();

    expect(
      guidance.compassMode,
      "entering alternatives must switch to northLocked for route overview",
    ).toBe("northLocked");
  });

  it("cancelAlternativesExploration restores compassMode to autoFollow", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    guidance.enterAlternativesExploration();
    expect(guidance.compassMode).toBe("northLocked");

    guidance.cancelAlternativesExploration();

    expect(
      guidance.compassMode,
      "cancelling must restore autoFollow so camera follows the rider",
    ).toBe("autoFollow");
  });

  it("selectedAlternativeIDForDisplay is undefined on enter exploration before any tap", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    guidance.enterAlternativesExploration();

    expect(
      guidance.selectedAlternativeIDForDisplay,
      "no alternative row must show a checkmark on enter — the 'Continue' button marks the active route",
    ).toBeUndefined();
  });

  it("selectedAlternativeIDForDisplay shows checkmark on alternative tapped during exploration", () => {
    const { guidance, planning } = buildHarness();
    guidance.startSelectedRoute();
    // Load a second alternative so there's something to tap
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
        {
          id: "a2",
          title: "Route 2",
          subtitle: "",
          distanceMeters: 3000,
          durationSeconds: 700,
          normalizedPackage: { ...straightLinePackage(), routeIdentifier: "osm-alt" },
        },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.enterAlternativesExploration();
    expect(guidance.selectedAlternativeIDForDisplay).toBeUndefined();

    guidance.selectAlternativeForExploration("a2");

    expect(
      guidance.selectedAlternativeIDForDisplay,
      "tapping an alternative during exploration must show its checkmark",
    ).toBe("a2");
  });

  it("selectedAlternativeIDForDisplay returns the planning-selected id outside exploration", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();

    expect(
      guidance.selectedAlternativeIDForDisplay,
      "outside exploration the selected alternative id must be visible",
    ).toBe("a1");
  });

  it("guidanceRoute stays stable during exploration when planning preview is replaced", () => {
    const { guidance, planning } = buildHarness();
    guidance.startSelectedRoute();
    const routeIdentifierBefore = guidance.guidanceRoute?.routeIdentifier;
    expect(routeIdentifierBefore).toBe("osm-straight");

    guidance.enterAlternativesExploration();
    // Simulate async plan returning a completely different set of alternatives
    planning.setPreview({
      alternatives: [
        {
          id: "new-alt",
          title: "New Route",
          subtitle: "",
          distanceMeters: 3500,
          durationSeconds: 800,
          normalizedPackage: { ...straightLinePackage(), routeIdentifier: "osm-new-plan" },
        },
      ],
      selectedAlternativeID: "new-alt",
    });

    expect(
      guidance.guidanceRoute?.routeIdentifier,
      "guidanceRoute must be frozen to the active ride route during exploration",
    ).toBe(routeIdentifierBefore);
  });

  it("guidanceAlternatives returns planning alternatives during exploration", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    guidance.enterAlternativesExploration();

    const alts = guidance.guidanceAlternatives;
    expect(alts.length, "must expose at least the one seeded alternative").toBeGreaterThan(0);
  });

  it("guidanceAlternatives is empty outside of exploration", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();

    expect(guidance.guidanceAlternatives).toHaveLength(0);
  });

  // ── deselectForExploration regression tests ────────────────────────────────

  it("deselectForExploration clears explorationSelectedID so Continue gets the checkmark again", () => {
    const { guidance, planning } = buildHarness();
    guidance.startSelectedRoute();
    planning.setPreview({
      alternatives: [
        { id: "a1", title: "R1", subtitle: "", distanceMeters: 2500, durationSeconds: 600, normalizedPackage: straightLinePackage() },
        { id: "a2", title: "R2", subtitle: "", distanceMeters: 3000, durationSeconds: 700, normalizedPackage: { ...straightLinePackage(), routeIdentifier: "osm-alt" } },
      ],
      selectedAlternativeID: "a1",
    });
    guidance.enterAlternativesExploration();
    guidance.selectAlternativeForExploration("a2");
    expect(guidance.selectedAlternativeIDForDisplay).toBe("a2");

    guidance.deselectForExploration();

    expect(
      guidance.selectedAlternativeIDForDisplay,
      "deselectForExploration must clear explorationSelectedID so Continue shows the checkmark",
    ).toBeUndefined();
  });

  it("deselectForExploration outside exploration is a no-op", () => {
    const { guidance } = buildHarness();
    guidance.startSelectedRoute();
    expect(guidance.selectedAlternativeIDForDisplay).toBe("a1");

    guidance.deselectForExploration();

    expect(
      guidance.selectedAlternativeIDForDisplay,
      "deselectForExploration outside exploration must not affect the planning-selected id",
    ).toBe("a1");
  });

  // ── double-checkmark regression ────────────────────────────────────────────

  it("selectedAlternativeIDForDisplay is undefined on exploration entry (no Continue double-checkmark)", () => {
    const { guidance, planning } = buildHarness();
    guidance.startSelectedRoute();
    planning.setPreview({
      alternatives: [
        { id: "a1", title: "R1", subtitle: "", distanceMeters: 2500, durationSeconds: 600, normalizedPackage: straightLinePackage() },
      ],
      selectedAlternativeID: "a1",
    });

    guidance.enterAlternativesExploration();

    expect(
      guidance.selectedAlternativeIDForDisplay,
      "on enter exploration selectedAlternativeIDForDisplay must be undefined — Continue shows the only checkmark",
    ).toBeUndefined();
  });

  it("startSelectedRoute clears isExploringAlternativesFromGuidance", () => {
    const { guidance, planning } = buildHarness();
    guidance.startSelectedRoute();
    (
      guidance as unknown as { enterAlternativesExploration: () => void }
    ).enterAlternativesExploration();
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
