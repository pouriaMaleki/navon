import { beforeEach, describe, expect, it } from "vitest";
import {
  FakeLocationService,
  FakePlaceSearch,
  FakeRoutingAdapter,
} from "../__testlib__/fakes/index.js";
import {
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
} from "../domain/models.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "./GuidanceStore.js";
import { LocationStore } from "./LocationStore.js";
import { PlanningStore, type ProvidersMap } from "./PlanningStore.js";
import { SettingsStore } from "./SettingsStore.js";

/**
 * iOS parity (`IconStackLayoutTests` on iOS): the home screen has two
 * persistent right- and left-side icon rails. Items appear in a fixed
 * top-down order; conditional items always sit at the BOTTOM so the
 * always-on items above never shift when the conditionals appear or
 * disappear.
 *
 * **Top-RIGHT** (top → bottom): settings → compass → device chip (only when paired).
 * **Top-LEFT**  (top → bottom): zoom-in → zoom-out → alternate-routes (only in routing).
 *
 * The where-to top bar is full width — Settings is on the rail, never
 * inline with the search field.
 */
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

function tinyRoute(): NormalizedRoutePackage {
  const end: CoordinatePoint = {
    latitude: HELSINKI.latitude + 400 / METERS_PER_DEG_LAT,
    longitude: HELSINKI.longitude,
  };
  return {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: "rail-test",
    revision: 1,
    geometry: [HELSINKI, end],
    maneuvers: [
      {
        id: "depart",
        maneuverType: "depart",
        location: HELSINKI,
        distanceFromStartMeters: 0,
        distanceToNextMeters: 400,
      },
      { id: "arrive", maneuverType: "arrive", location: end, distanceFromStartMeters: 400 },
    ],
    summary: { totalDistanceMeters: 400, estimatedDurationSeconds: 120, destinationLabel: "Park" },
    provenance: { providerID: "osm", generatedAtUnixMs: 0 },
  };
}

function setRouteAndStart(planning: PlanningStore, guidance: GuidanceStore) {
  planning.preview = {
    alternatives: [
      {
        id: "a",
        title: "T",
        subtitle: "",
        distanceMeters: 400,
        durationSeconds: 120,
        normalizedPackage: tinyRoute(),
      },
    ],
    selectedAlternativeID: "a",
    routeIdentifier: "rail-test",
    routeRevision: 1,
  };
  guidance.startSelectedRoute();
}

describe("GuidanceStore — top-right icon stack (iOS parity)", () => {
  beforeEach(() => globalThis.localStorage?.clear());

  it("planning mode (unpaired) is settings then compass", () => {
    const { guidance } = buildHarness();
    expect(guidance.topRightIconStack).toEqual(["settings", "compass"]);
  });

  it("planning order matches routing order (positions don't shift between modes)", () => {
    const { planning, guidance } = buildHarness();
    const planningStack = guidance.topRightIconStack;
    setRouteAndStart(planning, guidance);
    expect(guidance.topRightIconStack).toEqual(planningStack);
  });

  it("settings is always FIRST so its on-screen position never moves", () => {
    const { guidance } = buildHarness();
    expect(guidance.topRightIconStack[0]).toBe("settings");
  });
});

describe("GuidanceStore — top-left icon stack (iOS parity)", () => {
  beforeEach(() => globalThis.localStorage?.clear());

  it("planning mode is just zoom-in / zoom-out", () => {
    const { guidance } = buildHarness();
    expect(guidance.topLeftIconStack).toEqual(["zoomIn", "zoomOut"]);
  });

  it("routing mode appends alternate-routes at the bottom", () => {
    const { planning, guidance } = buildHarness();
    setRouteAndStart(planning, guidance);
    expect(guidance.topLeftIconStack).toEqual(["zoomIn", "zoomOut", "alternateRoutes"]);
  });

  it("zoom positions don't change between planning and routing", () => {
    const { planning, guidance } = buildHarness();
    setRouteAndStart(planning, guidance);
    expect(guidance.topLeftIconStack.indexOf("zoomIn")).toBe(0);
    expect(guidance.topLeftIconStack.indexOf("zoomOut")).toBe(1);
  });
});
