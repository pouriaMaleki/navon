import { beforeEach, describe, expect, it } from "vitest";
import type { RootStore } from "../../app/RootStore.js";
import { dispatchCameraTarget } from "./cameraDispatcher.js";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "../../stores/GuidanceStore.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { MapCameraStore } from "../../stores/MapCameraStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../../__testlib__/fakes/index.js";

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

/**
 * Minimal MapLibre-like stub that records calls. Emulates the quirk that
 * `isStyleLoaded()` transiently returns false while `setData` is in flight —
 * that was the source of the "Start doesn't move camera" bug.
 */
function makeFakeMap(viewportHeight = 800) {
  const easeToCalls: Array<{ center: [number, number]; zoom: number; bearing: number }> = [];
  const fitBoundsCalls: Array<unknown> = [];
  const paddingCalls: Array<{ top: number; right: number; bottom: number; left: number }> = [];
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
    getCanvas: () => ({ clientHeight: viewportHeight }),
    setPadding: (p: { top: number; right: number; bottom: number; left: number }) => {
      paddingCalls.push(p);
    },
    easeTo: (args: { center: [number, number]; zoom: number; bearing: number }) => {
      easeToCalls.push(args);
    },
    fitBounds: (bounds: unknown, opts?: unknown) => {
      fitBoundsCalls.push({ bounds, opts });
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

  it("reserves bottomReservedPx as MapLibre padding.bottom for fitBounds (regression: route squished above tall alternatives card)", () => {
    // After Stop, the planning view shows up to 3 alternatives in a tall
    // bottom card (~330 px). If the dispatcher doesn't reserve that space,
    // fitBounds renders the whole route polyline behind the card and only
    // the upper third of the map shows usable content. The bottom-overlay
    // observer in HomeView writes the measured height to
    // `mapCameraStore.bottomReservedPx` — the dispatcher must consume it.
    store.mapCameraStore.fitBounds([HELSINKI, HELSINKI_DEST], 120);
    store.mapCameraStore.setBottomReservedPx(330);
    const map = makeFakeMap(896);
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, true);
    expect(map._paddingCalls.length).toBeGreaterThan(0);
    const lastPadding = map._paddingCalls[map._paddingCalls.length - 1];
    expect(lastPadding.bottom, "fitBounds must reserve the measured bottom-overlay height").toBe(
      330,
    );
    expect(
      lastPadding.top,
      "fitBounds must NOT push content down with a top padding — only reserve the bottom",
    ).toBe(0);
  });

  it("anchors the rider against the visible map area (viewport minus bottomReservedPx) for follow-rider, not the raw viewport", () => {
    // Spec line 84: rider in the bottom quarter. With a 132 px routing card
    // on an 896 px viewport, the rider rendered at 0.72 * 896 = 645 sits
    // BEHIND the card. The fix reads anchor against the *visible* area, so
    // rider y = 0.72 * (896 - 132) ≈ 550 — comfortably above the card.
    store.mapCameraStore.setCenter(HELSINKI, 16, 0);
    store.mapCameraStore.setRiderAnchorNormalizedY(0.72);
    store.mapCameraStore.setBottomReservedPx(132);
    const map = makeFakeMap(896);
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, true);
    const lastPadding = map._paddingCalls[map._paddingCalls.length - 1];
    // MapLibre center y = (top + h - bottom) / 2 must equal 0.72 * (h - bottom).
    const camCenterY = (lastPadding.top + 896 - lastPadding.bottom) / 2;
    const expectedY = 0.72 * (896 - 132);
    expect(camCenterY).toBeCloseTo(expectedY, 0);
    // And the rider must NOT land in the bottom-card region.
    expect(camCenterY).toBeLessThan(896 - 132);
  });

  it("fitBounds dispatch includes bearing: 0 so the overview always resets north-up (regression: rotated overview after heading-up ride)", () => {
    // After a heading-up routing session the MapLibre map has a non-zero bearing
    // (e.g. 45°). fitBounds without `bearing: 0` inherits the current bearing
    // and renders the overview rotated instead of north-up. Every path that
    // calls fitBounds — compass tap (northPreview/northLocked), stopGuidance,
    // route-overview in planning — must reset bearing explicitly.
    store.mapCameraStore.fitBounds([HELSINKI, HELSINKI_DEST], 120);
    const map = makeFakeMap();
    dispatchCameraTarget(map as unknown as import("maplibre-gl").Map, store, true);
    expect(map._fitBoundsCalls).toHaveLength(1);
    const entry = map._fitBoundsCalls[0] as { bounds: unknown; opts?: { bearing?: number } };
    expect(
      entry.opts?.bearing,
      "fitBounds must pass bearing: 0 to MapLibre so the route overview shows north-up regardless of the current map heading",
    ).toBe(0);
  });
});
