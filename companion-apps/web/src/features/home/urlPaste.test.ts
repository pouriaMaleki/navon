import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../../__testlib__/fakes/index.js";

const HELSINKI = { latitude: 60.1699, longitude: 24.9384 };
const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

function buildHarness() {
  globalThis.localStorage?.clear();
  const persistence = new LocalStoragePersistence();
  const settings = new SettingsStore(persistence);
  const location = new LocationStore(new FakeLocationService(), persistence);
  const search = new FakePlaceSearch();
  const providers: ProvidersMap = {
    hsl: new FakeRoutingAdapter("hsl"),
    osm: new FakeRoutingAdapter("osm"),
    gpxImport: new FakeRoutingAdapter("gpxImport"),
    fitImport: new FakeRoutingAdapter("fitImport"),
    tcxImport: new FakeRoutingAdapter("tcxImport"),
  } as ProvidersMap;
  const planning = new PlanningStore(providers, search, location, settings);
  planning.routeRequest = {
    ...planning.routeRequest,
    origin: HELSINKI,
    destination: HELSINKI_DEST,
  };
  return { planning, search };
}

describe("URL paste (plan flows #29-31)", () => {
  let originalFetch: typeof fetch;

  beforeEach(() => {
    globalThis.localStorage?.clear();
    originalFetch = globalThis.fetch;
    // Stub fetch so UrlExpander's proxy fallback chain resolves instantly.
    // Return 200 with empty bodies — expander can't find coordinates → null.
    globalThis.fetch = vi.fn(async () => {
      return new Response("", { status: 200, headers: { "content-type": "text/html" } });
    }) as typeof fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("inline @lat,lng URL resolves without hitting a network proxy", async () => {
    const { planning, search } = buildHarness();
    search.nextResolve = {
      id: "resolved",
      title: "Destination",
      subtitle: "",
      coordinate: { latitude: 60.16, longitude: 24.95 },
    };
    await planning.resolveUrlDestination("https://www.google.com/maps/@60.16,24.95,15z");
    expect(planning.isResolvingUrl).toBe(false);
    expect(planning.urlResolveError).toBeUndefined();
    expect(planning.routeRequest.destination).toEqual({ latitude: 60.16, longitude: 24.95 });
  });

  it("url_paste_loading_indicator (flow #30): isResolvingUrl is true before settle", async () => {
    const { planning, search } = buildHarness();
    search.nextResolve = null;
    // Call resolveUrlDestination without awaiting; inspect before microtask flush.
    const pending = planning.resolveUrlDestination("https://www.google.com/maps/@60.16,24.95,15z");
    const observedWhileInFlight = planning.isResolvingUrl;
    await pending;
    expect(
      observedWhileInFlight,
      "isResolvingUrl must flip to true synchronously before the promise settles",
    ).toBe(true);
    expect(planning.isResolvingUrl).toBe(false);
  });

  it("url_paste_error_message (flow #31): invalid URL surfaces urlResolveError", async () => {
    const { planning } = buildHarness();
    await planning.resolveUrlDestination("https://example.invalid/no-coords-here");
    expect(planning.isResolvingUrl).toBe(false);
    expect(
      planning.urlResolveError,
      "resolver should populate urlResolveError when no coordinate can be found",
    ).toBeDefined();
  });

  it("url_paste_recovers_after_error (flow #31 followup): a subsequent success clears the error", async () => {
    const { planning, search } = buildHarness();
    await planning.resolveUrlDestination("https://example.invalid/no-coords");
    expect(planning.urlResolveError).toBeDefined();
    search.nextResolve = {
      id: "ok",
      title: "Resolved",
      subtitle: "",
      coordinate: { latitude: 60.16, longitude: 24.95 },
    };
    await planning.resolveUrlDestination("https://www.google.com/maps/@60.16,24.95,15z");
    expect(planning.urlResolveError).toBeUndefined();
  });
});
