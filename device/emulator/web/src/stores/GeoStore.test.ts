import { describe, expect, it } from "vitest";

import { buildRuntimeGpsSampleFromGeolocation } from "./GeoStore";

describe("GeoStore live GPS normalization", () => {
  it("forwards browser-provided speed directly", () => {
    const sample = buildRuntimeGpsSampleFromGeolocation(
      makePosition({
        lat: 60.17442,
        lon: 24.9421,
        timestampMs: 2_000,
        speed: 4.2,
        heading: null,
      }),
      {
        lat: 60.1744198,
        lon: 24.9420998,
        timestampMs: 1_000,
      },
    );

    expect(sample.speedMps).toBe(4.2);
  });

  it("forwards browser-provided heading directly", () => {
    const sample = buildRuntimeGpsSampleFromGeolocation(
      makePosition({
        lat: 60.17442,
        lon: 24.9421,
        timestampMs: 2_000,
        speed: null,
        heading: 123,
      }),
      null,
    );

    expect(sample.courseRad).not.toBeNull();
    expect(sample.courseRad).toBeCloseTo((123 * Math.PI) / 180);
  });

  it("falls back to delta-derived speed and heading when browser fields are missing", () => {
    const sample = buildRuntimeGpsSampleFromGeolocation(
      makePosition({
        lat: 60.17442,
        lon: 24.9422,
        timestampMs: 2_000,
        speed: null,
        heading: null,
      }),
      {
        lat: 60.17442,
        lon: 24.9421,
        timestampMs: 1_000,
      },
    );

    expect(sample.speedMps).toBeGreaterThan(1);
    expect(sample.courseRad).not.toBeNull();
  });

  it("keeps browser speed on tiny-step updates instead of collapsing to zero", () => {
    const sample = buildRuntimeGpsSampleFromGeolocation(
      makePosition({
        lat: 60.1744201,
        lon: 24.9421001,
        timestampMs: 2_000,
        speed: 3.5,
        heading: 90,
      }),
      {
        lat: 60.17442,
        lon: 24.9421,
        timestampMs: 1_000,
      },
    );

    expect(sample.speedMps).toBe(3.5);
    expect(sample.courseRad).toBeCloseTo(Math.PI / 2);
  });
});

function makePosition({
  lat,
  lon,
  timestampMs,
  speed,
  heading,
}: {
  lat: number;
  lon: number;
  timestampMs: number;
  speed: number | null;
  heading: number | null;
}): GeolocationPosition {
  return {
    coords: {
      latitude: lat,
      longitude: lon,
      accuracy: 5,
      altitude: null,
      altitudeAccuracy: null,
      heading,
      speed,
      toJSON: () => ({}),
    },
    timestamp: timestampMs,
    toJSON: () => ({}),
  } as GeolocationPosition;
}
