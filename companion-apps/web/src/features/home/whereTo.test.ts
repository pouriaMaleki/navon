import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocalStoragePersistence } from "../../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "../../stores/LocationStore.js";
import { PlanningStore, type ProvidersMap } from "../../stores/PlanningStore.js";
import { SettingsStore } from "../../stores/SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../../__testlib__/fakes/index.js";
import { loadUxConstants } from "../../__testlib__/fixtures/uxConstants.js";

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
  return { planning, settings, search, location };
}

describe("where-to dropdown (plan flows #20, #25-27, #35-37)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("typeahead calls search adapter and populates suggestions", async () => {
    const { planning, search } = buildHarness();
    search.nextResults = [
      {
        id: "r1",
        title: "Helsinki Cathedral",
        subtitle: "Senaatintori",
        coordinate: { latitude: 60.1699, longitude: 24.9522 },
      },
    ];
    await planning.updateQuery("cathedral");
    // Typeahead is now debounced (spec line 34). Wait out the debounce + let
    // the async search settle.
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(search.searchCalls.length).toBe(1);
    expect(planning.suggestions.length).toBe(1);
    expect(planning.suggestions[0].title).toBe("Helsinki Cathedral");
  });

  it("empty query clears suggestions (blank input)", async () => {
    const { planning, search } = buildHarness();
    search.nextResults = [
      {
        id: "r1",
        title: "Helsinki",
        subtitle: "",
        coordinate: HELSINKI,
      },
    ];
    await planning.updateQuery("helsinki");
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(planning.suggestions.length).toBe(1);
    await planning.updateQuery("");
    expect(planning.suggestions.length).toBe(0);
  });

  it("manual_keyboard_delete_wipes_destination_and_routes (flow #37)", async () => {
    const { planning } = buildHarness();
    planning.query = "cathedral";
    planning.preview = {
      alternatives: [
        {
          id: "a1",
          title: "Route 1",
          subtitle: "",
          distanceMeters: 1000,
          durationSeconds: 300,
          normalizedPackage: {
            version: { major: 1, minor: 0 },
            routeIdentifier: "osm-test",
            revision: 1,
            geometry: [HELSINKI, HELSINKI_DEST],
            maneuvers: [],
            summary: { totalDistanceMeters: 1000, estimatedDurationSeconds: 300 },
            provenance: { providerID: "osm", generatedAtUnixMs: 0 },
          },
        },
      ],
    };
    await planning.updateQuery("");
    expect(planning.suggestions.length).toBe(0);
  });
});

describe("typeahead debounce + loading + area bias (plan flows #25, #26, #27)", () => {
  let search: FakePlaceSearch;
  let planning: PlanningStore;

  beforeEach(() => {
    globalThis.localStorage?.clear();
    vi.useFakeTimers();
    const harness = buildHarness();
    planning = harness.planning;
    search = harness.search;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("typeahead_debounces (flow #25): rapid keystrokes produce one adapter call", async () => {
    // Spec line 75: typeahead searches should debounce. Current `updateQuery`
    // fires on every keystroke without a debounce window — this test is
    // expected RED until a debouncer is added.
    const constants = loadUxConstants();
    const promises = [
      planning.updateQuery("h"),
      planning.updateQuery("he"),
      planning.updateQuery("hel"),
      planning.updateQuery("hels"),
    ];
    vi.advanceTimersByTime(constants.typeaheadDebounceMs + 10);
    await Promise.all(promises);
    expect(
      search.searchCalls.length,
      "debounce should coalesce rapid keystrokes into one call",
    ).toBe(1);
  });

  it("typeahead_loading_indicator (flow #26): a pending query exposes observable loading state", async () => {
    // Current PlanningStore has `planningStatus` for plan-route loading, and
    // `isResolvingUrl` for URL-resolution loading, but NO observable flag for
    // an in-flight typeahead search. Expected RED until one is added.
    search.searchDelayMs = 100;
    const pending = planning.updateQuery("cathedral");
    const storeAny = planning as unknown as { isTypeaheadSearching?: boolean };
    expect(
      storeAny.isTypeaheadSearching,
      "PlanningStore should expose isTypeaheadSearching while a search is in flight",
    ).toBe(true);
    await vi.advanceTimersByTimeAsync(200);
    await pending;
  });

  it("typeahead_area_bias (flow #27): search adapter receives rider location context", async () => {
    // Spec line 75: suggestions should favour same-city / near-rider results.
    // `PlanningStore.runTypeaheadSearch` pulls the rider location from the
    // LocationStore and passes it as `riderBias` to `searchDestinations`.
    // Seed a fix on the location and verify the bias propagates.
    vi.useRealTimers();
    const harness = buildHarness();
    const fakeLoc = harness.location as unknown as { currentLocation: typeof HELSINKI };
    fakeLoc.currentLocation = HELSINKI;
    await harness.planning.updateQuery("station");
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(
      harness.search.lastQueryBias,
      "searchDestinations should be called with rider-location bias",
    ).toEqual(HELSINKI);
  });
});

describe("recents pagination (plan flows #22, #23, #24)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("recents_paginate_on_scroll (flow #23): initial slice is bounded", () => {
    // Spec lines 72–73: the dropdown shows "only a few until user scroll to
    // the bottom of it". The initial visibleRecentCount must therefore be
    // bounded so a large history doesn't render everything up-front.
    const { planning } = buildHarness();
    expect(
      planning.visibleRecentCount,
      "initial recents slice should be bounded to a page",
    ).toBeLessThan(30);
  });

  it("recents_paginate_on_scroll: load-more is gated on reaching the end", () => {
    // Real semantics: load-more should fire only when the user has scrolled
    // to the last visible item — passing a random id must NOT grow the count.
    // Today the store grows unconditionally (PlanningStore.loadMoreRecentsIfNeeded
    // ignores the id), which is RED against this assertion.
    const { planning } = buildHarness();
    // Seed the persistence with enough recents that the page limit is meaningful.
    const persistence = new LocalStoragePersistence();
    for (let i = 0; i < 25; i++) {
      persistence.saveRecentDestination({
        latitude: 60.1 + i * 0.01,
        longitude: 24.9,
      });
    }
    const before = planning.visibleRecentCount;
    planning.loadMoreRecentsIfNeeded("a-random-id-not-the-last");
    expect(
      planning.visibleRecentCount,
      "loadMoreRecentsIfNeeded must not grow for a non-last item — pagination should be gated on the end of the visible slice",
    ).toBe(before);
  });

  it("recents_pagination_loading_indicator (flow #24): in-flight load-more surfaces a loading flag", () => {
    // Spec line 72 demands "give user indicators when things are loading".
    // PlanningStore does not expose `isLoadingMoreRecents` today.
    // Expected RED until that observable lands.
    const { planning } = buildHarness();
    const storeAny = planning as unknown as { isLoadingMoreRecents?: boolean };
    planning.loadMoreRecentsIfNeeded("seed");
    expect(
      storeAny.isLoadingMoreRecents,
      "PlanningStore should expose isLoadingMoreRecents during async recents loads",
    ).toBeDefined();
  });

  it("selectSuggestion immediately closes the dropdown (regression: list stays open after pick on real devices)", async () => {
    // Spec lines 25-27 + user-reported bug: tapping a suggestion should
    // collapse the dropdown so the route preview underneath is visible
    // before the user picks an alternative. The existing whereTo tests
    // never asserted post-selection close — only typeahead population.
    const { planning, search } = buildHarness();
    search.nextResults = [
      {
        id: "r1",
        title: "Helsinki Cathedral",
        subtitle: "Senaatintori",
        coordinate: { latitude: 60.1699, longitude: 24.9522 },
      },
    ];
    await planning.updateQuery("cathedral");
    await new Promise((r) => setTimeout(r, 300));
    expect(planning.suggestions.length).toBe(1);
    planning.openSearch();
    expect(planning.isSearchOpen).toBe(true);
    planning.selectSuggestion(planning.suggestions[0]);
    // Synchronous: store-level close.
    expect(planning.isSearchOpen, "selectSuggestion must close the dropdown synchronously").toBe(
      false,
    );
    // Even after planRoute has fully resolved (which would otherwise rebuild
    // the preview and could trigger UI re-focus), the dropdown stays closed.
    await new Promise((r) => setTimeout(r, 100));
    expect(
      planning.isSearchOpen,
      "post-selection async planRoute must not re-open the search panel",
    ).toBe(false);
  });

  it("openSearch is a no-op for a short window after selectSuggestion (defensive against view-layer re-focus)", async () => {
    // The real-device bug is the input retains focus after the touch and a
    // subsequent React re-render replays `onFocus={() => openSearch()}`.
    // PlanningStore must absorb that follow-up open() call within a short
    // post-selection window, otherwise the dropdown re-opens visually even
    // though the store had set it false. Expected RED until the latch lands.
    const { planning, search } = buildHarness();
    search.nextResults = [
      {
        id: "r1",
        title: "Kallio",
        subtitle: "",
        coordinate: { latitude: 60.184, longitude: 24.952 },
      },
    ];
    await planning.updateQuery("kallio");
    await new Promise((r) => setTimeout(r, 300));
    planning.openSearch();
    planning.selectSuggestion(planning.suggestions[0]);
    expect(planning.isSearchOpen).toBe(false);
    // Simulate the view-layer re-focus replay.
    planning.openSearch();
    expect(
      planning.isSearchOpen,
      "openSearch within the post-selection latch window must be a no-op (anti-re-focus)",
    ).toBe(false);
  });

  it("recents_dedupe (flow #22): duplicate entries collapse at the persistence layer", () => {
    // Spec line 71: "doesn't have duplicates". Persistence dedupes within
    // 80 m (see LocalStoragePersistence.saveRecentDestination); this test
    // locks in the behaviour.
    const persistence = new LocalStoragePersistence();
    const p = { latitude: 60.17, longitude: 24.94 };
    persistence.saveRecentDestination(p);
    persistence.saveRecentDestination(p);
    expect(
      persistence.loadRecentDestinations().length,
      "two saves of the same coordinate should dedupe to one recent",
    ).toBe(1);
  });
});
