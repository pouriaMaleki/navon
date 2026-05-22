import { describe, expect, it } from "vitest";
import type { CoordinatePoint, RouteManeuver } from "../../domain/models.js";
import { filterGlitchClusters } from "./glitchTurnFilter.js";
import { cumulativeDistances } from "../geo.js";

const METERS_PER_DEG_LAT = 111_320;

function straightGeometry(lengthM: number, stepM = 5): CoordinatePoint[] {
  const points: CoordinatePoint[] = [];
  for (let d = 0; d <= lengthM; d += stepM) {
    points.push({ latitude: 60.17 + d / METERS_PER_DEG_LAT, longitude: 24.94 });
  }
  return points;
}

/**
 * Geometry with a localized bend at `bendAtM` by `totalBendDeg` degrees.
 * Before the bend: heading 0° (north). After: heading = totalBendDeg (clockwise).
 * The bend transition happens over 5m around `bendAtM`.
 */
function bentGeometry(
  lengthM: number,
  bendAtM: number,
  totalBendDeg: number,
  stepM = 5,
): CoordinatePoint[] {
  const bendRad = (totalBendDeg * Math.PI) / 180;
  const points: CoordinatePoint[] = [];
  let lat = 60.17;
  let lon = 24.94;
  points.push({ latitude: lat, longitude: lon });

  for (let d = stepM; d <= lengthM; d += stepM) {
    const t = Math.max(0, Math.min(1, (d - (bendAtM - 2.5)) / 5));
    const heading = bendRad * t;
    const cosLat = Math.cos((lat * Math.PI) / 180);
    lat += (Math.cos(heading) * stepM) / METERS_PER_DEG_LAT;
    lon += (Math.sin(heading) * stepM) / (METERS_PER_DEG_LAT * cosLat);
    points.push({ latitude: lat, longitude: lon });
  }
  return points;
}

/** Pick the geometry point closest to a given distance along the path. */
function pointAtDistance(geometry: CoordinatePoint[], targetDistM: number): CoordinatePoint {
  const cum = cumulativeDistances(geometry);
  for (let i = 0; i < geometry.length; i++) {
    if (cum[i] >= targetDistM - 1e-3) return geometry[i];
  }
  return geometry[geometry.length - 1];
}

function m(
  id: string,
  type: RouteManeuver["maneuverType"],
  dist: number,
  location?: CoordinatePoint,
): RouteManeuver {
  return {
    id,
    maneuverType: type,
    location: location ?? { latitude: 60.17 + dist / METERS_PER_DEG_LAT, longitude: 24.94 },
    distanceFromStartMeters: dist,
  };
}

function ids(maneuvers: RouteManeuver[]): string[] {
  return maneuvers.map((m) => m.id);
}

describe("filterGlitchClusters", () => {
  it("returns empty unchanged", () => {
    expect(filterGlitchClusters([], [])).toEqual([]);
  });

  it("returns single maneuver unchanged", () => {
    const maneuvers = [m("m1", "left", 100)];
    const geom = straightGeometry(200);
    expect(filterGlitchClusters(maneuvers, geom)).toEqual(maneuvers);
  });

  it("returns unchanged when geometry has < 2 points", () => {
    const maneuvers = [m("m1", "left", 100), m("m2", "left", 107)];
    expect(filterGlitchClusters(maneuvers, [{ latitude: 60.17, longitude: 24.94 }])).toEqual(
      maneuvers,
    );
  });

  it("removes two close maneuvers on a straight path", () => {
    const maneuvers = [m("m1", "left", 100), m("m2", "left", 107)];
    const geom = straightGeometry(200);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(result).toHaveLength(0);
  });

  it("preserves two close maneuvers when the path actually bends", () => {
    // Bend at 103.5m (between m1@100 and m2@107) so entry/exit bearings differ.
    const geom = bentGeometry(200, 103.5, 12);
    const maneuvers = [
      m("m1", "left", 100, pointAtDistance(geom, 100)),
      m("m2", "left", 107, pointAtDistance(geom, 107)),
    ];
    const result = filterGlitchClusters(maneuvers, geom);
    expect(result).toHaveLength(2);
    expect(ids(result)).toEqual(["m1", "m2"]);
  });

  it("removes a three-maneuver cluster on a straight path", () => {
    const maneuvers = [m("m1", "left", 100), m("m2", "right", 105), m("m3", "left", 112)];
    const geom = straightGeometry(200);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(result).toHaveLength(0);
  });

  it("preserves a three-maneuver cluster on a curved path (>=10deg)", () => {
    const geom = bentGeometry(200, 108.5, 15);
    const maneuvers = [
      m("m1", "left", 100, pointAtDistance(geom, 100)),
      m("m2", "right", 105, pointAtDistance(geom, 105)),
      m("m3", "left", 112, pointAtDistance(geom, 112)),
    ];
    const result = filterGlitchClusters(maneuvers, geom);
    expect(ids(result)).toEqual(["m1", "m2", "m3"]);
  });

  it("does not group maneuvers more than 10m apart", () => {
    const maneuvers = [m("m1", "left", 100), m("m2", "left", 115)];
    const geom = straightGeometry(200);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(ids(result)).toEqual(["m1", "m2"]);
  });

  it("handles two separate clusters independently", () => {
    const maneuvers = [
      m("m1", "left", 100),
      m("m2", "right", 107),
      m("m3", "left", 300),
      m("m4", "right", 306),
    ];
    const geom = straightGeometry(500);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(result).toHaveLength(0);
  });

  it("keeps the non-clustered maneuver after a removed cluster", () => {
    const maneuvers = [m("m1", "left", 100), m("m2", "right", 107), m("m3", "left", 300)];
    const geom = straightGeometry(500);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(ids(result)).toEqual(["m3"]);
  });

  it("preserves a glitch cluster that turns sharply enough", () => {
    const geom = bentGeometry(200, 103.5, 10.5);
    const maneuvers = [
      m("m1", "left", 100, pointAtDistance(geom, 100)),
      m("m2", "left", 107, pointAtDistance(geom, 107)),
    ];
    const result = filterGlitchClusters(maneuvers, geom);
    expect(ids(result)).toEqual(["m1", "m2"]);
  });

  it("handles cluster at route start", () => {
    const maneuvers = [m("depart", "depart", 0), m("m1", "left", 5)];
    const geom = straightGeometry(200);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(result).toHaveLength(0);
  });

  it("handles cluster at route end", () => {
    const maneuvers = [m("m1", "left", 180), m("m2", "right", 187), m("arrive", "arrive", 200)];
    const geom = straightGeometry(200);
    const result = filterGlitchClusters(maneuvers, geom);
    expect(ids(result)).toEqual(["arrive"]);
  });

  it("bend just above threshold is preserved", () => {
    const geom = bentGeometry(200, 103.5, 10.2);
    const maneuvers = [
      m("m1", "left", 100, pointAtDistance(geom, 100)),
      m("m2", "left", 107, pointAtDistance(geom, 107)),
    ];
    const result = filterGlitchClusters(maneuvers, geom);
    expect(ids(result)).toEqual(["m1", "m2"]);
  });
});
