import { describe, expect, it } from "vitest";
import type { CoordinatePoint } from "../../domain/models.js";
import { HeadingTrail } from "../../integrations/location/HeadingTrail.js";

// Spec line 110 — authoritative: "camera rotates so that riding direction
// is towards top of the screen this overrides the camera of routing. Most
// important camera behaviour is this. (it needs to determine the direction
// by last few GPS locations it receives)".
//
// These tests pin the smoothness contract: small GPS jitter must NOT
// produce large bearing swings, and the trail must reject displacements
// below a minimum threshold so that parked-rider noise doesn't spin the
// camera.

const HELSINKI: CoordinatePoint = { latitude: 60.17, longitude: 24.94 };

function offsetByMeters(
  base: CoordinatePoint,
  eastMeters: number,
  northMeters: number,
): CoordinatePoint {
  const metersPerDegreeLat = 111_320.0;
  const meanLat = (base.latitude * Math.PI) / 180.0;
  return {
    latitude: base.latitude + northMeters / metersPerDegreeLat,
    longitude: base.longitude + eastMeters / (metersPerDegreeLat * Math.cos(meanLat)),
  };
}

describe("HeadingTrail (spec line 110 — GPS-derived heading, smoothed)", () => {
  it("returns undefined until there is enough displacement", () => {
    const trail = new HeadingTrail({
      maxAgeMs: 5_000,
      maxFixes: 10,
      minDisplacementM: 3.0,
      smoothingAlpha: 0.25,
    });
    // A single fix — impossible to derive a heading.
    trail.recordFix(HELSINKI, 0);
    expect(trail.travelHeadingDegrees).toBeUndefined();
    // Second fix, 0.5 m away — below the displacement floor (GPS jitter).
    trail.recordFix(offsetByMeters(HELSINKI, 0.5, 0), 100);
    expect(
      trail.travelHeadingDegrees,
      "tiny GPS jitter below the displacement floor must not produce a heading",
    ).toBeUndefined();
  });

  it("produces a heading once total displacement exceeds the floor (east → ≈90°)", () => {
    const trail = new HeadingTrail({
      maxAgeMs: 5_000,
      maxFixes: 10,
      minDisplacementM: 3.0,
      smoothingAlpha: 0.25,
    });
    for (let i = 0; i < 5; i++) {
      trail.recordFix(offsetByMeters(HELSINKI, i * 1.2, 0), i * 200);
    }
    const heading = trail.travelHeadingDegrees;
    expect(heading).toBeDefined();
    expect(Math.abs((heading as number) - 90)).toBeLessThan(5);
  });

  it("smooths small lateral jitter (does not swing bearing wildly)", () => {
    const trail = new HeadingTrail({
      maxAgeMs: 5_000,
      maxFixes: 10,
      minDisplacementM: 3.0,
      smoothingAlpha: 0.25,
    });
    // Rider is clearly heading east. Inject small north/south noise on each fix.
    const base = HELSINKI;
    for (let i = 0; i < 20; i++) {
      const east = i * 2.5; // 2.5 m per fix — steady forward motion.
      const noise = ((i % 2) * 2 - 1) * 1.5; // ±1.5 m lateral.
      trail.recordFix(offsetByMeters(base, east, noise), i * 200);
    }
    const heading = trail.travelHeadingDegrees as number;
    expect(heading).toBeDefined();
    // Even with ±1.5 m lateral jitter on every step, the smoothed heading
    // must stay tight to due east (90°). A raw step-to-step bearing would
    // oscillate by tens of degrees.
    expect(Math.abs(heading - 90)).toBeLessThan(8);
  });

  it("expires old fixes so a new leg in a different direction wins within the window", () => {
    const trail = new HeadingTrail({
      maxAgeMs: 2_000,
      maxFixes: 10,
      minDisplacementM: 3.0,
      smoothingAlpha: 0.25,
    });
    // Leg 1: east.
    for (let i = 0; i < 5; i++) trail.recordFix(offsetByMeters(HELSINKI, i * 1.5, 0), i * 200);
    expect(Math.abs((trail.travelHeadingDegrees as number) - 90)).toBeLessThan(5);
    // Long pause — everything in the buffer ages out.
    const pivot = offsetByMeters(HELSINKI, 6, 0);
    // Leg 2: due north, starting after the pause.
    for (let i = 0; i < 5; i++) {
      trail.recordFix(offsetByMeters(pivot, 0, i * 1.5), 10_000 + i * 200);
    }
    const heading = trail.travelHeadingDegrees as number;
    // North is 0°; allow a small EMA tail from whatever primed the buffer.
    expect(Math.abs(((heading + 540) % 360) - 180)).toBeLessThan(20);
  });

  it("reset() clears the buffer (resuming fresh after Stop)", () => {
    const trail = new HeadingTrail({
      maxAgeMs: 5_000,
      maxFixes: 10,
      minDisplacementM: 3.0,
      smoothingAlpha: 0.25,
    });
    for (let i = 0; i < 5; i++) trail.recordFix(offsetByMeters(HELSINKI, i * 1.5, 0), i * 200);
    expect(trail.travelHeadingDegrees).toBeDefined();
    trail.reset();
    expect(trail.travelHeadingDegrees).toBeUndefined();
  });
});
