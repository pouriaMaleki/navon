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

export type AnnotationSeverity = "bug" | "improvement" | "note";

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

export {
  ANNOTATION_SEVERITY_LABELS,
  ANNOTATION_TAG_LABELS,
  interpolateGps,
  sessionElapsed,
  sessionEndTime,
  sessionStartTime,
} from "./debuggerHelpers.js";
