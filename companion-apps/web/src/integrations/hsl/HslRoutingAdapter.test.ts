import { describe, expect, it } from "vitest";
import {
  type CompanionSettings,
  DEFAULT_COMPANION_SETTINGS,
  type RoutePlanRequest,
} from "../../domain/models.js";
import { HslRoutingAdapter } from "./HslRoutingAdapter.js";

const ORIGIN = { latitude: 60.1699, longitude: 24.9384 };
const DESTINATION = { latitude: 60.1921, longitude: 24.9458 };
const REQUEST: RoutePlanRequest = {
  origin: ORIGIN,
  destination: DESTINATION,
  providerID: "hsl",
};

function makeAdapter(overrides: Partial<CompanionSettings> = {}): HslRoutingAdapter {
  const settings: CompanionSettings = { ...DEFAULT_COMPANION_SETTINGS, ...overrides };
  return new HslRoutingAdapter(() => settings);
}

// Why existing tests didn't cover this: HSL adapter unit tests were limited
// to source-mode tab gating (`sourceSelection.test.ts`) and to fake adapters
// in flow tests; nothing exercised real `HslRoutingAdapter` ETA calculation.
describe("HSL ETA with riding-speed override", () => {
  it("derives durationSeconds from totalDistance / cyclingSpeedKph when override is set", async () => {
    // The default sample geometry returns ~2.5 km. At 18 kph (5 m/s) the
    // expected duration is roughly distance / 5 ≈ 500 s.
    const adapter = makeAdapter({ cyclingSpeedKph: 18 });
    const preview = await adapter.planRoute(REQUEST);
    expect(preview.alternatives.length).toBeGreaterThan(0);
    for (const alt of preview.alternatives) {
      const distance = alt.normalizedPackage.summary.totalDistanceMeters;
      const expectedSec = Math.round(distance / (18 / 3.6));
      // ±2 s for rounding tolerance.
      expect(
        Math.abs(alt.normalizedPackage.summary.estimatedDurationSeconds - expectedSec),
      ).toBeLessThanOrEqual(2);
      expect(Math.abs(alt.durationSeconds - expectedSec)).toBeLessThanOrEqual(2);
    }
  });

  it("higher speed produces lower ETA than lower speed for the same route", async () => {
    const slow = makeAdapter({ cyclingSpeedKph: 12 });
    const fast = makeAdapter({ cyclingSpeedKph: 25 });
    const slowPreview = await slow.planRoute(REQUEST);
    const fastPreview = await fast.planRoute(REQUEST);
    const slowSec = slowPreview.alternatives[0].normalizedPackage.summary.estimatedDurationSeconds;
    const fastSec = fastPreview.alternatives[0].normalizedPackage.summary.estimatedDurationSeconds;
    expect(fastSec).toBeLessThan(slowSec);
  });
});
