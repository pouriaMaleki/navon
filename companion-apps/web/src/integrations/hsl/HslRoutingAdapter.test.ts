import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
  const settings: CompanionSettings = {
    ...DEFAULT_COMPANION_SETTINGS,
    hslEndpointURL: "https://navon.bike/api/hsl/routing",
    ...overrides,
  };
  return new HslRoutingAdapter(() => settings);
}

const LIVE_RESPONSE = {
  data: {
    plan: {
      itineraries: [
        {
          duration: 360,
          legs: [
            {
              mode: "BICYCLE",
              distance: 1200,
              from: { lat: ORIGIN.latitude, lon: ORIGIN.longitude, name: "Start" },
              to: { lat: DESTINATION.latitude, lon: DESTINATION.longitude, name: "End" },
              legGeometry: { points: "oytqH_kjPyGjEoIpDyI|DcNzIw@oEr@" },
            },
          ],
        },
      ],
    },
  },
};

describe("HslRoutingAdapter", () => {
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    fetchSpy = vi.spyOn(globalThis, "fetch");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // -- Live routing success --

  it("returns live itineraries when upstream responds 200", async () => {
    fetchSpy.mockResolvedValue(
      new Response(JSON.stringify(LIVE_RESPONSE), { status: 200 }),
    );
    const adapter = makeAdapter();
    const preview = await adapter.planRoute(REQUEST);
    expect(preview.alternatives.length).toBe(1);
    expect(preview.planningNotice).toBe("Live HSL Digitransit");
    const alt = preview.alternatives[0];
    expect(alt.normalizedPackage.geometry.length).toBeGreaterThan(1);
  });

  it("applies cycling-speed override to live itineraries", async () => {
    fetchSpy.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify(LIVE_RESPONSE), { status: 200 })),
    );
    const slow = makeAdapter({ cyclingSpeedKph: 12 });
    const fast = makeAdapter({ cyclingSpeedKph: 25 });
    const [slowP, fastP] = await Promise.all([slow.planRoute(REQUEST), fast.planRoute(REQUEST)]);
    const slowSec = slowP.alternatives[0].normalizedPackage.summary.estimatedDurationSeconds;
    const fastSec = fastP.alternatives[0].normalizedPackage.summary.estimatedDurationSeconds;
    expect(fastSec).toBeLessThan(slowSec);
  });

  // -- Upstream error responses —

  it("throws when upstream returns 502", async () => {
    fetchSpy.mockResolvedValue(
      new Response("Bad Gateway", { status: 502 }),
    );
    const adapter = makeAdapter();
    await expect(adapter.planRoute(REQUEST)).rejects.toThrow(
      "HSL Digitransit returned HTTP 502",
    );
  });

  it("throws when upstream returns 500", async () => {
    fetchSpy.mockResolvedValue(
      new Response("Internal Server Error", { status: 500 }),
    );
    const adapter = makeAdapter();
    await expect(adapter.planRoute(REQUEST)).rejects.toThrow(
      "HSL Digitransit returned HTTP 500",
    );
  });

  it("throws on network error", async () => {
    fetchSpy.mockRejectedValue(new TypeError("fetch failed"));
    const adapter = makeAdapter();
    await expect(adapter.planRoute(REQUEST)).rejects.toThrow("fetch failed");
  });

  // -- Timeout --

  it("throws after 15 s timeout when upstream hangs", async () => {
    vi.useFakeTimers();
    // Never resolves
    fetchSpy.mockReturnValue(new Promise(() => {}));
    const adapter = makeAdapter();
    const plan = adapter.planRoute(REQUEST);
    // Advance past the 15 s timeout
    vi.advanceTimersByTime(15_001);
    await expect(plan).rejects.toThrow("HSL Digitransit request timed out");
    vi.useRealTimers();
  });

  // -- Empty response —

  it("throws when upstream returns empty itineraries", async () => {
    fetchSpy.mockResolvedValue(
      new Response(
        JSON.stringify({ data: { plan: { itineraries: [] } } }),
        { status: 200 },
      ),
    );
    const adapter = makeAdapter();
    await expect(adapter.planRoute(REQUEST)).rejects.toThrow(
      "No HSL route alternatives were returned",
    );
  });

  // -- GraphQL errors —

  it("throws when upstream returns GraphQL errors", async () => {
    fetchSpy.mockResolvedValue(
      new Response(
        JSON.stringify({ errors: [{ message: "Internal error" }] }),
        { status: 200 },
      ),
    );
    const adapter = makeAdapter();
    await expect(adapter.planRoute(REQUEST)).rejects.toThrow("Internal error");
  });
});
