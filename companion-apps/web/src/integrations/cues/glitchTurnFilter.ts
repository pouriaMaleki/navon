import type { CoordinatePoint, RouteManeuver } from "../../domain/models.js";
import {
  bearingDegrees,
  cumulativeDistances,
  findClosestPointIndex,
  walkAlongPolyline,
} from "../geo.js";

const GLITCH_CLUSTER_MAX_GAP_M = 10;
const GLITCH_ANGLE_THRESHOLD_DEG = 10;
const GLITCH_ANGLE_LOOK_DISTANCE_M = 10;

/**
 * Remove clusters of consecutive maneuvers that are map glitches:
 * ≥2 maneuvers each within 10m of the previous, where the net path
 * direction change from before the cluster to after it is < 10°.
 */
export function filterGlitchClusters(
  maneuvers: RouteManeuver[],
  geometry: CoordinatePoint[],
): RouteManeuver[] {
  if (maneuvers.length < 2 || geometry.length < 2) return maneuvers;

  const cumDist = cumulativeDistances(geometry);
  if (cumDist.length === 0) return maneuvers;

  const result = [...maneuvers];
  let i = 0;
  while (i < result.length) {
    if (i + 1 >= result.length) break;
    const gap = result[i + 1].distanceFromStartMeters - result[i].distanceFromStartMeters;
    if (gap > GLITCH_CLUSTER_MAX_GAP_M) {
      i++;
      continue;
    }

    // Extend cluster forward while consecutive gaps ≤ threshold
    let clusterEnd = i + 1;
    while (clusterEnd + 1 < result.length) {
      const nextGap =
        result[clusterEnd + 1].distanceFromStartMeters - result[clusterEnd].distanceFromStartMeters;
      if (nextGap > GLITCH_CLUSTER_MAX_GAP_M) break;
      clusterEnd++;
    }

    const clusterSize = clusterEnd - i + 1;
    if (clusterSize < 2) {
      i = clusterEnd + 1;
      continue;
    }

    // Compute net direction change before→after the cluster
    const firstIdx = findClosestPointIndex(geometry, result[i].location);
    const lastIdx = findClosestPointIndex(geometry, result[clusterEnd].location);

    if (firstIdx >= 0 && firstIdx < geometry.length && lastIdx >= 0 && lastIdx < geometry.length) {
      const entryApproach = walkAlongPolyline(
        geometry,
        cumDist,
        firstIdx,
        GLITCH_ANGLE_LOOK_DISTANCE_M,
        "backward",
      );
      const exitDepart = walkAlongPolyline(
        geometry,
        cumDist,
        lastIdx,
        GLITCH_ANGLE_LOOK_DISTANCE_M,
        "forward",
      );
      const entryBearing = bearingDegrees(entryApproach, geometry[firstIdx]);
      const exitBearing = bearingDegrees(geometry[lastIdx], exitDepart);
      let delta = Math.abs(exitBearing - entryBearing);
      if (delta > 180) delta = 360 - delta;

      if (delta < GLITCH_ANGLE_THRESHOLD_DEG) {
        result.splice(i, clusterSize);
        continue; // re-evaluate this position after removal
      }
    }
    i = clusterEnd + 1;
  }
  return result;
}
