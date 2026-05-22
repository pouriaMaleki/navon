import type { CoordinatePoint, NormalizedRoutePackage, RouteManeuverType } from "../domain/models.js";
import { filterGlitchClusters } from "../integrations/cues/glitchTurnFilter.js";

export const DEFAULT_RIDER_FALLBACK: CoordinatePoint = {
  latitude: 60.1699,
  longitude: 24.9384,
};

// Thresholds matching runtime-core defaults
export const OFF_ROUTE_ENTER_DISTANCE_M = 35;
export const OFF_ROUTE_EXIT_DISTANCE_M = 22;
export const MAJOR_TURN_ALERT_DISTANCE_M = 80;
export const REROUTE_REQUEST_DELAY_MS = 2000;
export const REROUTING_BACKOFF_WINDOW_MS = 30_000;
export const REROUTING_THROTTLE_AT_ATTEMPTS = 3;
export const REROUTING_ESCALATE_AT_ATTEMPTS = 5;
export const REROUTING_BACKOFF_DELAY_MS = 5_000;
export const REROUTING_BACKOFF_LONG_DELAY_MS = 10_000;
export const ARRIVAL_RADIUS_M = 25;

export type TurnAlertKind = "left" | "right" | "slightLeft" | "slightRight" | "uturn" | "generic";

export type UpcomingTurnAlert = {
  kind: TurnAlertKind;
  distanceRemainingM: number;
  instructionText?: string;
};

export type StoredManeuver = {
  alertKind: TurnAlertKind | undefined;
  distanceAlongRouteM: number;
  instructionText?: string;
};

export function turnAlertKindFromManeuverType(type: RouteManeuverType): TurnAlertKind | undefined {
  switch (type) {
    case "depart":
    case "straight":
    case "arrive":
      return undefined;
    case "roundabout":
    case "merge":
    case "ramp":
      return "generic";
    case "left":
    case "sharpLeft":
      return "left";
    case "slightLeft":
      return "slightLeft";
    case "right":
    case "sharpRight":
      return "right";
    case "slightRight":
      return "slightRight";
    case "uturn":
      return "uturn";
  }
}

export function buildStoredManeuvers(route: NormalizedRoutePackage): StoredManeuver[] {
  return filterGlitchClusters(route.maneuvers, route.geometry).map((m) => ({
    alertKind: turnAlertKindFromManeuverType(m.maneuverType),
    distanceAlongRouteM: m.distanceFromStartMeters,
    instructionText: m.instructionText,
  }));
}

export function computeUpcomingTurnAlert(
  maneuvers: StoredManeuver[],
  progressDistanceM: number,
  thresholdM: number,
): UpcomingTurnAlert | undefined {
  for (const m of maneuvers) {
    if (!m.alertKind) continue;
    const remaining = m.distanceAlongRouteM - progressDistanceM;
    if (remaining < 0) continue;
    if (remaining > thresholdM) return undefined;
    return {
      kind: m.alertKind,
      distanceRemainingM: remaining,
      instructionText: m.instructionText,
    };
  }
  return undefined;
}

export function formatDistanceLabel(meters: number): string {
  if (meters >= 1000) return `${(meters / 1000).toFixed(1)} km`;
  return `${Math.round(meters)} m`;
}

export function turnAlertLabel(kind: TurnAlertKind): string {
  switch (kind) {
    case "left": return "Turn left";
    case "right": return "Turn right";
    case "slightLeft": return "Slight left";
    case "slightRight": return "Slight right";
    case "uturn": return "U-turn";
    case "generic": return "Turn";
  }
}
