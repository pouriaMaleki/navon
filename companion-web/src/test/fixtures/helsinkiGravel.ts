import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { CoordinatePoint } from "../../domain/models.js";

export type RideSample = {
  latitude: number;
  longitude: number;
  speedMps: number;
  courseRad: number;
  accuracyM: number;
  timeOffsetMs: number;
};

let streamCache: RideSample[] | null = null;
let routeCache: CoordinatePoint[] | null = null;

export function loadHelsinkiGravelStream(): RideSample[] {
  if (streamCache) return streamCache;
  const text = readFileSync(fixturePath("stream.jsonl"), "utf-8");
  streamCache = text
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => {
      const parsed = JSON.parse(line);
      return {
        latitude: parsed.lat_deg,
        longitude: parsed.lon_deg,
        speedMps: parsed.speed_mps,
        courseRad: parsed.course_rad,
        accuracyM: parsed.accuracy_m,
        timeOffsetMs: parsed.t_ms,
      } satisfies RideSample;
    });
  return streamCache;
}

export function loadHelsinkiGravelRoute(): CoordinatePoint[] {
  if (routeCache) return routeCache;
  const text = readFileSync(fixturePath("route.geojson"), "utf-8");
  const parsed = JSON.parse(text);
  const coordinates = parsed.geometry?.coordinates ?? [];
  routeCache = coordinates.map(([lon, lat]: [number, number]) => ({
    latitude: lat,
    longitude: lon,
  }));
  return routeCache as CoordinatePoint[];
}

function fixturePath(name: string): string {
  const thisFile = fileURLToPath(import.meta.url);
  return resolve(
    dirname(thisFile),
    "../../../../parity-fixtures/data/helsinki-gravel",
    name,
  );
}
