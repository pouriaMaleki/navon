import type { CoordinatePoint, RouteManeuverType } from "../domain/models.js";

const METERS_PER_DEGREE_LAT = 111_320.0;

/**
 * Approximate bounding box for mainland Finland (including Åland). Digitransit's
 * `finland` router aggregates GTFS feeds nationwide, so we hide the HSL tab only
 * when either endpoint is outside the country.
 */
const FINLAND_BOUNDS = {
  minLat: 59.7,
  maxLat: 70.1,
  minLon: 19.0,
  maxLon: 31.7,
};

export function isInFinland(point: CoordinatePoint): boolean {
  return (
    point.latitude >= FINLAND_BOUNDS.minLat &&
    point.latitude <= FINLAND_BOUNDS.maxLat &&
    point.longitude >= FINLAND_BOUNDS.minLon &&
    point.longitude <= FINLAND_BOUNDS.maxLon
  );
}

export function approximateDistanceMeters(start: CoordinatePoint, end: CoordinatePoint): number {
  const latMeters = (end.latitude - start.latitude) * METERS_PER_DEGREE_LAT;
  const meanLat = ((start.latitude + end.latitude) / 2.0) * (Math.PI / 180.0);
  const lonMeters = (end.longitude - start.longitude) * Math.cos(meanLat) * METERS_PER_DEGREE_LAT;
  return Math.sqrt(latMeters * latMeters + lonMeters * lonMeters);
}

export function bearingDegrees(start: CoordinatePoint, end: CoordinatePoint): number {
  const latMeters = (end.latitude - start.latitude) * METERS_PER_DEGREE_LAT;
  const meanLat = ((start.latitude + end.latitude) / 2.0) * (Math.PI / 180.0);
  const lonMeters = (end.longitude - start.longitude) * Math.cos(meanLat) * METERS_PER_DEGREE_LAT;
  return (Math.atan2(lonMeters, latMeters) * 180.0) / Math.PI;
}

export function turnDeltaDegrees(
  previous: CoordinatePoint,
  current: CoordinatePoint,
  next: CoordinatePoint,
): number {
  const incoming = bearingDegrees(previous, current);
  const outgoing = bearingDegrees(current, next);
  let delta = outgoing - incoming;
  while (delta <= -180.0) delta += 360.0;
  while (delta > 180.0) delta -= 360.0;
  return delta;
}

export type ClassifiedTurn = { type: RouteManeuverType; instruction: string };

/** Distance to walk along the polyline for incoming/outgoing bearing at a
 *  maneuver point. 10 m is enough to see the junction entry/exit angle
 *  without blurring it with the approach curve. */
const MANEUVER_ANGLE_LOOK_DISTANCE_M = 10;

/** Find the index of the polyline point closest to a target coordinate. */
export function findClosestPointIndex(
  geometry: CoordinatePoint[],
  target: CoordinatePoint,
): number {
  if (geometry.length === 0) return 0;
  let best = 0;
  let bestDistSq = Infinity;
  for (let i = 0; i < geometry.length; i++) {
    const dlat = geometry[i].latitude - target.latitude;
    const dlon = geometry[i].longitude - target.longitude;
    const distSq = dlat * dlat + dlon * dlon;
    if (distSq < bestDistSq) {
      bestDistSq = distSq;
      best = i;
    }
  }
  return best;
}

export function walkAlongPolyline(
  geometry: CoordinatePoint[],
  cumulative: number[],
  startIndex: number,
  distanceM: number,
  direction: "backward" | "forward",
): CoordinatePoint {
  let remaining = distanceM;
  let idx = startIndex;
  while (remaining > 1e-6 && idx > 0 && idx < geometry.length - 1) {
    const nextIdx = direction === "backward" ? idx - 1 : idx + 1;
    if (nextIdx < 0 || nextIdx >= geometry.length) break;
    const segLen =
      direction === "backward"
        ? cumulative[idx] - cumulative[nextIdx]
        : cumulative[nextIdx] - cumulative[idx];
    if (segLen <= 1e-9) {
      idx = nextIdx;
      continue;
    }
    if (remaining >= segLen) {
      remaining -= segLen;
      idx = nextIdx;
    } else {
      const t = direction === "backward" ? remaining / segLen : remaining / segLen;
      const from = direction === "backward" ? geometry[idx] : geometry[idx];
      const to = direction === "backward" ? geometry[nextIdx] : geometry[nextIdx];
      return {
        latitude: from.latitude + (to.latitude - from.latitude) * t,
        longitude: from.longitude + (to.longitude - from.longitude) * t,
      };
    }
  }
  return geometry[idx];
}

/** Compute the turn angle at a maneuver location using points ~10m before
 *  and after on the geometry. Positive = right turn, negative = left turn. */
export function maneuverAngleDegrees(
  geometry: CoordinatePoint[],
  cumulative: number[],
  maneuverIndex: number,
): number {
  const maneuverPoint = geometry[maneuverIndex];
  const behind = walkAlongPolyline(geometry, cumulative, maneuverIndex, MANEUVER_ANGLE_LOOK_DISTANCE_M, "backward");
  const ahead = walkAlongPolyline(geometry, cumulative, maneuverIndex, MANEUVER_ANGLE_LOOK_DISTANCE_M, "forward");
  return turnDeltaDegrees(behind, maneuverPoint, ahead);
}

export function classifyTurn(deltaDegrees: number): ClassifiedTurn | null {
  const magnitude = Math.abs(deltaDegrees);
  if (magnitude < 25) return null;
  if (magnitude >= 170) return { type: "uturn", instruction: "Make a U-turn" };
  if (magnitude >= 110)
    return deltaDegrees > 0
      ? { type: "sharpRight", instruction: "Turn sharply right" }
      : { type: "sharpLeft", instruction: "Turn sharply left" };
  if (magnitude >= 50)
    return deltaDegrees > 0
      ? { type: "right", instruction: "Turn right" }
      : { type: "left", instruction: "Turn left" };
  return deltaDegrees > 0
    ? { type: "slightRight", instruction: "Slight right" }
    : { type: "slightLeft", instruction: "Slight left" };
}

export function deduplicateConsecutive(points: CoordinatePoint[]): CoordinatePoint[] {
  const out: CoordinatePoint[] = [];
  for (const p of points) {
    const last = out[out.length - 1];
    if (!last || last.latitude !== p.latitude || last.longitude !== p.longitude) {
      out.push(p);
    }
  }
  return out;
}

export function cumulativeDistances(geometry: CoordinatePoint[]): number[] {
  const cumulative = [0];
  for (let i = 1; i < geometry.length; i++) {
    cumulative.push(cumulative[i - 1] + approximateDistanceMeters(geometry[i - 1], geometry[i]));
  }
  return cumulative;
}

export function totalDistanceMeters(geometry: CoordinatePoint[]): number {
  let total = 0;
  for (let i = 1; i < geometry.length; i++) {
    total += approximateDistanceMeters(geometry[i - 1], geometry[i]);
  }
  return total;
}

// ── Route projection utilities (ported from runtime-core/src/route/mod.rs) ──

export type RouteProjection = {
  progressDistanceM: number;
  distanceToRouteM: number;
};

/**
 * Project a rider position onto a polyline, returning:
 * - progressDistanceM: distance along the polyline to the closest point
 * - distanceToRouteM: perpendicular distance from the rider to the closest point on the polyline
 */
export function projectOntoPolyline(
  geometry: CoordinatePoint[],
  rider: CoordinatePoint,
): RouteProjection {
  if (geometry.length === 0) return { progressDistanceM: 0, distanceToRouteM: 0 };
  if (geometry.length === 1)
    return {
      progressDistanceM: 0,
      distanceToRouteM: approximateDistanceMeters(geometry[0], rider),
    };

  let bestDistanceSq = Infinity;
  let bestProgressM = 0;
  let traversedM = 0;

  for (let i = 0; i < geometry.length - 1; i++) {
    const start = geometry[i];
    const end = geometry[i + 1];
    const segmentLengthM = approximateDistanceMeters(start, end);
    if (segmentLengthM <= 1e-9) continue;

    // Convert to local meter coordinates relative to segment start
    const meanLat = ((start.latitude + rider.latitude) / 2) * (Math.PI / 180);
    const cosLat = Math.cos(meanLat);
    const endX = (end.longitude - start.longitude) * cosLat * METERS_PER_DEGREE_LAT;
    const endY = (end.latitude - start.latitude) * METERS_PER_DEGREE_LAT;
    const riderX = (rider.longitude - start.longitude) * cosLat * METERS_PER_DEGREE_LAT;
    const riderY = (rider.latitude - start.latitude) * METERS_PER_DEGREE_LAT;

    // Project rider onto segment
    const segLenSq = endX * endX + endY * endY;
    let t = segLenSq > 1e-9 ? (riderX * endX + riderY * endY) / segLenSq : 0;
    t = Math.max(0, Math.min(1, t));
    const projX = t * endX;
    const projY = t * endY;
    const dx = riderX - projX;
    const dy = riderY - projY;
    const distSq = dx * dx + dy * dy;

    if (distSq < bestDistanceSq) {
      bestDistanceSq = distSq;
      bestProgressM = traversedM + segmentLengthM * t;
    }
    traversedM += segmentLengthM;
  }

  return {
    progressDistanceM: bestProgressM,
    distanceToRouteM: Math.sqrt(bestDistanceSq),
  };
}

/**
 * Split a polyline at a given distance, returning completed and remaining segments.
 * The split point is linearly interpolated on the current segment.
 */
export function splitPolylineAtDistance(
  geometry: CoordinatePoint[],
  progressDistanceM: number,
): { completed: CoordinatePoint[]; remaining: CoordinatePoint[] } {
  if (geometry.length === 0) return { completed: [], remaining: [] };
  if (geometry.length === 1) return { completed: [geometry[0]], remaining: [geometry[0]] };
  if (progressDistanceM <= 0) return { completed: [geometry[0]], remaining: [...geometry] };

  const total = totalDistanceMeters(geometry);
  if (progressDistanceM >= total)
    return { completed: [...geometry], remaining: [geometry[geometry.length - 1]] };

  const completed: CoordinatePoint[] = [geometry[0]];
  let traversedM = 0;

  for (let i = 0; i < geometry.length - 1; i++) {
    const start = geometry[i];
    const end = geometry[i + 1];
    const segLen = approximateDistanceMeters(start, end);
    if (segLen <= 1e-9) continue;

    const nextM = traversedM + segLen;
    if (progressDistanceM >= nextM) {
      completed.push(end);
      traversedM = nextM;
      continue;
    }

    const localT = Math.max(0, Math.min(1, (progressDistanceM - traversedM) / segLen));
    const splitPoint: CoordinatePoint = {
      latitude: start.latitude + (end.latitude - start.latitude) * localT,
      longitude: start.longitude + (end.longitude - start.longitude) * localT,
    };
    completed.push(splitPoint);
    const remaining: CoordinatePoint[] = [splitPoint];
    if (localT < 1) remaining.push(end);
    for (let j = i + 2; j < geometry.length; j++) remaining.push(geometry[j]);
    return { completed, remaining };
  }

  return { completed: [...geometry], remaining: [geometry[geometry.length - 1]] };
}

/** Decode a Google-style encoded polyline (precision 5). */
export function decodePolyline(encoded: string): CoordinatePoint[] {
  if (!encoded) return [];
  const points: CoordinatePoint[] = [];
  let index = 0;
  let latitude = 0;
  let longitude = 0;

  function nextValue(): number | null {
    let result = 0;
    let shift = 0;
    while (index < encoded.length) {
      const value = encoded.charCodeAt(index) - 63;
      index++;
      result |= (value & 0x1f) << shift;
      shift += 5;
      if (value < 0x20) {
        return (result & 1) === 0 ? result >> 1 : ~(result >> 1);
      }
    }
    return null;
  }

  while (index < encoded.length) {
    const latDelta = nextValue();
    if (latDelta === null) break;
    const lonDelta = nextValue();
    if (lonDelta === null) break;
    latitude += latDelta;
    longitude += lonDelta;
    points.push({ latitude: latitude / 100_000.0, longitude: longitude / 100_000.0 });
  }
  return points;
}

// ── Maneuver collapse ──

const COLLAPSE_DISTANCE_M = 5;
const COLLAPSE_ANGLE_DEG = 30;
const MANEUVER_LOOK_DIST_M = 10;

function coordAtDistance(geometry: CoordinatePoint[], dist: number, cumul: number[]): CoordinatePoint {
  if (dist <= 0) return geometry[0];
  const total = cumul[cumul.length - 1];
  if (dist >= total) return geometry[geometry.length - 1];
  for (let i = 1; i < cumul.length; i++) {
    if (cumul[i] >= dist) {
      const segLen = cumul[i] - cumul[i - 1];
      const t = segLen > 1e-9 ? (dist - cumul[i - 1]) / segLen : 0;
      return {
        latitude: geometry[i - 1].latitude + (geometry[i].latitude - geometry[i - 1].latitude) * t,
        longitude: geometry[i - 1].longitude + (geometry[i].longitude - geometry[i - 1].longitude) * t,
      };
    }
  }
  return geometry[geometry.length - 1];
}

/**
 * Collapse back-to-back maneuvers that are very close (<5m) when the net
 * direction change through them is >30°. Shared pedestrian path entries/exits
 * create multiple annotations but only the final real turn matters.
 */
export function collapseCloseManeuvers(
  maneuvers: { id: string; distanceFromStartM: number }[],
  geometry: CoordinatePoint[],
): { id: string; distanceFromStartM: number }[] {
  if (maneuvers.length < 2) return [...maneuvers];
  if (geometry.length < 2) return [...maneuvers];

  const cumul = cumulativeDistances(geometry);
  const totalDist = cumul[cumul.length - 1];

  function netAngleDeg(firstDist: number, lastDist: number): number {
    const approachFrom = coordAtDistance(geometry, Math.max(0, firstDist - MANEUVER_LOOK_DIST_M), cumul);
    const approachPt = coordAtDistance(geometry, firstDist, cumul);
    const exitPt = coordAtDistance(geometry, lastDist, cumul);
    const exitTo = coordAtDistance(geometry, Math.min(totalDist, lastDist + MANEUVER_LOOK_DIST_M), cumul);
    const inBearing = bearingDegrees(approachFrom, approachPt);
    const outBearing = bearingDegrees(exitPt, exitTo);
    let delta = outBearing - inBearing;
    while (delta <= -180) delta += 360;
    while (delta > 180) delta -= 360;
    return Math.abs(delta);
  }

  const result: typeof maneuvers = [];
  let i = 0;
  while (i < maneuvers.length) {
    let j = i + 1;
    while (
      j < maneuvers.length &&
      maneuvers[j].distanceFromStartM - maneuvers[j - 1].distanceFromStartM < COLLAPSE_DISTANCE_M
    ) {
      j++;
    }

    if (j - i > 1) {
      const firstDist = maneuvers[i].distanceFromStartM;
      const lastDist = maneuvers[j - 1].distanceFromStartM;
      if (netAngleDeg(firstDist, lastDist) > COLLAPSE_ANGLE_DEG) {
        result.push(maneuvers[j - 1]);
      } else {
        for (let k = i; k < j; k++) result.push(maneuvers[k]);
      }
    } else {
      result.push(maneuvers[i]);
    }
    i = j;
  }

  return result;
}
