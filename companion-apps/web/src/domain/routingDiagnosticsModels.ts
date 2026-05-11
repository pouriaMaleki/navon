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
}

export interface RoutingDiagDebugPackage {
  formatVersion: number;
  sessionId: string;
  createdAtMs: number;
  eventCount: number;
  events: RoutingDiagEvent[];
}

export function sessionDebugPackage(session: RoutingDiagSession): string {
  const pkg: RoutingDiagDebugPackage = {
    formatVersion: 1,
    sessionId: session.id,
    createdAtMs: session.createdAtMs,
    eventCount: session.events.length,
    events: session.events,
  };
  return JSON.stringify(pkg, null, 2);
}

export function sessionDurationMs(session: RoutingDiagSession): number {
  if (session.events.length === 0) return 0;
  return session.updatedAtMs - session.createdAtMs;
}

let nextEventId = 0;

export function newSessionId(): string {
  return `rd-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function newEventId(): string {
  nextEventId += 1;
  return `e${nextEventId}`;
}

export const ROUTING_DIAGNOSTICS_SESSION_LIMIT = 20;
export const LOCATION_EVENT_THROTTLE_MS = 5000;
