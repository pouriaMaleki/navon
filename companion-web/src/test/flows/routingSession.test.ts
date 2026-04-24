import { beforeEach, describe, expect, it } from "vitest";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../fakes/index.js";

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
  planning.routeRequest = {
    ...planning.routeRequest,
    origin: HELSINKI,
    destination: HELSINKI_DEST,
  };
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

  it("nextInstructionLine transitions to the next maneuver as the rider advances past a corner (spec line 102)", () => {
    // Spec line 102: the UI shows the upcoming turn direction + distance.
    // As the rider passes a maneuver, the displayed line must switch to
    // the NEXT maneuver. The previous test suite only used straight-line
    // routes (depart→arrive), so this transition was never asserted.
    //
    // L-shape route: start → 400 m N → 400 m E. Three maneuvers:
    //   m1 depart at 0 m
    //   m2 right at 400 m  (the corner)
    //   m3 arrive at 800 m
    const NORTH_M_PER_DEG = 111_320.0;
    const start = { latitude: 60.17, longitude: 24.94 };
    const mid = { latitude: 60.17 + 400 / NORTH_M_PER_DEG, longitude: 24.94 };
    const cosLat = Math.cos((60.17 * Math.PI) / 180);
    const end = { latitude: mid.latitude, longitude: mid.longitude + 400 / (NORTH_M_PER_DEG * cosLat) };
    const lShape = {
      version: CURRENT_ROUTE_PACKAGE_VERSION,
      routeIdentifier: "lshape",
      revision: 1,
      geometry: [start, mid, end],
      maneuvers: [
        {
          id: "m1",
          maneuverType: "depart" as const,
          location: start,
          distanceFromStartMeters: 0,
        },
        {
          id: "m2",
          maneuverType: "right" as const,
          location: mid,
          distanceFromStartMeters: 400,
          instructionText: "Turn right onto 2nd street",
        },
        {
          id: "m3",
          maneuverType: "arrive" as const,
          location: end,
          distanceFromStartMeters: 800,
        },
      ],
      summary: { totalDistanceMeters: 800, estimatedDurationSeconds: 240 },
      provenance: { providerID: "osm" as const, generatedAtUnixMs: 0 },
    };
    const { planning, guidance } = buildHarness();
    planning.setPreview({
      alternatives: [
        {
          id: "lr",
          title: "L route",
          subtitle: "",
          distanceMeters: 800,
          durationSeconds: 240,
          normalizedPackage: lShape,
        },
      ],
      selectedAlternativeID: "lr",
    });
    guidance.startSelectedRoute();
    // At start, the next maneuver should be the right turn at the corner.
    let line = guidance.nextInstructionLine ?? "";
    expect(line.toLowerCase(), "before any progress, next instruction is the right turn at the corner").toContain(
      "right",
    );
    // Move rider past the corner (slightly past mid, projecting beyond 400m).
    const pastCorner = { latitude: mid.latitude, longitude: mid.longitude + 50 / (NORTH_M_PER_DEG * cosLat) };
    guidance.advanceProgress(pastCorner, 0);
    guidance.advanceProgress(pastCorner, 500);
    line = guidance.nextInstructionLine ?? "";
    expect(
      line.toLowerCase(),
      "after passing the corner, next instruction must transition off 'right' to the next (arrive) maneuver",
    ).not.toContain("right");
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
