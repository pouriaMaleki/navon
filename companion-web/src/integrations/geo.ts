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
