import { describe, expect, it } from "vitest";
import type { RouteHistoryItem } from "../../domain/models.js";
import { mergeRecentDestinations, mergeRouteHistory } from "./LocalStoragePersistence.js";

describe("persistence merging", () => {
  it("dedupes recent destinations within 80 m", () => {
    const a = { latitude: 60.1699, longitude: 24.9384 };
    const b = { latitude: 60.16995, longitude: 24.93845 };
    const merged = mergeRecentDestinations([a], b);
    expect(merged).toHaveLength(1);
    expect(merged[0]).toEqual(b);
  });

  it("keeps separate entries when farther than 80 m apart", () => {
    const a = { latitude: 60.17, longitude: 24.94 };
    const b = { latitude: 60.18, longitude: 24.95 };
    const merged = mergeRecentDestinations([a], b);
    expect(merged).toHaveLength(2);
  });

  it("merges recent-destination history items by location", () => {
    const earlier: RouteHistoryItem = {
      id: "old",
      title: "Helsinki Central",
      subtitle: "60.17, 24.94",
      source: "recentDestination",
      sourceLabel: "Recent",
      createdAtMs: 1,
      destination: { latitude: 60.17, longitude: 24.94 },
      occurrenceCount: 1,
    };
    const later: RouteHistoryItem = {
      id: "new",
      title: "Helsinki Central",
      subtitle: "60.17, 24.94",
      source: "recentDestination",
      sourceLabel: "Recent",
      createdAtMs: 2,
      destination: { latitude: 60.1701, longitude: 24.94 },
    };
    const merged = mergeRouteHistory([earlier], later);
    expect(merged).toHaveLength(1);
    expect(merged[0].occurrenceCount).toBe(2);
  });

  it("rejects generic titles when merging recents", () => {
    const earlier: RouteHistoryItem = {
      id: "old",
      title: "Helsinki Central",
      subtitle: "",
      source: "recentDestination",
      sourceLabel: "Recent",
      createdAtMs: 1,
      destination: { latitude: 60.17, longitude: 24.94 },
    };
    const later: RouteHistoryItem = {
      id: "new",
      title: "Dropped pin",
      subtitle: "",
      source: "recentDestination",
      sourceLabel: "Recent",
      createdAtMs: 2,
      destination: { latitude: 60.17, longitude: 24.94 },
    };
    const merged = mergeRouteHistory([earlier], later);
    expect(merged[0].title).toBe("Helsinki Central");
  });
});
