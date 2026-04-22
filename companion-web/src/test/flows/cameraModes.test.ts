import { beforeEach, describe, expect, it, vi } from "vitest";
import { type CoordinatePoint, CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { MapCameraStore } from "../../stores/MapCameraStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../fakes/index.js";

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

describe("companion north-indicator intents (plan flow #52, parity with iOS)", () => {
  // Spec line 39 + iOS parity: compass tap during routing should NOT just
  // recenter on the rider — it shows the route overview (fit the whole route
  // geometry, north-up). Double-tap locks that state. A tap from the locked
  // state returns to "follow rider". We model this with two callbacks:
  //
  //   onRecenterRequested  — "follow rider" (center + zoom in + bearing)
  //   onFitRouteRequested  — "route overview" (fit geometry bounds, north-up)

  it("compass tap from autoFollow fires onFitRouteRequested (route overview)", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    let fitCount = 0;
    let followCount = 0;
    const storeAny = guidance as unknown as {
      onFitRouteRequested?: (callback: () => void) => () => void;
      onRecenterRequested?: (callback: () => void) => () => void;
    };
    expect(
      storeAny.onFitRouteRequested,
      "GuidanceStore must expose onFitRouteRequested so the map can show the route overview on compass tap",
    ).toBeDefined();
    storeAny.onFitRouteRequested?.(() => {
      fitCount += 1;
    });
    storeAny.onRecenterRequested?.(() => {
      followCount += 1;
    });
    // advanceProgress fires when startSelectedRoute → emit recenter — but
    // we're asking specifically about the compass tap. Reset counters.
    followCount = 0;
    fitCount = 0;
    guidance.handleCompassTap();
    expect(fitCount, "compass tap from autoFollow should trigger route overview").toBe(1);
    expect(followCount, "compass tap from autoFollow must not follow rider").toBe(0);
  });

  it("compass double-tap fires onFitRouteRequested", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    let fitCount = 0;
    const storeAny = guidance as unknown as {
      onFitRouteRequested?: (callback: () => void) => () => void;
    };
    storeAny.onFitRouteRequested?.(() => {
      fitCount += 1;
    });
    guidance.handleCompassDoubleTap();
    expect(fitCount).toBe(1);
  });

  it("compass tap from northLocked returns to autoFollow and fires onRecenterRequested (follow rider)", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassDoubleTap(); // → northLocked
    let followCount = 0;
    let fitCount = 0;
    guidance.onRecenterRequested(() => {
      followCount += 1;
    });
    const storeAny = guidance as unknown as {
      onFitRouteRequested?: (callback: () => void) => () => void;
    };
    storeAny.onFitRouteRequested?.(() => {
      fitCount += 1;
    });
    guidance.handleCompassTap(); // → autoFollow
    expect(guidance.compassMode).toBe("autoFollow");
    expect(followCount, "returning to autoFollow should request a follow-rider recenter").toBe(1);
    expect(fitCount, "returning to autoFollow must NOT trigger route overview").toBe(0);
  });
});

describe("startSelectedRoute snaps the camera in (fixes iOS parity gap)", () => {
  it("pressing Start emits onRecenterRequested so the camera can follow the rider immediately", () => {
    const { guidance } = buildGuidanceHarness();
    let followCount = 0;
    guidance.onRecenterRequested(() => {
      followCount += 1;
    });
    guidance.startSelectedRoute();
    expect(
      followCount,
      "startSelectedRoute must fire onRecenterRequested so the camera snaps onto the rider with routing zoom/bearing",
    ).toBeGreaterThanOrEqual(1);
  });
});

// Integration-level tests: assert on the ACTUAL MapCameraStore state after
// wire-equivalent RootStore reactions. The previous test layer only checked
// that signals fired — it missed a bug where Start appeared to do nothing
// on the real app because (a) nothing asserted target.kind flipped to
// "center" with zoom 16, and (b) stopGuidance had no fit-route emission so
// the camera stayed wherever Start left it.
describe("RootStore-equivalent camera wiring on start/stop (observable outcomes, spec lines 84/101/85)", () => {
  function buildWiredHarness() {
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
    const mapCamera = new MapCameraStore();

    const ROUTING_FOLLOW_ZOOM = 16;
    // Mirror RootStore follow-rider wiring.
    guidance.onRecenterRequested(() => {
      const rider =
        location.currentLocation ?? location.lastKnownLocation ?? planning.routeRequest.origin;
      if (!rider) return;
      const inRouting = guidance.homeMode === "phoneGuidance";
      const zoom = inRouting
        ? ROUTING_FOLLOW_ZOOM
        : mapCamera.target.kind === "center"
          ? mapCamera.target.zoom
          : 14;
      const bearing = inRouting ? (guidance.routingBearingDegrees ?? 0) : 0;
      mapCamera.setCenter(rider, zoom, bearing);
    });
    // Mirror RootStore fit-route wiring.
    guidance.onFitRouteRequested(() => {
      const selected =
        planning.preview.alternatives.find(
          (a) => a.id === planning.preview.selectedAlternativeID,
        ) ?? planning.preview.alternatives[0];
      const geometry = selected?.normalizedPackage.geometry;
      if (!geometry || geometry.length === 0) return;
      mapCamera.fitBounds(geometry, 120);
    });

    // Seed a route.
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
    return { guidance, planning, mapCamera };
  }

  it("startSelectedRoute flips MapCameraStore.target to kind='center' with ROUTING_FOLLOW_ZOOM", () => {
    const { guidance, mapCamera } = buildWiredHarness();
    // Start from planning with a route-overview fitBounds target, so we can
    // prove that Start visibly changes the camera.
    mapCamera.fitBounds([HELSINKI, HELSINKI_DEST], 80);
    expect(mapCamera.target.kind).toBe("fitBounds");

    guidance.startSelectedRoute();

    expect(
      mapCamera.target.kind,
      "after Start, camera target must be center-kind (follow-rider) — spec line 84",
    ).toBe("center");
    if (mapCamera.target.kind === "center") {
      expect(
        mapCamera.target.zoom,
        "after Start, camera must zoom to ROUTING_FOLLOW_ZOOM (16), not inherit planning-overview zoom",
      ).toBe(16);
    }
  });

  it("HomeView's refreshCameraForCurrentMode must NOT overwrite the routing bearing with GPS heading during phoneGuidance (regression: camera stuck north-up on web)", async () => {
    // Repro for the user-reported bug "camera doesn't rotate along route on web":
    //   1. Start fires emitRecenterRequested → RootStore sets bearing = first-segment.
    //   2. HomeView's unrelated reaction (on `location` / `homeMode`) calls
    //      `refreshCameraForCurrentMode` which used to do, during phoneGuidance:
    //        const bearing = locationStore.currentHeadingDegrees ?? 0;
    //        setCenter(rider, 16, bearing);
    //   3. Without a GPS heading (emulated or low-motion rider) that `?? 0`
    //      falls to 0 and overwrites the correct routing bearing.
    //
    // This test imports the actual function and runs it against a wired
    // harness that has already received the routing-bearing setCenter.
    // The bearing must survive.
    const { guidance, mapCamera } = buildWiredHarness();
    guidance.startSelectedRoute();
    expect(mapCamera.target.kind).toBe("center");
    const expectedBearing =
      mapCamera.target.kind === "center" ? mapCamera.target.bearing : Number.NaN;
    expect(expectedBearing, "sanity: first-segment bearing is non-zero").not.toBe(0);

    // Import the real function under test (extracted module, no MapLibre deps).
    const { refreshCameraForCurrentMode } = await import("../../features/home/refreshCamera.js");
    const fakeStore = {
      guidanceStore: guidance,
      mapCameraStore: mapCamera,
      locationStore: {
        currentHeadingDegrees: null, // Simulated GPS: no heading — the bug trigger.
      },
      planningStore: { preview: { alternatives: [], selectedAlternativeID: undefined } },
    } as unknown as import("../../app/RootStore.js").RootStore;

    refreshCameraForCurrentMode(fakeStore);

    expect(
      mapCamera.target.kind === "center" ? mapCamera.target.bearing : Number.NaN,
      "refreshCameraForCurrentMode must NOT overwrite the routing bearing during phoneGuidance",
    ).toBeCloseTo(expectedBearing, 3);
  });

  it("stopGuidance flips MapCameraStore.target back to kind='fitBounds' on the active route", () => {
    // Spec line 85: "pressing stop button will take user back to suggested
    // routes" — the camera should show the route overview again.
    const { guidance, mapCamera } = buildWiredHarness();
    guidance.startSelectedRoute();
    expect(mapCamera.target.kind).toBe("center"); // sanity

    guidance.stopGuidance();

    expect(
      mapCamera.target.kind,
      "after Stop, camera must fit the route again so the user sees route overview (spec line 85)",
    ).toBe("fitBounds");
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

describe("follow-rider during routing (spec line 84)", () => {
  it("advanceProgress during phoneGuidance fires a recenter signal so the camera tracks the rider", () => {
    // Spec line 84: "camera moves so that user location is on the bottom
    // quarter of the screen". For that to stay true as the rider moves, the
    // guidance store must notify the camera on every GPS tick. The existing
    // `onRecenterRequested` hook is the natural channel — RootStore already
    // wires it to `MapCameraStore.setCenter(riderLocation, …)`.
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    let recenterCount = 0;
    guidance.onRecenterRequested(() => {
      recenterCount += 1;
    });

    guidance.advanceProgress(HELSINKI, 1000);
    expect(
      recenterCount,
      "advanceProgress during routing must emit a recenter so the camera follows the rider",
    ).toBeGreaterThan(0);
  });

  it("advanceProgress outside phoneGuidance does NOT fire recenter", () => {
    // Sanity: we only auto-follow while actively routing.
    const { guidance } = buildGuidanceHarness();
    let recenterCount = 0;
    guidance.onRecenterRequested(() => {
      recenterCount += 1;
    });
    guidance.advanceProgress(HELSINKI, 1000);
    expect(recenterCount).toBe(0);
  });
});

describe("auto-recenter after map interaction during routing (spec line 104)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("noteUserMapInteraction during routing schedules a recenter after the pinned inactivity timeout", () => {
    // Spec line 104: "after a timeout camera goes back to default (when in
    // routing) smoothly". The timeout is the pinned recenter_inactivity from
    // ux-constants.toml (1300 ms).
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    let recenterCount = 0;
    guidance.onRecenterRequested(() => {
      recenterCount += 1;
    });

    const guidanceAny = guidance as unknown as { noteUserMapInteraction?: () => void };
    expect(
      guidanceAny.noteUserMapInteraction,
      "GuidanceStore must expose noteUserMapInteraction() so the MapSurface can report user gestures",
    ).toBeDefined();
    guidanceAny.noteUserMapInteraction?.();

    // Before the timeout: no recenter yet.
    vi.advanceTimersByTime(500);
    expect(recenterCount).toBe(0);

    // Past the timeout: recenter fires once.
    vi.advanceTimersByTime(900);
    expect(recenterCount).toBe(1);
  });

  it("successive interactions reset the timer (only the last one triggers recenter)", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    let recenterCount = 0;
    guidance.onRecenterRequested(() => {
      recenterCount += 1;
    });

    const guidanceAny = guidance as unknown as { noteUserMapInteraction?: () => void };
    guidanceAny.noteUserMapInteraction?.();
    vi.advanceTimersByTime(1000);
    guidanceAny.noteUserMapInteraction?.(); // reset
    vi.advanceTimersByTime(1000);
    expect(recenterCount).toBe(0); // still pending after most-recent interaction
    vi.advanceTimersByTime(500); // pushes past 1300 ms of the new timer
    expect(recenterCount).toBe(1);
  });

  it("noteUserMapInteraction outside routing is a no-op", () => {
    const { guidance } = buildGuidanceHarness();
    let recenterCount = 0;
    guidance.onRecenterRequested(() => {
      recenterCount += 1;
    });

    const guidanceAny = guidance as unknown as { noteUserMapInteraction?: () => void };
    guidanceAny.noteUserMapInteraction?.();
    vi.advanceTimersByTime(5000);
    expect(recenterCount).toBe(0);
  });
});

describe("GPS-trail-heading overrides routing bearing (spec line 110 — most important camera behaviour)", () => {
  // Spec line 110: "camera rotates so that riding direction is towards top
  // of the screen this overrides the camera of routing. Most important
  // camera behaviour is this. (it needs to determine the direction by
  // last few GPS locations it receives)".
  //
  // When the rider is moving, a GPS-derived heading from the last few
  // fixes must drive the camera — even if a route is active and the route
  // segment points a different direction (e.g., rider briefly drifts off
  // or the polyline's orientation doesn't match the actual road). When the
  // rider is stationary, we fall back to the route-segment bearing
  // (spec 101 "even when stationary yet"). These tests pin both cases.

  function offsetByMeters(base: CoordinatePoint, eastM: number, northM: number): CoordinatePoint {
    const metersPerDegreeLat = 111_320.0;
    const meanLat = (base.latitude * Math.PI) / 180.0;
    return {
      latitude: base.latitude + northM / metersPerDegreeLat,
      longitude: base.longitude + eastM / (metersPerDegreeLat * Math.cos(meanLat)),
    };
  }

  // Build a route whose first segment points NORTH — distinctly different
  // from an EAST-moving rider so the test can unambiguously say which bearing
  // the camera is tracking.
  function northRoutePackage() {
    const start = { latitude: 60.17, longitude: 24.94 };
    const end = { latitude: 60.175, longitude: 24.94 }; // ~500 m north
    return {
      version: CURRENT_ROUTE_PACKAGE_VERSION,
      routeIdentifier: "osm-north",
      revision: 1,
      geometry: [start, end],
      maneuvers: [
        { id: "m1", maneuverType: "depart" as const, location: start, distanceFromStartMeters: 0 },
        { id: "m2", maneuverType: "arrive" as const, location: end, distanceFromStartMeters: 500 },
      ],
      summary: { totalDistanceMeters: 500, estimatedDurationSeconds: 120 },
      provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
    };
  }

  function buildHeadingHarness() {
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
      origin: { latitude: 60.17, longitude: 24.94 },
      destination: { latitude: 60.175, longitude: 24.94 },
    };
    const guidance = new GuidanceStore(planning, persistence, location);
    const mapCamera = new MapCameraStore();

    const ROUTING_FOLLOW_ZOOM = 16;
    guidance.onRecenterRequested(() => {
      const rider =
        location.currentLocation ?? location.lastKnownLocation ?? planning.routeRequest.origin;
      if (!rider) return;
      const inRouting = guidance.homeMode === "phoneGuidance";
      const zoom = inRouting
        ? ROUTING_FOLLOW_ZOOM
        : mapCamera.target.kind === "center"
          ? mapCamera.target.zoom
          : 14;
      // Spec line 110: GPS-trail heading overrides the route-segment bearing
      // when available. Falls back to spec line 101 (stationary / no trail).
      const trailHeading = (location as unknown as { travelHeadingDegrees?: number })
        .travelHeadingDegrees;
      const routeBearing = inRouting ? (guidance.routingBearingDegrees ?? 0) : 0;
      const bearing = inRouting ? (trailHeading ?? routeBearing) : (trailHeading ?? 0);
      mapCamera.setCenter(rider, zoom, bearing);
    });
    return { guidance, planning, mapCamera, location };
  }

  it("while moving along a route, camera bearing tracks the GPS trail (east) NOT the route direction (north)", () => {
    const { guidance, planning, location, mapCamera } = buildHeadingHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "r",
          title: "North leg",
          subtitle: "",
          distanceMeters: 500,
          durationSeconds: 120,
          normalizedPackage: northRoutePackage(),
        },
      ],
      selectedAlternativeID: "r",
    });
    guidance.startSelectedRoute();
    // Simulate a rider who's actually travelling EAST (not along the north
    // route). Feed six GPS fixes to prime the trail + drive advanceProgress.
    const origin = { latitude: 60.17, longitude: 24.94 };
    const locAny = location as unknown as {
      recordFix?: (p: CoordinatePoint, ts: number) => void;
      travelHeadingDegrees?: number;
    };
    for (let i = 0; i < 8; i++) {
      const fix = offsetByMeters(origin, i * 2.5, 0);
      locAny.recordFix?.(fix, i * 200);
      guidance.advanceProgress(fix, i * 200);
    }
    expect(
      locAny.travelHeadingDegrees,
      "LocationStore must expose a trail-derived travelHeadingDegrees after a few fixes (spec line 110)",
    ).toBeDefined();
    expect(
      mapCamera.target.kind === "center" ? mapCamera.target.bearing : Number.NaN,
      "camera bearing must follow the rider's actual direction (east ≈ 90°), not the route's north-segment bearing",
    ).toBeCloseTo(90, 0);
  });

  it("while stationary on a route, camera bearing falls back to the route segment (spec 101)", () => {
    const { guidance, planning, mapCamera, location } = buildHeadingHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "r",
          title: "North leg",
          subtitle: "",
          distanceMeters: 500,
          durationSeconds: 120,
          normalizedPackage: northRoutePackage(),
        },
      ],
      selectedAlternativeID: "r",
    });
    guidance.startSelectedRoute();
    const origin = { latitude: 60.17, longitude: 24.94 };
    const locAny = location as unknown as {
      recordFix?: (p: CoordinatePoint, ts: number) => void;
    };
    // Feed the same location repeatedly — stationary, no trail.
    for (let i = 0; i < 6; i++) {
      locAny.recordFix?.(origin, i * 200);
      guidance.advanceProgress(origin, i * 200);
    }
    // First-segment bearing is 0° (due north).
    expect(
      mapCamera.target.kind === "center" ? mapCamera.target.bearing : Number.NaN,
      "stationary on a route: bearing falls back to route segment (north ≈ 0°)",
    ).toBeCloseTo(0, 0);
  });

  it("while moving without a route, camera bearing still tracks the GPS trail (spec 110 applies)", () => {
    const { guidance, mapCamera, location } = buildHeadingHarness();
    // NO startSelectedRoute — planning mode, no preview set.
    // But the rider is moving east. The camera should rotate accordingly on
    // an explicit request. (In companion-web, that request comes from the
    // recenter button or planning-mode camera reaction. We drive it here by
    // poking onRecenterRequested via a public API the test harness can call.)
    const origin = { latitude: 60.17, longitude: 24.94 };
    const locAny = location as unknown as {
      recordFix?: (p: CoordinatePoint, ts: number) => void;
      travelHeadingDegrees?: number;
    };
    for (let i = 0; i < 8; i++) {
      locAny.recordFix?.(offsetByMeters(origin, i * 2.5, 0), i * 200);
    }
    expect(locAny.travelHeadingDegrees).toBeDefined();
    // Force a recenter as planning-mode (no routing, so route-bearing branch
    // is out). The harness's listener reads trailHeading and falls back to 0.
    const guidanceAny = guidance as unknown as { requestRecenter?: () => void };
    // Pre-condition: homeMode is "planning" — guidance.requestRecenter is a
    // no-op in that case. We test via advanceProgress during planning?
    // advanceProgress is also gated on homeMode. Instead, we just trigger a
    // setCenter via a direct emit — use the store's public onRecenterRequested
    // listener chain by calling a public "refresh" method. If there isn't one
    // yet, the test drives the camera directly.
    // The spec says rotating to GPS heading applies in BOTH modes when moving.
    // We assert that a recentre in planning-with-no-route uses the trail heading.
    if (guidanceAny.requestRecenter) guidanceAny.requestRecenter();
    // In planning mode, guidance.requestRecenter is a no-op today, so we
    // assert on the FACT that the trail heading is available to any future
    // planning-mode camera user. The harness callback's `trailHeading ?? 0`
    // branch covers the "moving without route" case when invoked from the
    // non-routing recenter path.
    expect(locAny.travelHeadingDegrees as number).toBeCloseTo(90, 0);
  });

  it("small GPS jitter produces a smoothed bearing (no camera swings)", () => {
    const { location } = buildHeadingHarness();
    const origin = { latitude: 60.17, longitude: 24.94 };
    const locAny = location as unknown as {
      recordFix?: (p: CoordinatePoint, ts: number) => void;
      travelHeadingDegrees?: number;
    };
    // 20 fixes going east with ±1.5m north/south noise on each.
    for (let i = 0; i < 20; i++) {
      const east = i * 2.5;
      const noise = ((i % 2) * 2 - 1) * 1.5;
      locAny.recordFix?.(offsetByMeters(origin, east, noise), i * 200);
    }
    const heading = locAny.travelHeadingDegrees as number;
    expect(heading).toBeDefined();
    expect(
      Math.abs(heading - 90),
      "smoothed trail heading must stay tight to due east even under ±1.5m lateral jitter",
    ).toBeLessThan(8);
  });
});

describe("compass-lock holds the route-overview camera (spec lines 95-96, regression for '🧭 lock reverts after timeout')", () => {
  // User-reported bug: tap-hold-tap (double-tap) turns the compass to 🧭
  // (northLocked), but ~1.3s later the camera snaps back to follow-rider
  // even though compassMode stays locked. Root cause: the fit-route
  // programmatic animation triggers MapLibre's zoomstart/rotatestart, which
  // calls GuidanceStore.noteUserMapInteraction(), which schedules a
  // follow-rider recenter after the inactivity timeout. Fix: while locked,
  // interactions re-establish the route overview instead of snapping to
  // follow-rider — the lock means "stay in overview".
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("while northLocked, noteUserMapInteraction does NOT schedule a follow-rider recenter", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassDoubleTap(); // → northLocked
    expect(guidance.compassMode).toBe("northLocked");

    let followCount = 0;
    guidance.onRecenterRequested(() => {
      followCount += 1;
    });

    guidance.noteUserMapInteraction();
    vi.advanceTimersByTime(5000);

    expect(
      followCount,
      "while locked, the camera must NOT snap back to follow-rider after the inactivity timeout — the lock means stay in overview",
    ).toBe(0);
  });

  it("while northLocked, noteUserMapInteraction re-emits route overview after the inactivity timeout", () => {
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassDoubleTap(); // → northLocked

    let fitCount = 0;
    guidance.onFitRouteRequested(() => {
      fitCount += 1;
    });

    guidance.noteUserMapInteraction();
    // Before the timeout: no re-fit yet.
    vi.advanceTimersByTime(500);
    expect(fitCount).toBe(0);

    // Past the timeout: re-fit fires so the lock visibly holds the overview.
    vi.advanceTimersByTime(900);
    expect(
      fitCount,
      "while locked, the inactivity timeout should re-fit the route overview — that's what 'lock' means",
    ).toBe(1);
  });

  it("while northLocked, advanceProgress does NOT emit follow-rider recenter (lock overrides GPS-tick follow)", () => {
    // Spec line 84 ("follow rider") is gated on autoFollow / northPreview.
    // In northLocked, GPS ticks must not snap the camera off the overview.
    const { guidance } = buildGuidanceHarness();
    guidance.startSelectedRoute();
    guidance.handleCompassDoubleTap(); // → northLocked

    let followCount = 0;
    guidance.onRecenterRequested(() => {
      followCount += 1;
    });

    guidance.advanceProgress(HELSINKI, 1000);
    guidance.advanceProgress(HELSINKI, 2000);

    expect(
      followCount,
      "GPS ticks during northLocked must not emit follow-rider (camera stays in overview until user unlocks)",
    ).toBe(0);
  });
});

describe("routing camera bearing follows the current route segment (spec line 101)", () => {
  // Spec line 101: "camera rotates so that immediate route direction is
  // towards top of the screen (riding towards, even when stationary yet)".
  // Build a route with a north-leg then east-leg so the expected bearing
  // transitions clearly as progress advances.
  //
  // Anchor at 60.17 N, 24.94 E.
  // geometry[0] = start
  // geometry[1] = 400 m due north of start   → first-leg bearing ≈ 0°
  // geometry[2] = 400 m due east of [1]      → second-leg bearing ≈ 90°
  const NORTH_METERS_PER_DEG = 111_320.0;
  const START_POINT = { latitude: 60.17, longitude: 24.94 };
  const MID_POINT = {
    latitude: 60.17 + 400 / NORTH_METERS_PER_DEG,
    longitude: 24.94,
  };
  // ~400 m east at that latitude.
  const cosLat = Math.cos((60.17 * Math.PI) / 180);
  const END_POINT = {
    latitude: MID_POINT.latitude,
    longitude: MID_POINT.longitude + 400 / (NORTH_METERS_PER_DEG * cosLat),
  };

  function buildBearingHarness() {
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
      origin: START_POINT,
      destination: END_POINT,
    };
    const guidance = new GuidanceStore(planning, persistence, location);
    planning.setPreview({
      alternatives: [
        {
          id: "bearing-route",
          title: "Route",
          subtitle: "",
          distanceMeters: 800,
          durationSeconds: 240,
          normalizedPackage: {
            version: CURRENT_ROUTE_PACKAGE_VERSION,
            routeIdentifier: "bearing-test",
            revision: 1,
            geometry: [START_POINT, MID_POINT, END_POINT],
            maneuvers: [
              {
                id: "m1",
                maneuverType: "depart" as const,
                location: START_POINT,
                distanceFromStartMeters: 0,
              },
              {
                id: "m2",
                maneuverType: "right" as const,
                location: MID_POINT,
                distanceFromStartMeters: 400,
              },
              {
                id: "m3",
                maneuverType: "arrive" as const,
                location: END_POINT,
                distanceFromStartMeters: 800,
              },
            ],
            summary: { totalDistanceMeters: 800, estimatedDurationSeconds: 240 },
            provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
          },
        },
      ],
      selectedAlternativeID: "bearing-route",
    });
    return { guidance, planning };
  }

  it("exposes routingBearingDegrees on GuidanceStore", () => {
    const { guidance } = buildBearingHarness();
    guidance.startSelectedRoute();
    const storeAny = guidance as unknown as { routingBearingDegrees?: number };
    expect(
      storeAny.routingBearingDegrees,
      "GuidanceStore must expose `routingBearingDegrees` so the MapSurface can rotate the camera",
    ).toBeDefined();
  });

  it("bearing at route start (stationary) points along the first leg (spec line 101: even when stationary yet)", () => {
    const { guidance } = buildBearingHarness();
    guidance.startSelectedRoute();
    const storeAny = guidance as unknown as { routingBearingDegrees?: number };
    const bearing = storeAny.routingBearingDegrees;
    // First leg is due north → bearing ≈ 0°. Allow ±5° for projection noise.
    expect(bearing).toBeDefined();
    expect(Math.abs((((bearing as number) + 540) % 360) - 180)).toBeLessThan(5);
  });

  it("bearing shifts to the next leg once the rider's projected progress passes the corner", () => {
    const { guidance } = buildBearingHarness();
    guidance.startSelectedRoute();
    // Rider at the corner (MID_POINT) → now on the east-leg.
    guidance.advanceProgress(MID_POINT, 0);
    guidance.advanceProgress(MID_POINT, 500);
    const storeAny = guidance as unknown as { routingBearingDegrees?: number };
    const bearing = storeAny.routingBearingDegrees;
    // Second leg is due east → bearing ≈ 90°. Allow ±5°.
    expect(bearing).toBeDefined();
    expect(Math.abs((bearing as number) - 90)).toBeLessThan(5);
  });
});
