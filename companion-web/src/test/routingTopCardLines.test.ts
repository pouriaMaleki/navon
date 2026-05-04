import { beforeEach, describe, expect, it } from "vitest";
import {
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
} from "../domain/models.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../stores/GuidanceStore.js";
import { LocationStore } from "../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../stores/PlanningStore.js";
import { SettingsStore } from "../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "./fakes/index.js";

const HELSINKI: CoordinatePoint = { latitude: 60.17, longitude: 24.94 };
const METERS_PER_DEG_LAT = 111_320.0;

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
  return { planning, guidance };
}

function tinyRoute(destinationLabel: string | undefined, totalKm: number): NormalizedRoutePackage {
  const end: CoordinatePoint = {
    latitude: HELSINKI.latitude + (totalKm * 1000) / METERS_PER_DEG_LAT,
    longitude: HELSINKI.longitude,
  };
  return {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: "lines-test",
    revision: 1,
    geometry: [HELSINKI, end],
    maneuvers: [
      {
        id: "depart",
        maneuverType: "depart",
        location: HELSINKI,
        distanceFromStartMeters: 0,
        distanceToNextMeters: totalKm * 1000,
      },
      {
        id: "arrive",
        maneuverType: "arrive",
        location: end,
        distanceFromStartMeters: totalKm * 1000,
      },
    ],
    summary: {
      totalDistanceMeters: totalKm * 1000,
      estimatedDurationSeconds: 960,
      destinationLabel,
    },
    provenance: { providerID: "osm", generatedAtUnixMs: 0 },
  };
}

function setPreview(planning: PlanningStore, pkg: NormalizedRoutePackage) {
  planning.preview = {
    alternatives: [
      {
        id: "a",
        title: "T",
        subtitle: "",
        distanceMeters: pkg.summary.totalDistanceMeters,
        durationSeconds: pkg.summary.estimatedDurationSeconds,
        normalizedPackage: pkg,
      },
    ],
    selectedAlternativeID: "a",
    routeIdentifier: pkg.routeIdentifier,
    routeRevision: pkg.revision,
  };
}

describe("GuidanceStore — three-line routing top card (iOS parity)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("distanceToDestinationLine reads 'X km to <destination>'", () => {
    const { planning, guidance } = buildHarness();
    setPreview(planning, tinyRoute("Alppila", 8.6));
    guidance.startSelectedRoute();
    expect(guidance.distanceToDestinationLine).toBe("8.6 km to Alppila");
  });

  it("minutesRemainingLine reads 'Y min remaining'", () => {
    const { planning, guidance } = buildHarness();
    setPreview(planning, tinyRoute("Alppila", 8.6));
    guidance.startSelectedRoute();
    expect(guidance.minutesRemainingLine).toBe("16 min remaining");
  });

  it("falls back to activeSession.destinationLabel when the route's label is the placeholder", () => {
    const { planning, guidance } = buildHarness();
    setPreview(planning, tinyRoute("Selected destination", 3.4));
    // Stamp the user-typed name on the active session before Start, mirroring
    // the production flow (`applySelectedAlternativeToSession`).
    guidance.activeSession = { ...guidance.activeSession, destinationLabel: "Kallio" };
    guidance.startSelectedRoute();
    expect(guidance.distanceToDestinationLine).toBe("3.4 km to Kallio");
  });

  it("drops the 'to <name>' suffix when there is no usable destination label", () => {
    const { planning, guidance } = buildHarness();
    setPreview(planning, tinyRoute("No destination", 8.6));
    guidance.activeSession = { ...guidance.activeSession, destinationLabel: "" };
    guidance.startSelectedRoute();
    expect(guidance.distanceToDestinationLine).toBe("8.6 km");
  });
});

describe("GuidanceStore — activeNavigationTitle placeholder filtering", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("filters 'Selected destination' from guidanceRoute and falls back to activeSession label", () => {
    const { planning, guidance } = buildHarness();
    setPreview(planning, tinyRoute("Selected destination", 3.4));
    guidance.activeSession = { ...guidance.activeSession, destinationLabel: "Kallio" };
    guidance.startSelectedRoute();
    expect(guidance.activeNavigationTitle).toBe("Kallio");
  });

  it("filters 'Current location' from guidanceRoute and falls back to activeSession label", () => {
    const { planning, guidance } = buildHarness();
    setPreview(planning, tinyRoute("Current location", 3.4));
    guidance.activeSession = { ...guidance.activeSession, destinationLabel: "Alppila" };
    guidance.startSelectedRoute();
    expect(guidance.activeNavigationTitle).toBe("Alppila");
  });

  it("does not show provider-generated route title as destination in distanceToDestinationLine", () => {
    // Regression: before the fix, displayDestinationTitle used "OSM route" as fallback
    // when no real address was known, which bled into distanceToDestinationLine.
    const { planning, guidance } = buildHarness();
    // Simulate the new fallback behaviour: activeSession carries "No destination"
    // (the placeholder) instead of a provider name.
    setPreview(planning, tinyRoute("Selected destination", 8.6));
    guidance.activeSession = { ...guidance.activeSession, destinationLabel: "No destination" };
    guidance.startSelectedRoute();
    // Must NOT show "8.6 km to No destination" or "8.6 km to OSM route"
    expect(guidance.distanceToDestinationLine).toBe("8.6 km");
  });
});

describe("GuidanceStore — selectAlternativeForExploration preserves destinationLabel", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("selectAlternativeForExploration does not overwrite activeSession.destinationLabel", () => {
    // Regression: on Android/iOS, selectAlternativeForExploration previously called
    // selectAlternative which called applySelectedAlternativeToSession, erasing the
    // user-typed address. The web PlanningStore.selectAlternative only updates preview,
    // so this test pins that invariant stays true.
    const { planning, guidance } = buildHarness();
    const pkg1 = tinyRoute("Selected destination", 3.4);
    const pkg2 = tinyRoute("Selected destination", 5.0);
    planning.preview = {
      alternatives: [
        { id: "a", title: "T1", subtitle: "", distanceMeters: 3_400, durationSeconds: 600, normalizedPackage: pkg1 },
        { id: "b", title: "T2", subtitle: "", distanceMeters: 5_000, durationSeconds: 900, normalizedPackage: pkg2 },
      ],
      selectedAlternativeID: "a",
      routeIdentifier: pkg1.routeIdentifier,
      routeRevision: pkg1.revision,
    };
    guidance.activeSession = { ...guidance.activeSession, destinationLabel: "Kallio" };
    guidance.startSelectedRoute();
    guidance.selectAlternativeForExploration("b");
    expect(guidance.activeSession.destinationLabel).toBe("Kallio");
  });
});

describe("GuidanceStore — distance-first next-turn line (iOS parity)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("'17 m Turn right' (distance before instruction)", () => {
    const { planning, guidance } = buildHarness();
    const corner: CoordinatePoint = {
      latitude: HELSINKI.latitude + 400 / METERS_PER_DEG_LAT,
      longitude: HELSINKI.longitude,
    };
    const cosLat = Math.cos((HELSINKI.latitude * Math.PI) / 180);
    const end: CoordinatePoint = {
      latitude: corner.latitude,
      longitude: corner.longitude + 400 / (METERS_PER_DEG_LAT * cosLat),
    };
    const pkg: NormalizedRoutePackage = {
      version: CURRENT_ROUTE_PACKAGE_VERSION,
      routeIdentifier: "lshape",
      revision: 1,
      geometry: [HELSINKI, corner, end],
      maneuvers: [
        {
          id: "m1",
          maneuverType: "depart",
          location: HELSINKI,
          distanceFromStartMeters: 0,
          distanceToNextMeters: 400,
        },
        {
          id: "m2",
          maneuverType: "right",
          location: corner,
          distanceFromStartMeters: 400,
          distanceToNextMeters: 400,
          instructionText: "Turn right",
        },
        { id: "m3", maneuverType: "arrive", location: end, distanceFromStartMeters: 800 },
      ],
      summary: {
        totalDistanceMeters: 800,
        estimatedDurationSeconds: 240,
        destinationLabel: "Park",
      },
      provenance: { providerID: "osm", generatedAtUnixMs: 0 },
    };
    setPreview(planning, pkg);
    guidance.startSelectedRoute();
    // Drive progress to 383 m — 17 m short of the right-turn at 400 m.
    guidance.advanceProgress(
      {
        latitude: HELSINKI.latitude + 383 / METERS_PER_DEG_LAT,
        longitude: HELSINKI.longitude,
      },
      1_000,
    );
    expect(guidance.nextInstructionLine).toBe("17 m Turn right");
  });
});
