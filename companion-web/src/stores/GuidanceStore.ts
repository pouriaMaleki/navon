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
  bearingDegrees,
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
  private recenterListeners: Set<() => void> = new Set();
  private fitRouteListeners: Set<() => void> = new Set();
  private mapInteractionTimeoutId?: ReturnType<typeof setTimeout>;
  /**
   * Pinned auto-recenter delay for user map interactions during routing.
   * Mirrors `recenter_inactivity_ms` in parity-fixtures/data/ux-constants.toml
   * (spec line 104). Kept as a field so tests can pull it via reflection.
   */
  private readonly mapInteractionRecenterMs = 1300;

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

  /**
   * Bearing (clockwise from true north, degrees) of the route segment the
   * rider is currently progressed onto. Spec line 101: "camera rotates so
   * that immediate route direction is towards top of the screen (riding
   * towards, even when stationary yet)". Returns `undefined` when there's
   * no active geometry.
   */
  get routingBearingDegrees(): number | undefined {
    const geometry = this.activeRouteGeometry;
    if (geometry.length < 2) return undefined;
    // Walk the polyline accumulating distance until we find the segment
    // containing `progressDistanceM`. Return its start→end bearing.
    let traversed = 0;
    for (let i = 0; i < geometry.length - 1; i++) {
      const segStart = geometry[i];
      const segEnd = geometry[i + 1];
      const metersPerDegreeLat = 111_320.0;
      const latMeters = (segEnd.latitude - segStart.latitude) * metersPerDegreeLat;
      const meanLat = ((segStart.latitude + segEnd.latitude) / 2) * (Math.PI / 180);
      const lonMeters =
        (segEnd.longitude - segStart.longitude) * Math.cos(meanLat) * metersPerDegreeLat;
      const segLen = Math.sqrt(latMeters * latMeters + lonMeters * lonMeters);
      if (segLen <= 1e-6) continue;
      // Strict `<` so progress exactly at a vertex snaps to the NEXT
      // segment — that's the segment the rider is about to traverse
      // ("riding towards", spec line 101).
      if (this.progressDistanceM < traversed + segLen) {
        return bearingDegrees(segStart, segEnd);
      }
      traversed += segLen;
    }
    // Past the end: return the last segment's bearing.
    const last = geometry[geometry.length - 2];
    const end = geometry[geometry.length - 1];
    return bearingDegrees(last, end);
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
    // Spec: on Start, the camera snaps onto the rider with routing zoom and
    // bearing-up. Emit a follow-rider recenter so RootStore can react. iOS
    // gets this for free through `refreshCameraForCurrentMode()`; web
    // needs an explicit signal.
    this.emitRecenterRequested();
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
    this.cancelMapInteractionTimer();
    // Spec line 85: "pressing stop button will take user back to suggested
    // routes" — return to the route-overview camera so the user sees the
    // same routes from before Start.
    this.emitFitRouteRequested();
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

    // Spec line 84: the camera should follow the rider (bottom-quarter
    // anchor, heading-up). Emit a recenter so RootStore can update
    // MapCameraStore on every GPS tick — but ONLY while the compass is in
    // `autoFollow`. `northPreview` and `northLocked` mean "show route
    // overview", so GPS ticks must not snap the camera onto the rider and
    // break the overview the user just asked for.
    if (this.compassMode === "autoFollow") {
      this.emitRecenterRequested();
    }
  }

  /**
   * Notify the store that the user moved the map (pan / pinch / rotate)
   * during routing. After `mapInteractionRecenterMs` of no further
   * interaction, the camera auto-recenters to the routing default.
   * Spec line 104.
   */
  noteUserMapInteraction(): void {
    if (this.homeMode !== "phoneGuidance") return;
    this.cancelMapInteractionTimer();
    this.mapInteractionTimeoutId = setTimeout(() => {
      this.mapInteractionTimeoutId = undefined;
      if (this.homeMode !== "phoneGuidance") return;
      // Route the recenter target off the CURRENT compass mode — not the
      // mode at scheduling time — because the user may have tapped the
      // compass mid-inactivity.
      //   autoFollow   → follow-rider (spec line 104).
      //   northLocked  → re-fit route overview (the lock means "stay in
      //                  overview"; regression for '🧭 reverts after 1.3s').
      //   northPreview → no-op; the 2.5 s preview timer already handles the
      //                  return-to-follow, and we don't want a second
      //                  recenter racing it.
      if (this.compassMode === "autoFollow") {
        this.emitRecenterRequested();
      } else if (this.compassMode === "northLocked") {
        this.emitFitRouteRequested();
      }
    }, this.mapInteractionRecenterMs);
  }

  private cancelMapInteractionTimer(): void {
    if (this.mapInteractionTimeoutId !== undefined) {
      clearTimeout(this.mapInteractionTimeoutId);
      this.mapInteractionTimeoutId = undefined;
    }
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

  /**
   * Public entry for "the user asked for a recenter right now." Used by the
   * on-screen recenter button during routing — delegates to the same emit
   * path that Start/GPS-ticks/inactivity-timeout use, which is important
   * because those callbacks pin the route-direction bearing (spec line 101).
   * A direct `mapCameraStore.setCenter` from the UI would bypass that.
   */
  requestRecenter(): void {
    if (this.homeMode !== "phoneGuidance") return;
    this.emitRecenterRequested();
  }

  /**
   * Subscribe to "follow rider" camera intents — the map should center on
   * the rider with routing zoom and bearing-up. Fires on startSelectedRoute,
   * on every GPS tick during guidance, after the map-interaction inactivity
   * timeout, and when the compass returns from `northLocked` → `autoFollow`.
   * Returns an unsubscribe function.
   */
  onRecenterRequested(callback: () => void): () => void {
    this.recenterListeners.add(callback);
    return () => {
      this.recenterListeners.delete(callback);
    };
  }

  /**
   * Subscribe to "route overview" camera intents — the map should fit the
   * entire route geometry in view, north-up. Fires on compass single-tap
   * (entering `northPreview`) and double-tap (entering `northLocked`),
   * mirroring iOS's `fitCamera(to: route.geometry)` path. Spec lines 93-96.
   */
  onFitRouteRequested(callback: () => void): () => void {
    this.fitRouteListeners.add(callback);
    return () => {
      this.fitRouteListeners.delete(callback);
    };
  }

  private emitRecenterRequested(): void {
    for (const cb of this.recenterListeners) cb();
  }

  private emitFitRouteRequested(): void {
    for (const cb of this.fitRouteListeners) cb();
  }

  /** Compass single-tap: temporary north-preview, only meaningful in phone guidance. */
  handleCompassTap(): void {
    if (this.homeMode !== "phoneGuidance") return;
    if (this.compassMode === "northLocked") {
      // Return to follow-rider mode — request a rider-centered camera.
      this.compassMode = "autoFollow";
      this.cancelNorthPreviewTimer();
      this.emitRecenterRequested();
      return;
    }
    // Entering (or resetting) northPreview — show the route overview.
    this.compassMode = "northPreview";
    this.cancelNorthPreviewTimer();
    this.northPreviewTimeoutId = setTimeout(() => {
      this.compassMode = "autoFollow";
      // Timer elapsed: camera should smoothly snap back to follow-rider.
      this.emitRecenterRequested();
    }, 2500);
    this.emitFitRouteRequested();
  }

  /** Compass double-tap: lock north-up. */
  handleCompassDoubleTap(): void {
    if (this.homeMode !== "phoneGuidance") return;
    this.compassMode = "northLocked";
    this.cancelNorthPreviewTimer();
    this.emitFitRouteRequested();
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
