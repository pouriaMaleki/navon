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
  approximateDistanceMeters,
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
/** Sliding window over which past reroute attempts contribute to the backoff
 *  delay. Older attempts age out and stop counting. */
const REROUTING_BACKOFF_WINDOW_MS = 30_000;
/** Throttle threshold: at this many attempts in the window, hold the next
 *  auto-reroute by REROUTING_BACKOFF_DELAY_MS. */
const REROUTING_THROTTLE_AT_ATTEMPTS = 3;
/** Escalation threshold: at this many attempts, hold for REROUTING_BACKOFF_LONG_DELAY_MS. */
const REROUTING_ESCALATE_AT_ATTEMPTS = 5;
const REROUTING_BACKOFF_DELAY_MS = 5_000;
const REROUTING_BACKOFF_LONG_DELAY_MS = 10_000;
/**
 * Arrival is declared when the rider is within this many metres of the route
 * destination. Larger than the off-route exit distance so a rider drifting
 * around the destination doesn't bounce between "off route" and "arrived".
 */
const ARRIVAL_RADIUS_M = 25;

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
  /**
   * True while the user is browsing alternative routes launched from active
   * guidance (the "split icon" flow). Guidance keeps running — homeMode stays
   * "phoneGuidance" — but the route-suggestions card is shown so the rider can
   * pick a different route or dismiss back to the original.
   */
  isExploringAlternativesFromGuidance = false;
  /** Which alternative the user has explicitly tapped during exploration.
   *  Nil on enter and cleared on exit so the card starts with no checkmark. */
  private explorationSelectedID: string | undefined = undefined;
  /** Frozen snapshot of the active route when exploration begins.
   *  Prevents guidanceRoute from flipping to a browsed alternative and
   *  breaking GPS progress projection and the map's green-route polyline. */
  private activeRoutePackage: NormalizedRoutePackage | undefined = undefined;

  // Route progress state (ported from runtime-core ActiveRoute)
  progressDistanceM = 0;
  offRoute = false;
  offRouteDistanceM = 0;
  rerouteRequested = false;
  /** Sliding-window log of timestamps when an auto-reroute was attempted.
   *  Used to compute the backoff delay; entries older than the window age
   *  out automatically on every `recordReroutingAttempt` call. */
  reroutingAttemptTimestamps: number[] = [];
  /** Wall-clock millisecond timestamp at which the currently-deferred auto
   *  reroute will fire, or `undefined` if no reroute is being held back.
   *  Drives the "Waiting to reroute" UI and the manual-override button. */
  reroutingDelayedUntilMs: number | undefined = undefined;
  upcomingTurnAlert: UpcomingTurnAlert | undefined = undefined;
  /**
   * Set to a banner message when the rider has reached the destination and
   * guidance has been auto-stopped. Cleared when a new route starts.
   */
  arrivalNotice: string | undefined = undefined;

  /**
   * Dismiss-after timer for `arrivalNotice`. The banner used to persist
   * forever; if the rider then started picking a new destination, the banner
   * still won the BottomOverlay z-order against the new suggestions card.
   * Sixty seconds matches the iOS/Android auto-dismiss so platform behavior
   * stays in lock-step.
   */
  private arrivalNoticeTimeoutId?: ReturnType<typeof setTimeout>;
  private static readonly arrivalNoticeAutoDismissMs = 60_000;

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
    if (this.isExploringAlternativesFromGuidance) return this.activeRoutePackage;
    return selectedAlternative(this.planning.preview)?.normalizedPackage;
  }

  get nextInstructionLine(): string | undefined {
    // iOS parity: format is "<distance> <instruction>" so the eye lands
    // on the metric first (matches the routing top card's other two
    // lines: "8.6 km to Alppila", "16 min remaining").
    const alert = this.upcomingTurnAlert;
    if (alert) {
      const label = formatDistanceLabel(alert.distanceRemainingM);
      const instruction = alert.instructionText ?? turnAlertLabel(alert.kind);
      return `${label} ${instruction}`;
    }
    const route = this.guidanceRoute;
    if (!route) return undefined;
    for (const m of route.maneuvers) {
      if (m.maneuverType === "depart" || m.maneuverType === "arrive") continue;
      const remaining = m.distanceFromStartMeters - this.progressDistanceM;
      if (remaining < 0) continue;
      const label = formatDistanceLabel(remaining);
      return `${label} ${m.instructionText ?? "Continue"}`;
    }
    return undefined;
  }

  get activeNavigationTitle(): string {
    const placeholder = new Set(["", "No destination", "Selected destination", "Current location"]);
    const routeLabel = (this.guidanceRoute?.summary.destinationLabel ?? "").trim();
    if (routeLabel && !placeholder.has(routeLabel)) return routeLabel;
    return this.activeSession.destinationLabel;
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

  /**
   * Legacy single-line subtitle. Kept for back-compat callers; the
   * routing top card now renders the two new lines below directly.
   */
  get guidanceSubtitleLine(): string {
    const lines = [this.distanceToDestinationLine, this.minutesRemainingLine].filter(
      (line) => line.length > 0,
    );
    return lines.join(" • ") || this.activeNavigationSubtitle;
  }

  /**
   * iOS-parity routing top card line 2: "8.6 km to Alppila".
   * Prefers the user-typed destination on `activeSession` over the route
   * package's hardcoded placeholder so OSRM bike's "Selected destination"
   * never erases the address the rider actually picked.
   */
  get distanceToDestinationLine(): string {
    const remaining =
      this.remainingDistanceM > 0
        ? this.remainingDistanceM
        : (this.guidanceRoute?.summary.totalDistanceMeters ?? 0);
    if (remaining <= 0) return "";
    const km = (remaining / 1000).toFixed(1);
    const placeholder = new Set(["", "No destination", "Selected destination", "Current location"]);
    const candidates = [
      this.activeSession.destinationLabel ?? "",
      this.guidanceRoute?.summary.destinationLabel ?? "",
    ].map((s) => s.trim());
    const address = candidates.find((label) => !placeholder.has(label));
    return address && address.length > 0 ? `${km} km to ${address}` : `${km} km`;
  }

  /**
   * Direction-only description of the next maneuver, with no distance
   * or time, used as the lock-screen Notification body. Stable between
   * GPS ticks (only changes when the upcoming maneuver itself changes),
   * so notifications don't re-chime every few seconds while the rider
   * approaches a turn.
   */
  get nextTurnDescriptionForNotification(): string {
    const alert = this.upcomingTurnAlert;
    if (alert) {
      return alert.instructionText ?? turnAlertLabel(alert.kind);
    }
    const route = this.guidanceRoute;
    if (!route) return "On route";
    for (const m of route.maneuvers) {
      if (m.maneuverType === "depart" || m.maneuverType === "arrive") continue;
      const remaining = m.distanceFromStartMeters - this.progressDistanceM;
      if (remaining < 0) continue;
      return m.instructionText ?? "Continue";
    }
    return "Arrive";
  }

  /** iOS-parity routing top card line 3: "16 min remaining". */
  get minutesRemainingLine(): string {
    const seconds =
      this.remainingDurationSeconds > 0
        ? this.remainingDurationSeconds
        : (this.guidanceRoute?.summary.estimatedDurationSeconds ?? 0);
    if (seconds <= 0) return "";
    const minutes = Math.max(1, Math.ceil(seconds / 60));
    return `${minutes} min remaining`;
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

  /**
   * Geometry that the route-overview camera should fit when the user taps
   * the compass during routing. Equals the remaining route ahead of the
   * rider — late in a ride the start segment is no longer relevant, so
   * including it would zoom the camera out unnecessarily. Falls back to
   * the full active geometry before any progress has been recorded.
   */
  get routeOverviewGeometry(): CoordinatePoint[] | undefined {
    if (this.activeRouteGeometry.length === 0) return undefined;
    const split = this.routeSplit;
    if (split && split.remaining.length >= 2) return split.remaining;
    return this.activeRouteGeometry;
  }

  /**
   * iOS-parity top-right icon column. Same items in every mode so the
   * layout doesn't reflow when the rider transitions between planning
   * and routing. Order, top → bottom: settings, compass/north-up,
   * device chip (only when paired). The compass tap recentres the camera
   * (single tap = north-up; double-tap = lock north-up).
   *
   * The web app does not have ESP pairing yet; `deviceChip` never
   * appears here. Wire that in when `RootStore` exposes a paired flag.
   */
  get topRightIconStack(): Array<"settings" | "compass" | "deviceChip"> {
    return ["settings", "compass"];
  }

  /**
   * iOS-parity top-left icon column. Always-on zoom buttons stay at the
   * top so the alternate-routes button (visible only in routing) doesn't
   * shift their on-screen position when the rider presses Start.
   */
  get topLeftIconStack(): Array<"zoomIn" | "zoomOut" | "alternateRoutes"> {
    const icons: Array<"zoomIn" | "zoomOut" | "alternateRoutes"> = ["zoomIn", "zoomOut"];
    if (this.homeMode === "phoneGuidance") icons.push("alternateRoutes");
    return icons;
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
    // iOS parity: don't clobber a real user-typed destination on
    // activeSession with the route package's generic placeholder
    // ("Selected destination" comes baked into the OSRM mapper).
    const placeholder = new Set(["", "No destination", "Selected destination", "Current location"]);
    const sessionLabel = (this.activeSession.destinationLabel ?? "").trim();
    const packageLabel = (package_.summary.destinationLabel ?? "").trim();
    const sessionIsPlaceholder = placeholder.has(sessionLabel);
    const packageIsMeaningful = !placeholder.has(packageLabel);
    const destinationLabel =
      packageIsMeaningful && (sessionIsPlaceholder || sessionLabel.length === 0)
        ? packageLabel
        : sessionLabel || packageLabel;
    this.activeSession = {
      routeIdentifier: package_.routeIdentifier,
      routeRevision: package_.revision,
      destinationLabel,
      destinationCoordinate: package_.geometry[package_.geometry.length - 1],
      providerID: package_.provenance.providerID,
      sourceMode: this.planning.currentSourceMode,
    };
    this.persistence.saveLastSession(this.activeSession);
    this.homeMode = "phoneGuidance";
    this.compassMode = "autoFollow";
    this.isExploringAlternativesFromGuidance = false;
    this.explorationSelectedID = undefined;
    this.activeRoutePackage = package_;
    this.resetProgress(package_);
    // Spec: on Start, the camera snaps onto the rider with routing zoom and
    // bearing-up. Emit a follow-rider recenter so RootStore can react. iOS
    // gets this for free through `refreshCameraForCurrentMode()`; web
    // needs an explicit signal.
    this.emitRecenterRequested();
  }

  /**
   * Enter the "browse alternatives from guidance" state. Called by RootStore
   * once the async re-plan completes. homeMode stays "phoneGuidance" so
   * guidance keeps running; the route-suggestions card is shown via the flag.
   * No-op outside phoneGuidance so a stale async result can't flip the flag
   * if the rider stopped guidance while the re-plan was in flight.
   */
  enterAlternativesExploration(): void {
    if (this.homeMode !== "phoneGuidance") return;
    this.isExploringAlternativesFromGuidance = true;
    this.explorationSelectedID = undefined;
    this.compassMode = "northLocked";
  }

  /**
   * Cancel browsing — dismiss the alternatives card, resume normal routing UI.
   * The original route is still active; camera returns to autoFollow.
   */
  cancelAlternativesExploration(): void {
    this.isExploringAlternativesFromGuidance = false;
    this.explorationSelectedID = undefined;
    this.compassMode = "autoFollow";
  }

  /**
   * Deselect any explicitly-tapped alternative during exploration, moving
   * the checkmark back to "Continue on current route".
   */
  deselectForExploration(): void {
    if (!this.isExploringAlternativesFromGuidance) return;
    this.explorationSelectedID = undefined;
  }

  /**
   * Select an alternative for preview while exploring from guidance.
   * Updates the planning selection (map highlight) and records the explicit
   * user tap so selectedAlternativeIDForDisplay can show the checkmark.
   */
  selectAlternativeForExploration(id: string): void {
    this.explorationSelectedID = id;
    this.planning.selectAlternative(id);
  }

  /**
   * The alternative ID that should show a checkmark in the suggestions card.
   * During exploration, returns the ID explicitly tapped by the user (nil
   * until first tap). Outside exploration, returns the planning-selected ID.
   */
  get selectedAlternativeIDForDisplay(): string | undefined {
    if (this.isExploringAlternativesFromGuidance) return this.explorationSelectedID;
    return this.planning.preview.selectedAlternativeID;
  }

  /**
   * Alternative routes to render on the map during alternatives exploration.
   * Returns an empty array outside of exploration so the map shows nothing extra.
   */
  get guidanceAlternatives(): NormalizedRoutePackage[] {
    if (!this.isExploringAlternativesFromGuidance) return [];
    return this.planning.preview.alternatives.map((a) => a.normalizedPackage);
  }

  stopGuidance(): void {
    this.homeMode = "planning";
    this.compassMode = "autoFollow";
    this.isExploringAlternativesFromGuidance = false;
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

  /** Records an auto-reroute attempt at `now` and returns the required delay
   *  before the reroute should actually fire. Drives the throttle the
   *  RootStore reaction consults: 0 ms means fire immediately, > 0 ms means
   *  the caller should defer (and the UI will surface the wait via
   *  {@link isWaitingToReroute}). Old timestamps outside the sliding window
   *  age out automatically. */
  recordReroutingAttempt(now: number): number {
    this.reroutingAttemptTimestamps = this.reroutingAttemptTimestamps.filter(
      (t) => now - t < REROUTING_BACKOFF_WINDOW_MS,
    );
    this.reroutingAttemptTimestamps.push(now);
    const count = this.reroutingAttemptTimestamps.length;
    let delayMs = 0;
    if (count >= REROUTING_ESCALATE_AT_ATTEMPTS) delayMs = REROUTING_BACKOFF_LONG_DELAY_MS;
    else if (count >= REROUTING_THROTTLE_AT_ATTEMPTS) delayMs = REROUTING_BACKOFF_DELAY_MS;
    this.reroutingDelayedUntilMs = delayMs > 0 ? now + delayMs : undefined;
    return delayMs;
  }

  /** True while an auto-reroute is being held back by the throttle. The view
   *  layer passes the current wall-clock so this stays a pure read against
   *  the recorded `reroutingDelayedUntilMs` — easy to test, no clock side
   *  effects. */
  isWaitingToReroute(now: number): boolean {
    return this.reroutingDelayedUntilMs !== undefined && now < this.reroutingDelayedUntilMs;
  }

  /** Rider tapped "Reroute now" — clear the throttle delay so the next tick
   *  of the RootStore reaction fires the reroute immediately. Does not
   *  itself trigger the request; the caller observes
   *  `reroutingDelayedUntilMs === undefined` and proceeds. */
  requestManualReroute(): void {
    this.reroutingDelayedUntilMs = undefined;
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

    // Spec: when the rider arrives at the destination, end routing.
    // Use straight-line distance to the last geometry vertex so a rider
    // approaching from any side trips arrival, not just one travelling
    // along-route.
    const last = this.activeRouteGeometry[this.activeRouteGeometry.length - 1];
    if (last && approximateDistanceMeters(riderLocation, last) <= ARRIVAL_RADIUS_M) {
      this.declareArrival();
    }
  }

  private declareArrival(): void {
    this.arrivalNotice = "Arrived at destination";
    this.scheduleArrivalNoticeAutoDismiss();
    // Reuse the same teardown as a manual stop so persistence + UI camera
    // intents stay consistent. The arrival banner survives because we set it
    // before stopGuidance() (stopGuidance does not clear arrivalNotice).
    this.stopGuidance();
    // Wipe the search field and all route alternatives so the map returns
    // to a blank "Where to?" state. arrivalNotice persists until the rider
    // dismisses it, the 60s timer fires, or starts a new route.
    this.planning.clearPreview();
  }

  /** Manual dismissal from the banner's close button. */
  dismissArrivalNotice(): void {
    this.cancelArrivalNoticeTimer();
    this.arrivalNotice = undefined;
  }

  private scheduleArrivalNoticeAutoDismiss(): void {
    this.cancelArrivalNoticeTimer();
    this.arrivalNoticeTimeoutId = setTimeout(() => {
      this.arrivalNoticeTimeoutId = undefined;
      this.arrivalNotice = undefined;
    }, GuidanceStore.arrivalNoticeAutoDismissMs);
  }

  private cancelArrivalNoticeTimer(): void {
    if (this.arrivalNoticeTimeoutId !== undefined) {
      clearTimeout(this.arrivalNoticeTimeoutId);
      this.arrivalNoticeTimeoutId = undefined;
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
    this.cancelArrivalNoticeTimer();
    this.arrivalNotice = undefined;
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
