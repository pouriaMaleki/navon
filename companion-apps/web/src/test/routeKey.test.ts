import { describe, expect, it } from "vitest";
import { buildRouteKey } from "../integrations/cues/RoutingActivityCoordinator.js";

// Bug 3: when a reroute returns the same routeIdentifier but a bumped
// revision, the CueSnapshot.routeId must differ from the previous tick's
// value so CueEngine treats it as a new route and resets its latches.
// Previously buildCueSnapshot passed only routeIdentifier, so a revision
// bump was invisible to the engine — ghost arrivingInM cues followed.
describe("buildRouteKey — composite key for CueSnapshot.routeId", () => {
  it("returns undefined when routeIdentifier is undefined", () => {
    expect(buildRouteKey(undefined, 1)).toBeUndefined();
    expect(buildRouteKey(undefined, undefined)).toBeUndefined();
  });

  it("includes both identifier and revision so same-id reroutes differ", () => {
    expect(buildRouteKey("r1", 1)).toBe("r1-rev1");
    expect(buildRouteKey("r1", 2)).toBe("r1-rev2");
    // Different revisions must produce different keys.
    expect(buildRouteKey("r1", 1)).not.toBe(buildRouteKey("r1", 2));
  });

  it("defaults revision to 0 when undefined", () => {
    expect(buildRouteKey("r1", undefined)).toBe("r1-rev0");
  });

  it("different identifiers produce different keys regardless of revision", () => {
    expect(buildRouteKey("osm-route", 1)).not.toBe(buildRouteKey("hsl-route", 1));
  });
});
