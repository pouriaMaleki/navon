import type { CoordinatePoint } from "../../domain/models.js";

export type LocationErrorKind = "denied" | "unavailable" | "timeout" | "unsupported";

export type LocationUpdate =
  | { kind: "fix"; point: CoordinatePoint; accuracyMeters?: number }
  | { kind: "error"; error: LocationErrorKind; message: string };

export type LocationListener = (update: LocationUpdate) => void;

export interface LocationService {
  isSupported(): boolean;
  /** Returns 'granted' | 'prompt' | 'denied' | 'unknown'. Best-effort, may not exist on all browsers. */
  permissionState(): Promise<"granted" | "prompt" | "denied" | "unknown">;
  /** Begin watching. Calls listener once per fix or terminal error. Returns a stop function. */
  start(listener: LocationListener): () => void;
}

export class BrowserLocationService implements LocationService {
  isSupported(): boolean {
    return typeof navigator !== "undefined" && !!navigator.geolocation;
  }

  async permissionState(): Promise<"granted" | "prompt" | "denied" | "unknown"> {
    if (!this.isSupported()) return "unknown";
    if (typeof navigator.permissions?.query !== "function") return "unknown";
    try {
      const status = await navigator.permissions.query({
        name: "geolocation" as PermissionName,
      });
      return status.state as "granted" | "prompt" | "denied";
    } catch {
      return "unknown";
    }
  }

  start(listener: LocationListener): () => void {
    if (!this.isSupported()) {
      listener({ kind: "error", error: "unsupported", message: "Geolocation API is unavailable." });
      return () => {};
    }
    const id = navigator.geolocation.watchPosition(
      (position) => {
        listener({
          kind: "fix",
          point: {
            latitude: position.coords.latitude,
            longitude: position.coords.longitude,
          },
          accuracyMeters: position.coords.accuracy,
        });
      },
      (error) => {
        listener({
          kind: "error",
          error: mapErrorCode(error.code),
          message: error.message || "Unknown geolocation error",
        });
      },
      { enableHighAccuracy: true, maximumAge: 5000, timeout: 15000 },
    );
    return () => {
      navigator.geolocation.clearWatch(id);
    };
  }
}

function mapErrorCode(code: number): LocationErrorKind {
  if (code === 1) return "denied";
  if (code === 2) return "unavailable";
  if (code === 3) return "timeout";
  return "unavailable";
}
