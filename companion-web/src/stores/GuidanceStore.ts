import { makeAutoObservable } from "mobx";
import {
  type ActiveRouteSession,
  type CoordinatePoint,
  EMPTY_ACTIVE_SESSION,
  type HomeCompassMode,
  type HomeMode,
  type NormalizedRoutePackage,
  type RouteManeuverType,
  type RouteSourceMode,
  selectedAlternative,
} from "../domain/models.js";
import {
  projectOntoPolyline,
  splitPolylineAtDistance,
  totalDistanceMeters,
} from "../integrations/geo.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import type { LocationStore } from "./LocationStore.js";
import type { PlanningStore } from "./PlanningStore.js";

export const DEFAULT_RIDER_FALLBACK: CoordinatePoint = {
  latitude: 60.1699,
  longitude: 24.9384,
};

// Thresholds matching runtime-core defaults
const OFF_ROUTE_ENTER_DISTANCE_M = 35;
const OFF_ROUTE_EXIT_DISTANCE_M = 22;
const MAJOR_TURN_ALERT_DISTANCE_M = 80;
const REROUTE_REQUEST_DELAY_MS = 2000;

export type TurnAlertKind = "left" | "right" | "uturn" | "generic";

export type UpcomingTurnAlert = {
  kind: TurnAlertKind;
  distanceRemainingM: number;
  instructionText?: string;
};

type StoredManeuver = {
  alertKind: TurnAlertKind | undefined;
  distanceAlongRouteM: number;
  instructionText?: string;
};

function turnAlertKindFromManeuverType(type: RouteManeuverType): TurnAlertKind | undefined {
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
    case "slightLeft":
    case "sharpLeft":
      return "left";
    case "right":
    case "slightRight":
    case "sharpRight":
      return "right";
    case "uturn":
      return "uturn";
  }
}

function buildStoredManeuvers(route: NormalizedRoutePackage): StoredManeuver[] {
  return route.maneuvers.map((m) => ({
    alertKind: turnAlertKindFromManeuverType(m.maneuverType),
    distanceAlongRouteM: m.distanceFromStartMeters,
    instructionText: m.instructionText,
  }));
}

function computeUpcomingTurnAlert(
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

export class GuidanceStore {
  homeMode: HomeMode = "planning";
  compassMode: HomeCompassMode = "autoFollow";
  activeSession: ActiveRouteSession = EMPTY_ACTIVE_SESSION;

  // Route progress state (ported from runtime-core ActiveRoute)
  progressDistanceM = 0;
  offRoute = false;
  offRouteDistanceM = 0;
  rerouteRequested = false;
  upcomingTurnAlert: UpcomingTurnAlert | undefined = undefined;

  private offRouteDurationMs = 0;
  private lastAdvanceTimestampMs = 0;
  private storedManeuvers: StoredManeuver[] = [];
  private routeTotalDistanceM = 0;
  private northPreviewTimeoutId?: ReturnType<typeof setTimeout>;
  private activeRouteGeometry: CoordinatePoint[] = [];

  constructor(
    private readonly planning: PlanningStore,
    private readonly persistence: LocalStoragePersistence,
    private readonly location: LocationStore,
  ) {
    this.activeSession = persistence.loadLastSession();
    makeAutoObservable(this, {}, { autoBind: true });
  }

  /** Best estimate of where the rider currently is. Falls back to last-known then default. */
  get riderLocation(): CoordinatePoint {
    return this.location.bestKnownLocation() ?? DEFAULT_RIDER_FALLBACK;
  }

  get guidanceRoute(): NormalizedRoutePackage | undefined {
    if (this.homeMode !== "phoneGuidance") return undefined;
    return selectedAlternative(this.planning.preview)?.normalizedPackage;
  }

  get nextInstructionLine(): string | undefined {
    const alert = this.upcomingTurnAlert;
    if (alert) {
      const label = formatDistanceLabel(alert.distanceRemainingM);
      return `${alert.instructionText ?? turnAlertLabel(alert.kind)} in ${label}`;
    }
    // Fallback: find next non-depart maneuver ahead of current progress
    const route = this.guidanceRoute;
    if (!route) return undefined;
    for (const m of route.maneuvers) {
      if (m.maneuverType === "depart" || m.maneuverType === "arrive") continue;
      const remaining = m.distanceFromStartMeters - this.progressDistanceM;
      if (remaining < 0) continue;
      const label = formatDistanceLabel(remaining);
      return `${m.instructionText ?? "Continue"} in ${label}`;
    }
    return undefined;
  }

  get activeNavigationTitle(): string {
    const route = this.guidanceRoute;
    return route?.summary.destinationLabel ?? this.activeSession.destinationLabel;
  }

  get activeNavigationSubtitle(): string {
    const remaining = this.remainingDistanceM;
    if (remaining > 0) {
      const km = (remaining / 1000).toFixed(1);
      const eta = this.remainingDurationSeconds;
      const minutes = Math.max(Math.ceil(eta / 60), 1);
      return `${km} km remaining • ${minutes} min`;
    }
    const route = this.guidanceRoute;
    if (!route) return "Phone guidance ready";
    const minutes = Math.max(Math.floor(route.summary.estimatedDurationSeconds / 60), 1);
    const km = (route.summary.totalDistanceMeters / 1000).toFixed(1);
    return `${km} km • ${minutes} min`;
  }

  /** Remaining distance along the route from current progress. */
  get remainingDistanceM(): number {
    if (this.routeTotalDistanceM <= 0) return 0;
    return Math.max(0, this.routeTotalDistanceM - this.progressDistanceM);
  }

  /** Estimated remaining duration based on proportion of route completed. */
  get remainingDurationSeconds(): number {
    const route = this.guidanceRoute;
    if (!route || this.routeTotalDistanceM <= 0) return 0;
    const fraction = Math.max(0, 1 - this.progressDistanceM / this.routeTotalDistanceM);
    return route.summary.estimatedDurationSeconds * fraction;
  }

  /** Completed and remaining route geometry split at current progress. */
  get routeSplit(): { completed: CoordinatePoint[]; remaining: CoordinatePoint[] } | undefined {
    if (this.activeRouteGeometry.length === 0) return undefined;
    return splitPolylineAtDistance(this.activeRouteGeometry, this.progressDistanceM);
  }

  /** Off-route status label for UI display. */
  get offRouteLabel(): string | undefined {
    if (this.rerouteRequested) return "Rerouting…";
    if (this.offRoute) return "Off route";
    return undefined;
  }

  startSelectedRoute(): void {
    const selected = selectedAlternative(this.planning.preview);
    if (!selected) return;
    const package_ = selected.normalizedPackage;
    this.activeSession = {
      routeIdentifier: package_.routeIdentifier,
      routeRevision: package_.revision,
      destinationLabel: package_.summary.destinationLabel ?? this.activeSession.destinationLabel,
      destinationCoordinate: package_.geometry[package_.geometry.length - 1],
      providerID: package_.provenance.providerID,
      sourceMode: this.planning.currentSourceMode,
    };
    this.persistence.saveLastSession(this.activeSession);
    this.homeMode = "phoneGuidance";
    this.compassMode = "autoFollow";
    this.resetProgress(package_);
  }

  stopGuidance(): void {
    this.homeMode = "planning";
    this.compassMode = "autoFollow";
    this.activeSession = {
      ...this.activeSession,
      routeIdentifier: undefined,
      routeRevision: undefined,
    };
    this.persistence.saveLastSession(this.activeSession);
    this.clearProgress();
    this.cancelNorthPreviewTimer();
  }

  /** Called on every GPS update during guidance to advance route-follow state. */
  advanceProgress(riderLocation: CoordinatePoint, timestampMs: number): void {
    if (this.homeMode !== "phoneGuidance") return;
    if (this.activeRouteGeometry.length < 2) return;

    const dt = this.lastAdvanceTimestampMs > 0 ? timestampMs - this.lastAdvanceTimestampMs : 0;
    this.lastAdvanceTimestampMs = timestampMs;

    // Project rider onto route
    const projection = projectOntoPolyline(this.activeRouteGeometry, riderLocation);

    // Monotonic progress (never regresses, like Rust implementation)
    this.progressDistanceM = Math.min(
      Math.max(this.progressDistanceM, projection.progressDistanceM),
      this.routeTotalDistanceM,
    );
    this.offRouteDistanceM = projection.distanceToRouteM;

    // Off-route detection with hysteresis
    const wasOffRoute = this.offRoute;
    if (this.offRoute) {
      if (projection.distanceToRouteM <= OFF_ROUTE_EXIT_DISTANCE_M) {
        this.offRoute = false;
      }
    } else if (projection.distanceToRouteM >= OFF_ROUTE_ENTER_DISTANCE_M) {
      this.offRoute = true;
    }

    // Reroute request after sustained off-route
    if (this.offRoute) {
      this.offRouteDurationMs = wasOffRoute ? this.offRouteDurationMs + dt : dt;
      if (this.offRouteDurationMs >= REROUTE_REQUEST_DELAY_MS) {
        this.rerouteRequested = true;
      }
    } else {
      this.offRouteDurationMs = 0;
      this.rerouteRequested = false;
    }

    // Upcoming turn alert
    this.upcomingTurnAlert = computeUpcomingTurnAlert(
      this.storedManeuvers,
      this.progressDistanceM,
      MAJOR_TURN_ALERT_DISTANCE_M,
    );
  }

  /** Reset progress when a new or replacement route is loaded. */
  resetProgress(route: NormalizedRoutePackage): void {
    this.activeRouteGeometry = route.geometry;
    this.routeTotalDistanceM = totalDistanceMeters(route.geometry);
    this.storedManeuvers = buildStoredManeuvers(route);
    this.progressDistanceM = 0;
    this.offRoute = false;
    this.offRouteDistanceM = 0;
    this.offRouteDurationMs = 0;
    this.rerouteRequested = false;
    this.lastAdvanceTimestampMs = 0;
    this.upcomingTurnAlert = undefined;
  }

  private clearProgress(): void {
    this.activeRouteGeometry = [];
    this.routeTotalDistanceM = 0;
    this.storedManeuvers = [];
    this.progressDistanceM = 0;
    this.offRoute = false;
    this.offRouteDistanceM = 0;
    this.offRouteDurationMs = 0;
    this.rerouteRequested = false;
    this.lastAdvanceTimestampMs = 0;
    this.upcomingTurnAlert = undefined;
  }

  setSourceModeOnSession(mode: RouteSourceMode): void {
    this.activeSession = { ...this.activeSession, sourceMode: mode };
    this.persistence.saveLastSession(this.activeSession);
  }

  /** Compass single-tap: temporary north-preview, only meaningful in phone guidance. */
  handleCompassTap(): void {
    if (this.homeMode !== "phoneGuidance") return;
    if (this.compassMode === "northLocked") {
      this.compassMode = "autoFollow";
      this.cancelNorthPreviewTimer();
      return;
    }
    this.compassMode = "northPreview";
    this.cancelNorthPreviewTimer();
    this.northPreviewTimeoutId = setTimeout(() => {
      this.compassMode = "autoFollow";
    }, 2500);
  }

  /** Compass double-tap: lock north-up. */
  handleCompassDoubleTap(): void {
    if (this.homeMode !== "phoneGuidance") return;
    this.compassMode = "northLocked";
    this.cancelNorthPreviewTimer();
  }

  private cancelNorthPreviewTimer(): void {
    if (this.northPreviewTimeoutId !== undefined) {
      clearTimeout(this.northPreviewTimeoutId);
      this.northPreviewTimeoutId = undefined;
    }
  }
}

function formatDistanceLabel(meters: number): string {
  if (meters >= 1000) return `${(meters / 1000).toFixed(1)} km`;
  return `${Math.round(meters)} m`;
}

function turnAlertLabel(kind: TurnAlertKind): string {
  switch (kind) {
    case "left":
      return "Turn left";
    case "right":
      return "Turn right";
    case "uturn":
      return "Make a U-turn";
    case "generic":
      return "Maneuver ahead";
  }
}
