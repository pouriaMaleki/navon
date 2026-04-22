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
  const canvas = map.getCanvas?.();
  const viewportHeight = canvas?.clientHeight ?? 0;
  // MapLibre centers on y = (padding.top + height - padding.bottom) / 2.
  // To anchor the rider at fraction `anchorY` of the viewport (0 = top,
  // 1 = bottom), we want that y = anchorY * height, which solves to
  //   padding.top - padding.bottom = 2 * (anchorY - 0.5) * height.
  // For anchorY > 0.5 (rider in bottom half, e.g. routing's 0.72 = bottom
  // quarter, spec line 84), that means a positive padding.TOP — which pushes
  // the center DOWN on screen. The previous implementation set
  // padding.bottom instead, which pushed the rider UP into the top half.
  const topPadding = Math.max(0, (anchorY - 0.5) * 2 * viewportHeight);
  map.setPadding?.({ top: topPadding, right: 0, bottom: 0, left: 0 });
  if (target.kind === "center") {
    map.easeTo({
      center: [target.center.longitude, target.center.latitude],
      zoom: target.zoom,
      bearing: target.bearing,
      duration: 350,
    });
    return;
  }
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
