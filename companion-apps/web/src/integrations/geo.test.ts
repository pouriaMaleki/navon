import { describe, expect, it } from "vitest";
import type { CoordinatePoint } from "../domain/models.js";
import {
  approximateDistanceMeters,
  classifyTurn,
  collapseCloseManeuvers,
  decodePolyline,
  deduplicateConsecutive,
} from "./geo.js";

const METERS_PER_DEG = 111320;

/** Build a polyline that goes north until bendDistanceM, then turns right by bendAngleDeg. */
function makeBendGeometry(
  bendDistanceM: number,
  bendAngleDeg: number,
  totalLengthM: number,
): CoordinatePoint[] {
  const points: CoordinatePoint[] = [];
  const step = 5;
  let lat = 60.0;
  let lon = 25.0;
  points.push({ latitude: lat, longitude: lon });
  let dist = 0;
  while (dist < totalLengthM) {
    const segLen = Math.min(step, totalLengthM - dist);
    const bearing = dist >= bendDistanceM ? bendAngleDeg : 0;
    const rad = (bearing * Math.PI) / 180;
    const cosLat = Math.cos(((lat + lat) / 2) * (Math.PI / 180));
    lat += (segLen * Math.cos(rad)) / METERS_PER_DEG;
    lon += (segLen * Math.sin(rad)) / (METERS_PER_DEG * cosLat);
    dist += segLen;
    points.push({ latitude: lat, longitude: lon });
  }
  return points;
}

describe("geo", () => {
  it("approximates distance meters for nearby points", () => {
    const a = { latitude: 60.1699, longitude: 24.9384 };
    const b = { latitude: 60.1709, longitude: 24.9384 };
    const distance = approximateDistanceMeters(a, b);
    expect(distance).toBeGreaterThan(105);
    expect(distance).toBeLessThan(120);
  });

  it("decodes a known Google polyline", () => {
    const points = decodePolyline("_p~iF~ps|U_ulLnnqC_mqNvxq`@");
    expect(points).toHaveLength(3);
    expect(points[0].latitude).toBeCloseTo(38.5, 1);
    expect(points[0].longitude).toBeCloseTo(-120.2, 1);
  });

  it("deduplicates consecutive duplicates only", () => {
    const result = deduplicateConsecutive([
      { latitude: 1, longitude: 1 },
      { latitude: 1, longitude: 1 },
      { latitude: 2, longitude: 2 },
      { latitude: 1, longitude: 1 },
    ]);
    expect(result).toHaveLength(3);
  });

  it("classifies turn magnitudes per the iOS spec", () => {
    expect(classifyTurn(10)).toBeNull();
    expect(classifyTurn(30)?.type).toBe("slightRight");
    expect(classifyTurn(-30)?.type).toBe("slightLeft");
    expect(classifyTurn(80)?.type).toBe("right");
    expect(classifyTurn(-80)?.type).toBe("left");
    expect(classifyTurn(120)?.type).toBe("sharpRight");
    expect(classifyTurn(-120)?.type).toBe("sharpLeft");
    expect(classifyTurn(175)?.type).toBe("uturn");
  });
});

describe("collapseCloseManeuvers", () => {
  it("preserves single maneuver unchanged", () => {
    const geom = makeBendGeometry(200, 0, 250);
    const maneuvers = [{ id: "m1", distanceFromStartM: 100 }];
    const result = collapseCloseManeuvers(maneuvers, geom);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("m1");
  });

  it("returns empty for empty maneuvers", () => {
    const geom = makeBendGeometry(200, 0, 250);
    expect(collapseCloseManeuvers([], geom)).toHaveLength(0);
  });

  it("preserves maneuvers more than 5m apart", () => {
    const geom = makeBendGeometry(200, 0, 250);
    const maneuvers = [
      { id: "m1", distanceFromStartM: 100 },
      { id: "m2", distanceFromStartM: 110 },
    ];
    const result = collapseCloseManeuvers(maneuvers, geom);
    expect(result).toHaveLength(2);
  });

  it("collapses two close maneuvers when net angle exceeds 30 deg", () => {
    // Geometry bends 45° right at 105m. M1 at 100m, M2 at 103m (3m apart).
    // inBearing ~0° (north approach), outBearing ~45° → netAngle ~45° > 30°.
    const geom = makeBendGeometry(105, 45, 250);
    const maneuvers = [
      { id: "m1", distanceFromStartM: 100 },
      { id: "m2", distanceFromStartM: 103 },
    ];
    const result = collapseCloseManeuvers(maneuvers, geom);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("m2");
  });

  it("preserves two close maneuvers when net angle is under 30 deg", () => {
    // Geometry goes straight north (no bend). Both bearings ~0° → netAngle ~0°.
    const geom = makeBendGeometry(200, 0, 250);
    const maneuvers = [
      { id: "m1", distanceFromStartM: 100 },
      { id: "m2", distanceFromStartM: 103 },
    ];
    const result = collapseCloseManeuvers(maneuvers, geom);
    expect(result).toHaveLength(2);
  });

  it("collapses three close maneuvers with net angle >30 deg, keeps only last", () => {
    const geom = makeBendGeometry(105, 40, 250);
    const maneuvers = [
      { id: "m1", distanceFromStartM: 100 },
      { id: "m2", distanceFromStartM: 102 },
      { id: "m3", distanceFromStartM: 104 },
    ];
    const result = collapseCloseManeuvers(maneuvers, geom);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("m3");
  });

  it("handles gaps exactly at 5m as non-collapsible", () => {
    const geom = makeBendGeometry(105, 45, 250);
    const maneuvers = [
      { id: "m1", distanceFromStartM: 100 },
      { id: "m2", distanceFromStartM: 105 },
    ];
    const result = collapseCloseManeuvers(maneuvers, geom);
    // Gap is exactly 5m, which is NOT less than 5m → no collapse
    expect(result).toHaveLength(2);
  });
});
