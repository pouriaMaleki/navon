import { beforeEach, describe, expect, it } from "vitest";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "../../stores/LocationStore.js";
import type { ProvidersMap } from "../../stores/PlanningStore.js";
import { PlanningStore } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../fakes/index.js";

// Helsinki (inside Uusimaa) and Tampere (outside Uusimaa) anchors used below.
const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };
const TAMPERE = { latitude: 61.4978, longitude: 23.761 };

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
    // Import providers aren't exercised here but the map needs all keys.
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

  it("single_source_no_tabs: HSL unconfigured collapses to [osm]", () => {
    const { planning, settings } = buildHarness();
    settings.updateSettings({ preferLiveHslRouting: false, hslSubscriptionKey: "" });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    expect(planning.availableSourceModes).toEqual(["osm"]);
  });

  it("dual_source_shows_tabs: HSL configured + both endpoints in Uusimaa exposes all modes", () => {
    const { planning, settings } = buildHarness();
    settings.updateSettings({ preferLiveHslRouting: true, hslSubscriptionKey: "KEY" });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    expect(planning.availableSourceModes).toEqual(["mixed", "hsl", "osm"]);
  });

  it("hsl_skipped_outside_uusimaa: destination outside bbox collapses to [osm]", () => {
    const { planning, settings } = buildHarness();
    settings.updateSettings({ preferLiveHslRouting: true, hslSubscriptionKey: "KEY" });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: TAMPERE,
    };
    expect(planning.availableSourceModes).toEqual(["osm"]);
  });

  it("setSourceMode normalises hsl/mixed to osm when HSL unavailable", () => {
    const { planning, settings } = buildHarness();
    settings.updateSettings({ preferLiveHslRouting: false, hslSubscriptionKey: "" });
    planning.setSourceMode("mixed");
    expect(planning.currentSourceMode).toBe("osm");
    planning.setSourceMode("hsl");
    expect(planning.currentSourceMode).toBe("osm");
  });
});
