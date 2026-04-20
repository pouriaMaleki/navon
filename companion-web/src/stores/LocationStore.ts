import { makeAutoObservable, runInAction } from "mobx";
import type { CoordinatePoint } from "../domain/models.js";
import type {
  LocationErrorKind,
  LocationService,
} from "../integrations/location/BrowserLocationService.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

export type PermissionPromptState = "granted" | "prompt" | "denied" | "unknown";

export class LocationStore {
  /** Last good fix from the device, or null if we have never received one in this session. */
  currentLocation: CoordinatePoint | null = null;
  /** GPS heading in degrees from true north (0-360), or null if unavailable/stationary. */
  currentHeadingDegrees: number | null = null;
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
