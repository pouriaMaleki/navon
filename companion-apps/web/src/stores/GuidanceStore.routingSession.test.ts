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
    const end = {
      latitude: mid.latitude,
      longitude: mid.longitude + 400 / (NORTH_M_PER_DEG * cosLat),
    };
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
    expect(
      line.toLowerCase(),
      "before any progress, next instruction is the right turn at the corner",
    ).toContain("right");
    // Move rider past the corner (slightly past mid, projecting beyond 400m).
    const pastCorner = {
      latitude: mid.latitude,
      longitude: mid.longitude + 50 / (NORTH_M_PER_DEG * cosLat),
    };
    guidance.advanceProgress(pastCorner, 0);
    guidance.advanceProgress(pastCorner, 500);
    line = guidance.nextInstructionLine ?? "";
    expect(
      line.toLowerCase(),
      "after passing the corner, next instruction must transition off 'right' to the next (arrive) maneuver",
    ).not.toContain("right");
  });

  it("OSM cycling adapter fans out to BRouter fastbike + trekking + OSRM bike and labels alternatives", async () => {
    // The product shows up to 3 route alternatives (UX flow #44). Cycling
    // backends each have different infrastructure biases — BRouter
    // `fastbike` prefers paths, `trekking` is balanced, OSRM `bike` is
    // direct. Bundling them as alternatives lets the rider pick the line
    // that looks right on the map for their trip.
    const fs = await import("node:fs/promises");
    const path = await import("node:path");
    const fixturesDir = path.resolve(
      process.cwd(),
      "..",
      "..",
      "data",
      "parity-fixtures",
      "data",
      "cycling",
    );
    const fastbike = JSON.parse(
      await fs.readFile(path.join(fixturesDir, "brouter-fastbike-helsinki-kallio.json"), "utf-8"),
    );
    const trekking = JSON.parse(
      await fs.readFile(path.join(fixturesDir, "brouter-trekking-helsinki-kallio.json"), "utf-8"),
    );
    const osrm = JSON.parse(
      await fs.readFile(path.join(fixturesDir, "osrm-bike-helsinki-kallio.json"), "utf-8"),
    );

    const calls: string[] = [];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input: RequestInfo | URL) => {
      const url = String(input);
      calls.push(url);
      if (url.includes("brouter.de") && url.includes("profile=fastbike")) {
        return new Response(JSON.stringify(fastbike), { status: 200 });
      }
      if (url.includes("brouter.de") && url.includes("profile=trekking")) {
        return new Response(JSON.stringify(trekking), { status: 200 });
      }
      if (url.includes("router.project-osrm.org")) {
        return new Response(JSON.stringify(osrm), { status: 200 });
      }
      return new Response("not found", { status: 404 });
    };

    try {
      const mod = await import("../integrations/osm/OsmCyclingRoutingAdapter.js");
      const adapter = new mod.OsmCyclingRoutingAdapter();
      const preview = await adapter.planRoute({
        origin: { latitude: 60.1699, longitude: 24.9384 },
        destination: { latitude: 60.1854, longitude: 24.9522 },
        providerID: "osm",
      });
      expect(
        calls.some((u) => u.includes("brouter.de") && u.includes("fastbike")),
        "must hit BRouter fastbike",
      ).toBe(true);
      expect(
        calls.some((u) => u.includes("brouter.de") && u.includes("trekking")),
        "must hit BRouter trekking",
      ).toBe(true);
      expect(
        calls.some((u) => u.includes("router.project-osrm.org/route/v1/bike")),
        "must hit OSRM bike",
      ).toBe(true);
      const titles = preview.alternatives.map((a) => a.title);
      expect(titles, "all three labelled alternatives present").toEqual(
        expect.arrayContaining(["Bike-paths first", "Balanced cycling", "Fastest"]),
      );
      expect(preview.planningNotice).toMatch(/BRouter \+ OSRM/);
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("when a source fails, only successful alternatives are returned and the planningNotice flags it", async () => {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("brouter.de")) {
        return new Response("oops", { status: 500 });
      }
      // OSRM fixture
      const fs = await import("node:fs/promises");
      const path = await import("node:path");
      const osrm = JSON.parse(
        await fs.readFile(
          path.resolve(
            process.cwd(),
            "..",
            "..",
            "data",
            "parity-fixtures",
            "data",
            "cycling",
            "osrm-bike-helsinki-kallio.json",
          ),
          "utf-8",
        ),
      );
      return new Response(JSON.stringify(osrm), { status: 200 });
    };
    try {
      const mod = await import("../integrations/osm/OsmCyclingRoutingAdapter.js");
      const adapter = new mod.OsmCyclingRoutingAdapter();
      const preview = await adapter.planRoute({
        origin: { latitude: 60.1699, longitude: 24.9384 },
        destination: { latitude: 60.1854, longitude: 24.9522 },
        providerID: "osm",
      });
      expect(preview.alternatives).toHaveLength(1);
      expect(preview.alternatives[0].title).toBe("Fastest");
      expect(preview.planningNotice).toMatch(/2 sources unavailable/);
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("when ALL sources fail, falls back to the sample preview", async () => {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => new Response("oops", { status: 500 });
    try {
      const mod = await import("../integrations/osm/OsmCyclingRoutingAdapter.js");
      const adapter = new mod.OsmCyclingRoutingAdapter();
      const preview = await adapter.planRoute({
        origin: { latitude: 60.1699, longitude: 24.9384 },
        destination: { latitude: 60.1854, longitude: 24.9522 },
        providerID: "osm",
      });
      expect(preview.alternatives.length).toBeGreaterThan(0);
      expect(preview.planningNotice).toMatch(/sample/i);
    } finally {
      globalThis.fetch = originalFetch;
    }
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
