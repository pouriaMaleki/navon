import type { RootStore } from "../../app/RootStore.js";
import { selectedAlternative } from "../../domain/models.js";

/**
 * Recenter / re-fit the camera to match the current app mode.
 *
 * Before the GuidanceStore `onRecenterRequested` / `onFitRouteRequested`
 * architecture landed, this function also handled the in-routing cases —
 * calling `setCenter(rider, 16, locationStore.currentHeadingDegrees ?? 0)`.
 * That path silently overwrote the route-direction bearing set by the new
 * architecture (spec line 101), because a simulated or low-motion GPS fix
 * has no `headingDegrees` and falls through to bearing=0.
 *
 * During `phoneGuidance`, the RootStore's `onRecenterRequested` listener is
 * the single owner of the camera target. Calling this function from the
 * recenter button during routing should instead delegate to that listener
 * via `guidance.requestRecenter()`.
 */
export function refreshCameraForCurrentMode(store: RootStore): void {
  const guidance = store.guidanceStore;

  // In-routing camera is owned by GuidanceStore's emit-recenter path.
  if (guidance.homeMode === "phoneGuidance") {
    guidance.requestRecenter();
    return;
  }
  // Planning mode: fit to route or center on rider
  const selected = selectedAlternative(store.planningStore.preview);
  if (selected && selected.normalizedPackage.geometry.length > 0) {
    store.mapCameraStore.fitBounds(selected.normalizedPackage.geometry);
    return;
  }
  store.mapCameraStore.setCenter(guidance.riderLocation, 12, 0);
}
