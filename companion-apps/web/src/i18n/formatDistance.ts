// Distance formatting helpers — bridges raw meters to the
// (number, unit) tuple consumed by the cue-engine ICU template
// `cue.turn50m.left`, and produces ready-to-render UI labels via
// `units.distance.*` keys.

import { type Locale, t } from "./index.js";

export type DistanceMode = "metric" | "imperial";

const FT_PER_M = 3.280839895;
const MI_PER_M = 0.0006213712;
const KM_THRESHOLD_M = 1000;
const MI_THRESHOLD_M = 1609; // ~1 mile

/**
 * Round to the nearest 10 — matches the cue engine's existing
 * "X meters" rounding so audio cues stay smooth.
 */
export function roundTo10(n: number): number {
  return Math.round(n / 10) * 10;
}

/**
 * Cue-engine helper: produce the ICU placeholder bundle for a distance
 * voice cue. In metric mode emits `{ distance: 50, distanceUnit: "meters" }`;
 * in imperial mode converts to feet and rounds.
 */
export function distanceCueValues(
  meters: number,
  mode: DistanceMode,
): { distance: number; distanceUnit: "meters" | "feet" | "kilometers" } {
  if (mode === "imperial") {
    return { distance: roundTo10(meters * FT_PER_M), distanceUnit: "feet" };
  }
  if (meters >= KM_THRESHOLD_M) {
    const km = Math.round(meters / 100) / 10; // one decimal
    return { distance: km, distanceUnit: "kilometers" };
  }
  return { distance: roundTo10(meters), distanceUnit: "meters" };
}

/**
 * UI helper: produce a "8.6 km" / "5.4 mi" label for arbitrary distances,
 * choosing meters/feet for short distances and km/mi above ~1 unit.
 */
export function formatDistanceLabel(meters: number, mode: DistanceMode, _locale?: Locale): string {
  if (mode === "imperial") {
    if (meters >= MI_THRESHOLD_M) {
      const miles = meters * MI_PER_M;
      return t("units.distance.mi", { distance: Math.round(miles * 10) / 10 });
    }
    return t("units.distance.ft", { distance: Math.round(meters * FT_PER_M) });
  }
  if (meters >= KM_THRESHOLD_M) {
    const km = meters / 1000;
    return t("units.distance.km", { distance: Math.round(km * 10) / 10 });
  }
  return t("units.distance.m", { distance: Math.round(meters) });
}

/**
 * UI helper: same as `formatDistanceLabel` but returns just the unit value
 * keyword used by `guidance.distanceToDestination`'s ICU select arm.
 */
export function distanceLabelUnit(meters: number, mode: DistanceMode): "km" | "mi" | "m" | "ft" {
  if (mode === "imperial") {
    return meters >= MI_THRESHOLD_M ? "mi" : "ft";
  }
  return meters >= KM_THRESHOLD_M ? "km" : "m";
}
