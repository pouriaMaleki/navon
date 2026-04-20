import type { CoordinatePoint, RouteManeuverType } from "../domain/models.js";

const METERS_PER_DEGREE_LAT = 111_320.0;

/**
 * Approximate bounding box for the Uusimaa region of Finland (Helsinki, Espoo, Vantaa,
 * Porvoo, Hanko, Loviisa, etc.). HSL Digitransit only meaningfully covers this area, so
 * we hide the HSL tab when either endpoint is outside the box.
 */
const UUSIMAA_BOUNDS = {
  minLat: 59.8,
  maxLat: 60.8,
  minLon: 23.3,
  maxLon: 26.7,
};

export function isInUusimaa(point: CoordinatePoint): boolean {
  return (
    point.latitude >= UUSIMAA_BOUNDS.minLat &&
    point.latitude <= UUSIMAA_BOUNDS.maxLat &&
    point.longitude >= UUSIMAA_BOUNDS.minLon &&
    point.longitude <= UUSIMAA_BOUNDS.maxLon
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
    ? { type: "slightRight", instruction: "Bear right" }
    : { type: "slightLeft", instruction: "Bear left" };
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
