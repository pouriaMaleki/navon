import { makeAutoObservable, runInAction } from "mobx";
import type { CoordinatePoint } from "../domain/models.js";
import type {
  LocationErrorKind,
  LocationService,
} from "../integrations/location/BrowserLocationService.js";
import { HeadingTrail } from "../integrations/location/HeadingTrail.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

export type PermissionPromptState = "granted" | "prompt" | "denied" | "unknown";

export class LocationStore {
  /** Last good fix from the device, or null if we have never received one in this session. */
  currentLocation: CoordinatePoint | null = null;
  /** GPS heading in degrees from true north (0-360), or null if unavailable/stationary. */
  currentHeadingDegrees: number | null = null;
  /**
   * Smoothed travel heading derived from the last few GPS fixes. Spec line
   * 110 (authoritative): this is what the routing camera should rotate to
   * when the rider is moving — overrides the route-segment bearing.
   * `undefined` while stationary / no usable trail. Parameters match
   * runtime-core's motion filter (min displacement 3 m, alpha 0.25).
   */
  private readonly headingTrail = new HeadingTrail({
    maxAgeMs: 5_000,
    maxFixes: 10,
    minDisplacementM: 3.0,
    smoothingAlpha: 0.25,
  });
  /**
   * MobX-observable cache of `headingTrail.travelHeadingDegrees`. Read-only
   * for outside callers (use `travelHeadingDegrees`); mutated only inside
   * `recordFix` and the live geolocation listener so reactions/autoruns
   * fire when the smoothed heading changes.
   */
  private trailHeadingDegreesCache: number | undefined = undefined;
  /** Last good fix loaded from persistence — used as a fallback the first time the app boots. */
  lastKnownLocation: CoordinatePoint | null = null;
  /** True from start() until the first fix or a terminal error arrives. */
  isLocating = false;
  lastError: LocationErrorKind | null = null;
  permission: PermissionPromptState = "unknown";
  /** Whether we have already asked the browser this session (prevents flicker). */
  promptShown = false;

  private stopFn: (() => void) | undefined;

  constructor(
    private readonly service: LocationService,
    private readonly persistence: LocalStoragePersistence,
  ) {
    this.lastKnownLocation = persistence.loadLastKnownRider();
    this.promptShown = persistence.loadLocationPromptShown();
    makeAutoObservable(this, {}, { autoBind: true });
    void this.refreshPermission();
  }

  /** Begin watching. Idempotent — calling twice is a no-op while a watcher is active. */
  start(): void {
    if (this.stopFn) return;
    if (!this.service.isSupported()) {
      this.lastError = "unsupported";
      this.isLocating = false;
      return;
    }
    this.isLocating = true;
    this.lastError = null;
    this.persistence.saveLocationPromptShown(true);
    this.promptShown = true;
    this.stopFn = this.service.start((update) => {
      if (update.kind === "fix") {
        runInAction(() => {
          this.currentLocation = update.point;
          this.currentHeadingDegrees = update.headingDegrees ?? null;
          this.headingTrail.recordFix(update.point, Date.now());
          this.trailHeadingDegreesCache = this.headingTrail.travelHeadingDegrees;
          this.lastKnownLocation = update.point;
          this.isLocating = false;
          this.lastError = null;
          this.permission = "granted";
        });
        this.persistence.saveLastKnownRider(update.point);
      } else {
        runInAction(() => {
          this.isLocating = false;
          this.lastError = update.error;
          if (update.error === "denied") this.permission = "denied";
        });
      }
    });
  }

  stop(): void {
    if (this.stopFn) {
      this.stopFn();
      this.stopFn = undefined;
    }
    this.isLocating = false;
  }

  /** Best-known location: fresh fix > last persisted fix > undefined (caller falls back). */
  bestKnownLocation(): CoordinatePoint | null {
    return this.currentLocation ?? this.lastKnownLocation;
  }

  /**
   * Smoothed travel heading (0-360, clockwise from north) from the last few
   * GPS fixes, or `undefined` when stationary / buffer not primed. Spec 110.
   * Reads from an observable cache that's refreshed inside `recordFix` and
   * the live geolocation listener so MobX reactions can depend on it.
   */
  get travelHeadingDegrees(): number | undefined {
    return this.trailHeadingDegreesCache;
  }

  /**
   * Inject a fix into the trail. Used by the live geolocation listener
   * (replay) and by tests/headless drivers (e.g. simulated rides). Updates
   * the same observable fields a real fix would so observers stay coherent.
   */
  recordFix(point: CoordinatePoint, timestampMs: number): void {
    runInAction(() => {
      this.currentLocation = point;
      this.lastKnownLocation = point;
      this.headingTrail.recordFix(point, timestampMs);
      this.trailHeadingDegreesCache = this.headingTrail.travelHeadingDegrees;
    });
  }

  /** True the first time we are waiting for any usable fix (current or persisted). */
  get isWaitingForFirstFix(): boolean {
    return this.isLocating && this.bestKnownLocation() === null;
  }

  private async refreshPermission(): Promise<void> {
    const state = await this.service.permissionState();
    runInAction(() => {
      this.permission = state;
    });
  }
}
