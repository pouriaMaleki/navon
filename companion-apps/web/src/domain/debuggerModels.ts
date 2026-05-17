import type { CoordinatePoint } from "./models.js";
import type { RoutingDiagEvent, RoutingDiagSession } from "./routingDiagnosticsModels.js";

export type { CoordinatePoint };

export type AnnotationTag =
  | "wrong_cue"
  | "missing_cue"
  | "wrong_ui"
  | "missing_ui"
  | "wrong_timing"
  | "gps_issue"
  | "reroute_issue"
  | "other";

export const ANNOTATION_TAG_LABELS: Record<AnnotationTag, string> = {
  wrong_cue: "Wrong cue",
  missing_cue: "Missing cue",
  wrong_ui: "Wrong UI",
  missing_ui: "Missing UI",
  wrong_timing: "Wrong timing",
  gps_issue: "GPS issue",
  reroute_issue: "Reroute issue",
  other: "Other",
};

export type AnnotationSeverity = "bug" | "improvement" | "note";

export const ANNOTATION_SEVERITY_LABELS: Record<AnnotationSeverity, string> = {
  bug: "Bug",
  improvement: "Improvement",
  note: "Note",
};

export interface Annotation {
  id: string;
  tag: AnnotationTag;
  severity: AnnotationSeverity;
  timeRangeMs: [number, number];
  linkedEventIds: string[];
  coordinate?: CoordinatePoint;
  note: string;
  createdAtMs: number;
}

export interface DebuggerSession {
  diagSession: RoutingDiagSession;
  gpxGeometry?: CoordinatePoint[];
  annotations: Annotation[];
}

export interface AnnotationExport {
  formatVersion: number;
  sessionId: string;
  exportedAtMs: number;
  annotations: Annotation[];
  eventContext: Record<
    string,
    {
      before: RoutingDiagEvent[];
      during: RoutingDiagEvent[];
      after: RoutingDiagEvent[];
    }
  >;
}

export function sessionStartTime(session: RoutingDiagSession): number {
  if (session.events.length === 0) return session.createdAtMs;
  return session.events[0].timestampMs;
}

export function sessionEndTime(session: RoutingDiagSession): number {
  if (session.events.length === 0) return session.updatedAtMs;
  return session.events[session.events.length - 1].timestampMs;
}

export function sessionElapsed(session: RoutingDiagSession, timestampMs: number): number {
  return timestampMs - sessionStartTime(session);
}

export function interpolateGps(
  events: RoutingDiagEvent[],
  elapsedMs: number,
): CoordinatePoint | null {
  const locations = events
    .filter((e) => e.data.kind === "locationUpdate")
    .map((e) => ({
      ts: sessionElapsed(
        { events, createdAtMs: 0, updatedAtMs: 0, id: "" } as RoutingDiagSession,
        e.timestampMs,
      ),
      lat: (e.data as { kind: "locationUpdate"; lat: number; lon: number }).lat,
      lon: (e.data as { kind: "locationUpdate"; lat: number; lon: number }).lon,
    }));

  if (locations.length === 0) return null;
  if (locations.length === 1) return { latitude: locations[0].lat, longitude: locations[0].lon };

  let before = locations[0];
  let after = locations[0];
  for (const loc of locations) {
    if (loc.ts <= elapsedMs) {
      before = loc;
    } else {
      after = loc;
      break;
    }
  }

  if (before.ts === after.ts) {
    return { latitude: before.lat, longitude: before.lon };
  }

  const t = (elapsedMs - before.ts) / (after.ts - before.ts);
  const clamped = Math.max(0, Math.min(1, t));
  return {
    latitude: before.lat + (after.lat - before.lat) * clamped,
    longitude: before.lon + (after.lon - before.lon) * clamped,
  };
}
