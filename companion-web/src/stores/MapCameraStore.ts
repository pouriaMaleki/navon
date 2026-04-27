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
   * Note: this is anchored against the *visible* map area (the screen minus
   * `bottomReservedPx`), not the raw viewport — so the rider stays visible
   * above whatever bottom UI overlay is showing.
   */
  riderAnchorNormalizedY = 0.5;
  /**
   * Pixels at the bottom of the viewport that are reserved for an opaque UI
   * overlay (the routing card during phoneGuidance, the route-suggestions
   * card during planning, etc.). MapSurface measures the bottom overlay
   * with a ResizeObserver and writes the height here. The dispatcher uses
   * it as MapLibre `padding.bottom` so `fitBounds` and the follow-rider
   * camera both render in the unblocked area only.
   */
  bottomReservedPx = 0;
  /**
   * Increments each time the user taps an on-map zoom button. The sign of
   * the most recent delta lives on `lastZoomDelta` so MapSurface can apply
   * it directly to the live MapLibre instance — going through the store
   * keeps both the planning case (fitBounds target, no zoom field to
   * mutate) and the riding case (center target, persisted override) on
   * the same code path. Spec lines 10/11 ("zoom + and - buttons under the
   * top bar... when pressed it should preserve that zoom" for riding
   * mode; overview "only keep it for moment").
   */
  zoomTick = 0;
  lastZoomDelta = 0;

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

  requestZoomDelta(delta: number): void {
    this.lastZoomDelta = delta;
    this.zoomTick += 1;
  }

  setBottomReservedPx(px: number): void {
    const next = Math.max(0, px);
    if (Math.abs(next - this.bottomReservedPx) < 1) return;
    this.bottomReservedPx = next;
    this.revision += 1;
  }
}
