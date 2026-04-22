import { makeAutoObservable } from "mobx";
import type { CoordinatePoint } from "../domain/models.js";

export type CameraTarget =
  | { kind: "fitBounds"; coordinates: CoordinatePoint[]; padding?: number }
  | { kind: "center"; center: CoordinatePoint; zoom: number; bearing: number };

export class MapCameraStore {
  /** Monotonic counter — increments every time the desired camera changes. */
  revision = 0;
  target: CameraTarget = {
    kind: "center",
    center: { latitude: 60.1699, longitude: 24.9384 },
    zoom: 12,
    bearing: 0,
  };
  /** True when the user has panned/zoomed away from the last programmatic target. */
  needsRecenter = false;
  /**
   * Normalized Y coordinate (0 = top, 1 = bottom) where the rider should be
   * anchored on screen. Default 0.5 keeps rider centered when stationary
   * (spec lines 40, 50). Guidance switches to 0.72 to satisfy spec line 84:
   * "user location is on the bottom quarter of the screen" during routing.
   */
  riderAnchorNormalizedY = 0.5;

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  setCenter(center: CoordinatePoint, zoom: number, bearing: number): void {
    this.target = { kind: "center", center, zoom, bearing };
    this.revision += 1;
    this.needsRecenter = false;
  }

  fitBounds(coordinates: CoordinatePoint[], padding = 80): void {
    if (coordinates.length === 0) return;
    this.target = { kind: "fitBounds", coordinates, padding };
    this.revision += 1;
    this.needsRecenter = false;
  }

  markUserMovedAway(): void {
    if (!this.needsRecenter) this.needsRecenter = true;
  }

  setRiderAnchorNormalizedY(y: number): void {
    const clamped = Math.max(0, Math.min(1, y));
    if (Math.abs(clamped - this.riderAnchorNormalizedY) < 1e-6) return;
    this.riderAnchorNormalizedY = clamped;
    this.revision += 1;
  }
}
