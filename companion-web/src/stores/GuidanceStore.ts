import { makeAutoObservable } from "mobx";
import {
  type ActiveRouteSession,
  type CoordinatePoint,
  EMPTY_ACTIVE_SESSION,
  type HomeCompassMode,
  type HomeMode,
  type NormalizedRoutePackage,
  type RouteSourceMode,
  selectedAlternative,
} from "../domain/models.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import type { LocationStore } from "./LocationStore.js";
import type { PlanningStore } from "./PlanningStore.js";

export const DEFAULT_RIDER_FALLBACK: CoordinatePoint = {
  latitude: 60.1699,
  longitude: 24.9384,
};

export class GuidanceStore {
  homeMode: HomeMode = "planning";
  compassMode: HomeCompassMode = "autoFollow";
  activeSession: ActiveRouteSession = EMPTY_ACTIVE_SESSION;

  private northPreviewTimeoutId?: ReturnType<typeof setTimeout>;

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
    const route = this.guidanceRoute;
    if (!route) return undefined;
    const nextStep = route.maneuvers.find((m) => m.maneuverType !== "depart");
    if (!nextStep) return undefined;
    const distanceMeters = nextStep.distanceFromStartMeters;
    const distanceLabel =
      distanceMeters >= 1000
        ? `${(distanceMeters / 1000).toFixed(1)} km`
        : `${Math.round(distanceMeters)} m`;
    return `${nextStep.instructionText ?? "Continue"} in ${distanceLabel}`;
  }

  get activeNavigationTitle(): string {
    const route = this.guidanceRoute;
    return route?.summary.destinationLabel ?? this.activeSession.destinationLabel;
  }

  get activeNavigationSubtitle(): string {
    const route = this.guidanceRoute;
    if (!route) return "Phone guidance ready";
    const minutes = Math.max(Math.floor(route.summary.estimatedDurationSeconds / 60), 1);
    const km = (route.summary.totalDistanceMeters / 1000).toFixed(1);
    return `${km} km • ${minutes} min`;
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
    this.cancelNorthPreviewTimer();
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
      // direct mutation is safe because this is observable
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
