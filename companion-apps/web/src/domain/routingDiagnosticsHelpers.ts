import type { RoutingDiagDebugPackage, RoutingDiagSession } from "./routingDiagnosticsModels.js";

export function sessionDebugPackage(session: RoutingDiagSession): string {
  const pkg: RoutingDiagDebugPackage = {
    formatVersion: 1,
    sessionId: session.id,
    createdAtMs: session.createdAtMs,
    eventCount: session.events.length,
    events: session.events,
    routeGeometries: session.routeGeometries,
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
