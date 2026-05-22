import { beforeEach, describe, expect, it } from "vitest";
import { RootStore } from "./RootStore.js";
import type { RouteHistoryItem } from "../domain/models.js";

const HELSINKI_DEST = { latitude: 60.1921, longitude: 24.9458 };

function makeRecentItem(): RouteHistoryItem {
  return {
    id: `recent-${Date.now()}`,
    title: "Ensi linja 1",
    subtitle: "60.1921, 24.9458",
    source: "recentDestination",
    sourceLabel: "Recent",
    createdAtMs: Date.now(),
    destination: HELSINKI_DEST,
  };
}

// Why existing tests didn't cover this: whereTo flow tests asserted that
// `selectSuggestion` (a typeahead pick) closes the dropdown and writes the
// query, but `activateRouteHistoryItem` (the path used by tapping a recent)
// went through RootStore and never had the same assertions. The result was
// that picking a recent left the search panel open and the input blank,
// hiding the route suggestions card behind the still-mounted recents list.
describe("picking a recent updates the where-to input and closes the dropdown", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("sets planning.query to the recent's title", async () => {
    const store = new RootStore();
    store.planningStore.openSearch();
    expect(store.planningStore.isSearchOpen).toBe(true);
    expect(store.planningStore.query).toBe("");

    await store.activateRouteHistoryItem(makeRecentItem(), false);

    expect(
      store.planningStore.query,
      "tapping a recent must place its title into the where-to input",
    ).toBe("Ensi linja 1");
  });

  it("closes the search dropdown so the route suggestions card is visible", async () => {
    const store = new RootStore();
    store.planningStore.openSearch();

    await store.activateRouteHistoryItem(makeRecentItem(), false);

    expect(
      store.planningStore.isSearchOpen,
      "tapping a recent must close the where-to dropdown — otherwise it sits on top of the suggestions card",
    ).toBe(false);
  });

  it("post-selection latch absorbs an immediate openSearch() (defensive against view-layer re-focus)", async () => {
    const store = new RootStore();
    store.planningStore.openSearch();

    await store.activateRouteHistoryItem(makeRecentItem(), false);
    // The same React-onFocus replay that bites selectSuggestion would also
    // bite this path. The latch should swallow the immediate re-open.
    store.planningStore.openSearch();
    expect(store.planningStore.isSearchOpen).toBe(false);
  });
});
