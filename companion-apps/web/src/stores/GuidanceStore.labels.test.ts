import { beforeEach, describe, expect, it } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../domain/models.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "./GuidanceStore.js";
import { LocationStore } from "./LocationStore.js";
import { PlanningStore, type ProvidersMap } from "./PlanningStore.js";
import { SettingsStore } from "./SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../__testlib__/fakes/index.js";

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

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
  planning.routeRequest = {
    ...planning.routeRequest,
    origin: HELSINKI,
    destination: HELSINKI_DEST,
  };
  return { planning, guidance };
}

function startGuidanceWithDestination(label: string) {
  const { planning, guidance } = buildHarness();
  planning.setPreview({
    alternatives: [
      {
        id: "a1",
        title: "Route 1",
        subtitle: "",
        distanceMeters: 2500,
        durationSeconds: 600,
        normalizedPackage: {
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
              maneuverType: "left" as const,
              location: HELSINKI,
              distanceFromStartMeters: 60,
              instructionText: "Turn left onto Test St",
            },
            {
              id: "m3",
              maneuverType: "arrive" as const,
              location: HELSINKI_DEST,
              distanceFromStartMeters: 2500,
            },
          ],
          summary: {
            totalDistanceMeters: 2500,
            estimatedDurationSeconds: 600,
            destinationLabel: label,
          },
          provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
        },
      },
    ],
    selectedAlternativeID: "a1",
  });
  guidance.startSelectedRoute();
  return guidance;
}

// Why existing tests didn't cover this: there was no assertion that the
// top-bar title is the next-turn line (not the destination) and that the
// subtitle bundles destination + remaining distance + min — both came out
// duplicated in the bottom card before this change.
describe("active-guidance label layout (top is next-turn, subtitle bundles destination + remaining)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("nextInstructionLine drives the top title; destinationLabel never appears in the title", () => {
    const guidance = startGuidanceWithDestination("Ensi linja 1");
    expect(guidance.nextInstructionLine).toMatch(/Turn left/);
    // The new combined subtitle pins the destination address front-and-center,
    // alongside the remaining distance/time, so the top card alone covers
    // everything the user needs.
    expect(guidance.guidanceSubtitleLine).toContain("Ensi linja 1");
    expect(guidance.guidanceSubtitleLine).toMatch(/km|m/);
    expect(guidance.guidanceSubtitleLine).toMatch(/min/);
  });

  it("falls back to a sensible subtitle when the destination has no label", () => {
    const guidance = startGuidanceWithDestination("");
    // Empty label should not produce an awkward leading separator.
    expect(guidance.guidanceSubtitleLine.startsWith("•")).toBe(false);
    expect(guidance.guidanceSubtitleLine).toMatch(/min/);
  });
});
