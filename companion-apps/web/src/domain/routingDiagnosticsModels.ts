import type { CoordinatePoint } from "./models.js";

export type RoutingDiagEventData =
  | {
      kind: "locationUpdate";
      lat: number;
      lon: number;
      heading?: number;
      speed?: number;
      accuracyM?: number;
    }
  | {
      kind: "destinationChanged";
      label: string;
      lat: number;
      lon: number;
    }
  | {
      kind: "routeAlternativesSuggested";
      alternatives: Array<{ providerName: string; routeId: string; label: string }>;
    }
  | {
      kind: "routeSelected";
      alternativeId: string;
      providerName: string;
      routeId: string;
      label: string;
    }
  | {
      kind: "routeStarted";
    }
  | {
      kind: "routeStopped";
      reason?: string;
    }
  | {
      kind: "exploreAlternatives";
    }
  | {
      kind: "compassModeChanged";
      from: string;
      to: string;
    }
  | {
      kind: "audioCueDispatched";
      cueType: string;
      messageText: string;
    }
  | {
      kind: "nextTurnAlerted";
      instructionText: string;
      distanceRemainingM: number;
    }
  | {
      kind: "offRouteDetected";
      distanceM: number;
    }
  | {
      kind: "rerouteRequested";
    }
  | {
      kind: "rerouteCompleted";
      result: "success" | "failed";
    };

export interface RoutingDiagEvent {
  id: string;
  timestampMs: number;
  data: RoutingDiagEventData;
}

export interface RoutingDiagSession {
  id: string;
  createdAtMs: number;
  updatedAtMs: number;
  events: RoutingDiagEvent[];
  routeGeometries?: RouteGeometryEntry[];
}

export interface RouteGeometryEntry {
  routeId: string;
  providerName: string;
  geometry: CoordinatePoint[];
}

export interface RoutingDiagDebugPackage {
  formatVersion: number;
  sessionId: string;
  createdAtMs: number;
  eventCount: number;
  events: RoutingDiagEvent[];
  routeGeometries?: RouteGeometryEntry[];
}

export {
  LOCATION_EVENT_THROTTLE_MS,
  ROUTING_DIAGNOSTICS_SESSION_LIMIT,
  newEventId,
  newSessionId,
  sessionDebugPackage,
  sessionDurationMs,
} from "./routingDiagnosticsHelpers.js";
