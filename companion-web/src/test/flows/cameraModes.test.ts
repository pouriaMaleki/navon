import { beforeEach, describe, expect, it, vi } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { MapCameraStore } from "../../stores/MapCameraStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import {
  FakeLocationService,
  FakePlaceSearch,
  FakeRoutingAdapter,
} from "../fakes/index.js";

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

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

function buildGuidanceHarness() {
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
  planning.routeRequest = { ...planning.routeRequest, origin: HELSINKI, destination: HELSINKI_DEST };
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

describe("MapCameraStore basics (plan flows #49)", () => {
  it("setCenter bumps revision and clears needsRecenter", () => {
    const cam = new MapCameraStore();
    cam.markUserMovedAway();
    expect(cam.needsRecenter).toBe(true);
    cam.setCenter(HELSINKI, 14, 0);
    expect(cam.needsRecenter).toBe(false);
    expect(cam.revision).toBeGreaterThanOrEqual(1);
  });

  it("fitBounds stores the coordinates and bumps revision", () => {
    const cam = new MapCameraStore();
    const initialRev = cam.revision;
    cam.fitBounds([HELSINKI, { latitude: 60.19, longitude: 24.94 }]);
    expect(cam.target.kind).toBe("fitBounds");
    expect(cam.revision).toBeGreaterThan(initialRev);
  });

  it("markUserMovedAway flips needsRecenter once (idempotent)", () => {
    const cam = new MapCameraStore();
    cam.markUserMovedAway();
    const rev = cam.revision;
    cam.markUserMovedAway();
    expect(cam.needsRecenter).toBe(true);
    expect(cam.revision).toBe(rev);
  });
});

describe("compass mode state machine (plan flows #44, #45, #59, #60)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("handleCompassTap goes to northPreview during routing", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassTap();
    expect(guidance.compassMode).toBe("northPreview");
  });

  it("handleCompassDoubleTap locks north-up", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassDoubleTap();
    expect(guidance.compassMode).toBe("northLocked");
  });

  it("handleCompassTap when locked returns to autoFollow (flow #45 tail)", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassDoubleTap();
    expect(guidance.compassMode).toBe("northLocked");
    guidance.handleCompassTap();
    expect(guidance.compassMode).toBe("autoFollow");
  });

  it("handleCompassTap outside guidance is a no-op", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.handleCompassTap();
    expect(guidance.compassMode).toBe("autoFollow");
  });
});

describe("companion north-indicator single-tap recenter (plan flow #52)", () => {
  it("GuidanceStore exposes a map-recenter callback that handleCompassTap fires", () => {
    // Spec line 39: "on companion apps [the north indicator] also resets and
    // recenters the camera". Today `GuidanceStore.handleCompassTap` only
    // mutates `compassMode` and has no path into `MapCameraStore`. Expected
    // RED until a named recenter callback (or similar signal) lands.
    //
    // We assert on a specific API shape so the test can't be "fixed" by
    // sprinkling an unrelated field onto the store — the callback has to
    // genuinely exist and be invoked.
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    let recenterCallCount = 0;
    const storeAny = guidance as unknown as {
      onRecenterRequested?: (callback: () => void) => void;
    };
    storeAny.onRecenterRequested?.(() => {
      recenterCallCount += 1;
    });
    guidance.handleCompassTap();
    expect(
      recenterCallCount,
      "compass-tap handler must invoke the GuidanceStore recenter callback — today GuidanceStore has no such hook",
    ).toBe(1);
  });
});

describe("camera rider anchor in routing (plan flow #54)", () => {
  it("camera_rider_bottom_quarter_when_routing: MapCameraStore exposes a rider-anchor observable", () => {
    // Spec lines 84, 89: in routing mode the rider is pinned to the bottom
    // quarter of the screen. The only way the app can enforce this without
    // the MapSurface knowing about guidance state is for MapCameraStore (or
    // the store graph) to expose a `riderAnchorNormalizedY` the surface
    // reads. Today MapCameraStore has no such field — expected RED until a
    // named anchor observable exists.
    const cam = new MapCameraStore();
    const camAny = cam as unknown as { riderAnchorNormalizedY?: number };
    expect(
      camAny.riderAnchorNormalizedY,
      "MapCameraStore should expose riderAnchorNormalizedY so the MapSurface can honour spec line 84 (rider in bottom quarter when routing)",
    ).toBeDefined();
  });
});
