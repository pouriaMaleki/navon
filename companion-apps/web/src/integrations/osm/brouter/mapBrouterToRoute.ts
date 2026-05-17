import {
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
  type RouteAlternative,
  type RouteManeuver,
  type RouteManeuverType,
  type RoutePlanRequest,
} from "../../../domain/models.js";
import { newAlternativeId } from "../../routePackage.js";
import type { BrouterFeature, BrouterProfile } from "./BrouterClient.js";

/**
 * BRouter `voicehints` encode turns as integer command codes. This mapping
 * is documented in BRouter's source (TIMode=2 / `VoiceHint` class). Codes
 * we don't recognise default to "straight" — better than dropping the
 * maneuver entirely.
 */
const BROUTER_CMD_TO_MANEUVER: Record<number, RouteManeuverType> = {
  1: "straight", // C / continue
  2: "left", // TL
  3: "slightLeft", // TSL (slight)
  4: "sharpLeft", // TSHL
  5: "right", // TR
  6: "slightRight", // TSR
  7: "sharpRight", // TSHR
  8: "slightLeft", // KL keep-left
  9: "slightRight", // KR keep-right
  10: "uturn", // U-turn
  12: "roundabout", // RNDB / roundabout entry (BRouter emits with exit count)
  13: "arrive", // TU / arrive
};

const CMD_INSTRUCTION: Record<number, string> = {
  1: "Continue",
  2: "Turn left",
  3: "Slight left",
  4: "Turn sharply left",
  5: "Turn right",
  6: "Slight right",
  7: "Turn sharply right",
  8: "Keep left",
  9: "Keep right",
  10: "Make a U-turn",
  12: "Enter roundabout",
  13: "Arrive at destination",
};

export type BrouterAlternativeLabel = {
  title: string;
  /** Used to dedupe near-identical alternatives across profiles. */
  profile: BrouterProfile;
};

/**
 * Convert a BRouter Feature into one `RouteAlternative` ready to drop
 * into a `RoutePreviewModel`. Pure function — no I/O. Tests can feed it
 * the canonical Phase-0 fixture.
 */
export function mapBrouterToAlternative(
  feature: BrouterFeature,
  request: RoutePlanRequest,
  revision: number,
  label: BrouterAlternativeLabel,
): RouteAlternative | null {
  const coords = feature.geometry.coordinates;
  if (!coords || coords.length < 2) return null;
  const geometry: CoordinatePoint[] = coords.map((c) => ({ longitude: c[0], latitude: c[1] }));
  const distanceMeters = parseNumberLike(feature.properties["track-length"]) ?? 0;
  const durationSeconds = Math.max(parseNumberLike(feature.properties["total-time"]) ?? 0, 60);
  const maneuvers = buildManeuversFromVoiceHints(geometry, feature.properties.voicehints ?? []);
  const package_: NormalizedRoutePackage = {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: brouterRouteId(request, label.profile),
    revision,
    geometry,
    maneuvers,
    summary: {
      totalDistanceMeters: distanceMeters,
      estimatedDurationSeconds: durationSeconds,
      startLabel: "Current location",
      destinationLabel: "Selected destination",
    },
    provenance: {
      providerID: "osm",
      sourceReference: `BRouter ${label.profile}`,
      generatedAtUnixMs: Date.now(),
    },
  };
  return {
    id: newAlternativeId(),
    title: label.title,
    subtitle: formatSubtitle(distanceMeters, durationSeconds),
    distanceMeters: Math.round(distanceMeters),
    durationSeconds: Math.round(durationSeconds),
    normalizedPackage: package_,
  };
}

function buildManeuversFromVoiceHints(
  geometry: CoordinatePoint[],
  voicehints: number[][],
): RouteManeuver[] {
  const maneuvers: RouteManeuver[] = [
    {
      id: "depart",
      maneuverType: "depart",
      location: geometry[0],
      distanceFromStartMeters: 0,
      distanceToNextMeters: voicehints[0]?.[3] ?? undefined,
      instructionText: "Start riding",
    },
  ];
  let cumulative = 0;
  for (let i = 0; i < voicehints.length; i++) {
    const hint = voicehints[i];
    if (!hint || hint.length < 4) continue;
    const [geomIdx, cmd, _exit, distToNext] = hint;
    cumulative = approxDistanceFromStart(geometry, geomIdx, cumulative);
    const type = BROUTER_CMD_TO_MANEUVER[cmd] ?? "straight";
    if (type === "straight") continue;
    const at = geometry[Math.min(Math.max(geomIdx, 0), geometry.length - 1)];
    maneuvers.push({
      id: `vh-${i}`,
      maneuverType: type,
      location: at,
      distanceFromStartMeters: cumulative,
      distanceToNextMeters: distToNext,
      instructionText: CMD_INSTRUCTION[cmd] ?? "Continue",
    });
  }
  // BRouter doesn't always emit an explicit arrive; append one so the
  // companion's nextInstructionLine fallback ("Arrive in X") works.
  maneuvers.push({
    id: "arrive",
    maneuverType: "arrive",
    location: geometry[geometry.length - 1],
    distanceFromStartMeters: polylineLengthMeters(geometry),
    instructionText: "Arrive at destination",
  });
  return maneuvers;
}

/**
 * Walk the polyline up to (but not past) `targetIdx` to estimate the
 * cumulative distance from start. Reuses the previous cumulative so we
 * don't recompute the prefix every hint.
 */
function approxDistanceFromStart(
  geometry: CoordinatePoint[],
  targetIdx: number,
  cumulativeSoFar: number,
): number {
  // Naive: rebuild from 0 each time. Cheap for sub-200-segment routes.
  let total = 0;
  const cap = Math.min(Math.max(targetIdx, 1), geometry.length - 1);
  for (let i = 1; i <= cap; i++) total += haversine(geometry[i - 1], geometry[i]);
  return Math.max(total, cumulativeSoFar);
}

function polylineLengthMeters(geometry: CoordinatePoint[]): number {
  let total = 0;
  for (let i = 1; i < geometry.length; i++) total += haversine(geometry[i - 1], geometry[i]);
  return total;
}

function haversine(a: CoordinatePoint, b: CoordinatePoint): number {
  const metersPerDegLat = 111_320.0;
  const meanLatRad = ((a.latitude + b.latitude) / 2) * (Math.PI / 180);
  const dN = (b.latitude - a.latitude) * metersPerDegLat;
  const dE = (b.longitude - a.longitude) * Math.cos(meanLatRad) * metersPerDegLat;
  return Math.sqrt(dN * dN + dE * dE);
}

function parseNumberLike(value: unknown): number | null {
  if (typeof value === "number") return value;
  if (typeof value === "string") {
    const n = Number.parseFloat(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

function formatSubtitle(distanceM: number, durationS: number): string {
  const km = (distanceM / 1000).toFixed(1);
  const min = Math.max(Math.round(durationS / 60), 1);
  return `${km} km • ${min} min`;
}

function brouterRouteId(request: RoutePlanRequest, profile: BrouterProfile): string {
  const o = `${request.origin.latitude.toFixed(5)},${request.origin.longitude.toFixed(5)}`;
  const d = `${request.destination.latitude.toFixed(5)},${request.destination.longitude.toFixed(5)}`;
  return `osm-brouter-${profile}:${o}->${d}`;
}
