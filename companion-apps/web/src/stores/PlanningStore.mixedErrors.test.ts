import { describe, expect, it } from "vitest";
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

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

function buildHarness(opts?: { hslThrows?: boolean; osmThrows?: boolean }) {
  globalThis.localStorage?.clear();
  const persistence = new LocalStoragePersistence();
  const settings = new SettingsStore(persistence);
  const location = new LocationStore(new FakeLocationService(), persistence);
  const hsl = new FakeRoutingAdapter("hsl");
  const osm = new FakeRoutingAdapter("osm");
  if (opts?.hslThrows) {
    hsl.planFactory = () => {
      throw new Error("HSL upstream 502");
    };
  }
  if (opts?.osmThrows) {
    osm.planFactory = () => {
      throw new Error("OSM network down");
    };
  }
  const providers: ProvidersMap = {
    hsl,
    osm,
    gpxImport: new FakeRoutingAdapter("gpxImport"),
    fitImport: new FakeRoutingAdapter("fitImport"),
    tcxImport: new FakeRoutingAdapter("tcxImport"),
  } as ProvidersMap;
  const planning = new PlanningStore(providers, new FakePlaceSearch(), location, settings);
  return { planning, settings, location, hsl, osm };
}

describe("mixed-mode provider failure", () => {
  it("shows OSM routes when HSL fails with error", async () => {
    const { planning } = buildHarness({ hslThrows: true });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    planning.setSourceMode("mixed");
    await planning.planRoute();
    // OSM should still produce alternatives
    expect(planning.preview.alternatives.length).toBeGreaterThan(0);
    // The notice should mention hidden providers
    expect(planning.preview.planningNotice).toContain("some providers are hidden");
  });

  it("shows HSL routes when OSM fails with error", async () => {
    const { planning } = buildHarness({ osmThrows: true });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    planning.setSourceMode("mixed");
    await planning.planRoute();
    expect(planning.preview.alternatives.length).toBeGreaterThan(0);
    expect(planning.preview.planningNotice).toContain("some providers are hidden");
  });

  it("shows error notice when both providers fail", async () => {
    const { planning } = buildHarness({ hslThrows: true, osmThrows: true });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    planning.setSourceMode("mixed");
    await planning.planRoute();
    // No alternatives — both failed
    expect(planning.preview.alternatives.length).toBe(0);
    // Notice mentions hidden providers
    expect(planning.preview.planningNotice).toContain("some providers are hidden");
  });

  it("shows error notice when pure HSL fails", async () => {
    const { planning } = buildHarness({ hslThrows: true });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    planning.setSourceMode("hsl");
    await planning.planRoute();
    expect(planning.preview.alternatives.length).toBe(0);
    expect(planning.preview.planningNotice).toContain("Planning failed");
    expect(planning.preview.planningNotice).toContain("502");
  });

  it("pure OSM works normally when HSL is not involved", async () => {
    const { planning } = buildHarness({ hslThrows: true });
    planning.routeRequest = {
      ...planning.routeRequest,
      origin: HELSINKI,
      destination: HELSINKI_DEST,
    };
    planning.setSourceMode("osm");
    await planning.planRoute();
    expect(planning.preview.alternatives.length).toBeGreaterThan(0);
    // No error — HSL not involved
    expect(planning.preview.planningNotice).toBeFalsy();
  });
});
