import { beforeEach, describe, expect, it } from "vitest";
import {
  FakeLocationService,
  FakePlaceSearch,
  FakeRoutingAdapter,
} from "../__testlib__/fakes/index.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "./LocationStore.js";
import type { ProvidersMap } from "./PlanningStore.js";
import { PlanningStore } from "./PlanningStore.js";
import { SettingsStore } from "./SettingsStore.js";

// Helsinki (inside Finland) and Stockholm (outside Finland) anchors used below.
const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };
const STOCKHOLM = { latitude: 59.3293, longitude: 18.0686 };

type Harness = {
  planning: PlanningStore;
  settings: SettingsStore;
  location: LocationStore;
  fakeHsl: FakeRoutingAdapter;
  fakeOsm: FakeRoutingAdapter;
};

function buildHarness(): Harness {
  globalThis.localStorage?.clear();
  const persistence = new LocalStoragePersistence();
  const settings = new SettingsStore(persistence);
  const location = new LocationStore(new FakeLocationService(), persistence);
  const fakeHsl = new FakeRoutingAdapter("hsl");
  const fakeOsm = new FakeRoutingAdapter("osm");
  const providers: ProvidersMap = {
    hsl: fakeHsl,
    osm: fakeOsm,
    gpxImport: new FakeRoutingAdapter("gpxImport"),
    fitImport: new FakeRoutingAdapter("fitImport"),
    tcxImport: new FakeRoutingAdapter("tcxImport"),
  } as ProvidersMap;
  const planning = new PlanningStore(providers, new FakePlaceSearch(), location, settings);
  return { planning, settings, location, fakeHsl, fakeOsm };
}

describe("source selection (flows #38, #39, #41)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("both_endpoints_in_finland_exposes_all_modes", () => {
    const { planning } = buildHarness();
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    expect(planning.availableSourceModes).toEqual(["mixed", "hsl", "osm"]);
  });

  it("hsl_skipped_outside_finland: destination outside bbox collapses to [osm]", () => {
    const { planning } = buildHarness();
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: STOCKHOLM,
    };
    expect(planning.availableSourceModes).toEqual(["osm"]);
  });

  it("setSourceMode normalises hsl/mixed to osm when outside Finland", () => {
    const { planning } = buildHarness();
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: STOCKHOLM,
      destination: STOCKHOLM,
    };
    planning.setSourceMode("mixed");
    expect(planning.currentSourceMode).toBe("osm");
    planning.setSourceMode("hsl");
    expect(planning.currentSourceMode).toBe("osm");
  });
});
