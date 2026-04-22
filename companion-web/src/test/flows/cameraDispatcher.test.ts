import { beforeEach, describe, expect, it } from "vitest";
import type { RootStore } from "../../app/RootStore.js";
import { dispatchCameraTarget } from "../../features/home/cameraDispatcher.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { MapCameraStore } from "../../stores/MapCameraStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../fakes/index.js";

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

/**
 * Minimal MapLibre-like stub that records calls. Emulates the quirk that
 * `isStyleLoaded()` transiently returns false while `setData` is in flight —
 * that was the source of the "Start doesn't move camera" bug.
 */
function makeFakeMap() {
  const easeToCalls: Array<{ center: [number, number]; zoom: number; bearing: number }> = [];
  const fitBoundsCalls: Array<unknown> = [];
  const paddingCalls: Array<{ bottom: number }> = [];
  let styleLoadedReturn = true;
  return {
    _easeToCalls: easeToCalls,
    _fitBoundsCalls: fitBoundsCalls,
    _paddingCalls: paddingCalls,
    // Public knob for tests: flip to false to simulate MapLibre's transient
    // "source.setData in flight" window.
    _setStyleLoaded: (value: boolean) => {
      styleLoadedReturn = value;
    },
    isStyleLoaded: () => styleLoadedReturn,
    getCanvas: () => ({ clientHeight: 800 }),
    setPadding: (p: { bottom: number }) => {
      paddingCalls.push(p);
    },
    easeTo: (args: { center: [number, number]; zoom: number; bearing: number }) => {
      easeToCalls.push(args);
    },
    fitBounds: (bounds: unknown) => {
      fitBoundsCalls.push(bounds);
    },
  };
}

function buildRootEquivalent() {
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
  const mapCameraStore = new MapCameraStore();
  const guidanceStore = new GuidanceStore(planning, persistence, location);
  return {
    planningStore: planning,
    guidanceStore,
    mapCameraStore,
    locationStore: location,
  } as unknown as RootStore;
}

describe("dispatchCameraTarget (spec lines 84, 101 — regression for 'Start does nothing on web')", () => {
  let store: RootStore;

  beforeEach(() => {
    store = buildRootEquivalent();
  });

  it("calls easeTo when the target is center-kind and the map is ready", () => {
    store.mapCameraStore.setCenter(HELSINKI, 16, 45);
    const map = makeFakeMap();
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, true);
    expect(map._easeToCalls).toHaveLength(1);
    expect(map._easeToCalls[0].zoom).toBe(16);
    expect(map._easeToCalls[0].bearing).toBe(45);
  });

  it("calls fitBounds when the target is fitBounds-kind", () => {
    store.mapCameraStore.fitBounds([HELSINKI, HELSINKI_DEST], 120);
    const map = makeFakeMap();
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, true);
    expect(map._fitBoundsCalls).toHaveLength(1);
  });

  it("dispatches camera updates EVEN WHEN isStyleLoaded is false (regression)", () => {
    // This is the bug that made Start silently do nothing on the real app:
    // `source.setData` during route-data reaction puts the style into a
    // transient "not loaded" state, and the previous implementation bailed
    // from the camera reaction on that condition. After `load` has fired
    // once, camera/source calls are safe to make regardless.
    store.mapCameraStore.setCenter(HELSINKI, 16, 0);
    const map = makeFakeMap();
    map._setStyleLoaded(false); // simulate setData-in-flight
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, true);
    expect(
      map._easeToCalls,
      "camera must still dispatch when MapLibre reports isStyleLoaded=false (post-load); the old pushCameraTarget bailed here",
    ).toHaveLength(1);
  });

  it("does NOT dispatch if the map has not yet reached its initial load event", () => {
    store.mapCameraStore.setCenter(HELSINKI, 16, 0);
    const map = makeFakeMap();
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, false);
    expect(map._easeToCalls).toHaveLength(0);
  });
});
