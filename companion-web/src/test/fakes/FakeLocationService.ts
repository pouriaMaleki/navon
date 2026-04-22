import type {
  LocationErrorKind,
  LocationListener,
  LocationService,
  LocationUpdate,
} from "../../integrations/location/BrowserLocationService.js";

/**
 * Test double for `LocationService`. Tests push updates via `emitFix` or
 * `emitError`; the store sees them exactly as it would from the real browser
 * Geolocation API.
 */
export class FakeLocationService implements LocationService {
  private listener?: LocationListener;
  private stopped = false;
  private currentPermission: "granted" | "prompt" | "denied" | "unknown" = "prompt";
  supported = true;

  isSupported(): boolean {
    return this.supported;
  }

  async permissionState(): Promise<"granted" | "prompt" | "denied" | "unknown"> {
    return this.currentPermission;
  }

  start(listener: LocationListener): () => void {
    this.listener = listener;
    this.stopped = false;
    return () => {
      this.stopped = true;
      this.listener = undefined;
    };
  }

  setPermission(state: "granted" | "prompt" | "denied" | "unknown"): void {
    this.currentPermission = state;
  }

  get isActive(): boolean {
    return !!this.listener && !this.stopped;
  }

  emitFix(
    latitude: number,
    longitude: number,
    options: { accuracyMeters?: number; headingDegrees?: number } = {},
  ): void {
    this.emit({
      kind: "fix",
      point: { latitude, longitude },
      accuracyMeters: options.accuracyMeters,
      headingDegrees: options.headingDegrees,
    });
  }

  emitError(error: LocationErrorKind, message = "fake error"): void {
    this.emit({ kind: "error", error, message });
  }

  private emit(update: LocationUpdate): void {
    if (!this.listener || this.stopped) return;
    this.listener(update);
  }
}
