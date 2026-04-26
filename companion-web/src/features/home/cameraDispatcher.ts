import type { LngLatBoundsLike, Map as MaplibreMap } from "maplibre-gl";
import type { RootStore } from "../../app/RootStore.js";

/**
 * Dispatch the current `MapCameraStore.target` to a MapLibre `Map` instance.
 * Extracted from the MapSurface component so the decision path is testable in
 * isolation.
 *
 * `mapReady` must be `true` once the map's initial `load` event has fired.
 * We deliberately DO NOT guard on `map.isStyleLoaded()` here: that function
 * transiently returns `false` whenever a `GeoJSONSource.setData()` call is
 * in flight (MapLibre treats the style as "not loaded" while sources settle).
 * The camera reaction and the route-data reaction fire back-to-back on
 * `startSelectedRoute` (because `homeMode` is part of `routeSignature`),
 * and the prior implementation — which called `map.isStyleLoaded()` — would
 * silently no-op camera updates whenever a source setData happened in the
 * same tick. That manifested as "Start doesn't move the camera" on web.
 */
export function dispatchCameraTarget(map: MaplibreMap, store: RootStore, mapReady: boolean): void {
  if (!mapReady) return;
  const target = store.mapCameraStore.target;
  const anchorY = store.mapCameraStore.riderAnchorNormalizedY;
  const bottomInset = store.mapCameraStore.bottomReservedPx;
  const canvas = map.getCanvas?.();
  const viewportHeight = canvas?.clientHeight ?? 0;
  // The bottom UI overlay (routing card during guidance, alternatives card
  // during planning) covers the lowest `bottomInset` pixels. Treat the
  // *visible* map area as `viewportHeight - bottomInset` and apply MapLibre
  // padding so neither follow-rider nor fitBounds lands content under that
  // overlay. Rider anchor is interpreted relative to the visible area —
  // spec line 84 "bottom quarter" means bottom quarter of what the user can
  // actually see.
  if (target.kind === "center") {
    const visibleH = Math.max(0, viewportHeight - bottomInset);
    const targetY = anchorY * visibleH; // rider's y in screen pixels
    // MapLibre centers at y = (top + h - bottom) / 2; solve for top.
    const topPadding = Math.max(0, 2 * targetY - viewportHeight + bottomInset);
    map.setPadding?.({ top: topPadding, right: 0, bottom: bottomInset, left: 0 });
    map.easeTo({
      center: [target.center.longitude, target.center.latitude],
      zoom: target.zoom,
      bearing: target.bearing,
      duration: 350,
    });
    return;
  }
  // fitBounds: only reserve bottom for the overlay; let the bounds fill the
  // unblocked area without an artificial top push.
  map.setPadding?.({ top: 0, right: 0, bottom: bottomInset, left: 0 });
  if (target.coordinates.length === 0) return;
  let minLat = target.coordinates[0].latitude;
  let maxLat = minLat;
  let minLon = target.coordinates[0].longitude;
  let maxLon = minLon;
  for (const c of target.coordinates) {
    if (c.latitude < minLat) minLat = c.latitude;
    if (c.latitude > maxLat) maxLat = c.latitude;
    if (c.longitude < minLon) minLon = c.longitude;
    if (c.longitude > maxLon) maxLon = c.longitude;
  }
  const bounds: LngLatBoundsLike = [
    [minLon, minLat],
    [maxLon, maxLat],
  ];
  map.fitBounds(bounds, { padding: target.padding ?? 80, duration: 400, maxZoom: 16 });
}
