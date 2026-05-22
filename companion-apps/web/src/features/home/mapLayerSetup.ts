import type { Map as MaplibreMap } from "maplibre-gl";
import type { GeoJSONSource } from "maplibre-gl";
import type { RootStore } from "../../app/RootStore.js";
import { type CoordinatePoint, selectedAlternative } from "../../domain/models.js";
import { buildRouteFeatures } from "./mapRouteFeatures.js";

export const ROUTE_SOURCE_ID = "companion-routes";
export const SELECTED_ROUTE_LAYER_ID = "companion-route-selected";
export const ALTERNATE_ROUTE_LAYER_ID = "companion-route-alternates";
export const COMPLETED_ROUTE_LAYER_ID = "companion-route-completed";
export const MARKERS_SOURCE_ID = "companion-markers";
export const MARKERS_LAYER_ID = "companion-markers-layer";
export const RIDER_SOURCE_ID = "companion-rider";
export const RIDER_LAYER_ID = "companion-rider-layer";

export function addCompanionLayers(map: MaplibreMap): void {
  map.addSource(ROUTE_SOURCE_ID, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: ALTERNATE_ROUTE_LAYER_ID,
    type: "line",
    source: ROUTE_SOURCE_ID,
    filter: ["!=", ["get", "selected"], true],
    layout: { "line-cap": "round", "line-join": "round" },
    paint: { "line-color": "#12A3A3", "line-width": 4, "line-opacity": 0.6 },
  });
  map.addLayer({
    id: SELECTED_ROUTE_LAYER_ID,
    type: "line",
    source: ROUTE_SOURCE_ID,
    filter: ["==", ["get", "selected"], true],
    layout: { "line-cap": "round", "line-join": "round" },
    paint: { "line-color": "#D7FF3F", "line-width": 6 },
  });
  map.addLayer({
    id: COMPLETED_ROUTE_LAYER_ID,
    type: "line",
    source: ROUTE_SOURCE_ID,
    filter: ["==", ["get", "completed"], true],
    layout: { "line-cap": "round", "line-join": "round" },
    paint: { "line-color": "#12A3A3", "line-width": 5, "line-opacity": 0.4 },
  });

  map.addSource(MARKERS_SOURCE_ID, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: MARKERS_LAYER_ID,
    type: "circle",
    source: MARKERS_SOURCE_ID,
    paint: {
      "circle-radius": 7,
      "circle-color": [
        "match",
        ["get", "kind"],
        "start",
        "#12A3A3",
        "destination",
        "#D7FF3F",
        "#ffffff",
      ],
      "circle-stroke-color": "#050B12",
      "circle-stroke-width": 2,
    },
  });

  map.addSource(RIDER_SOURCE_ID, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: RIDER_LAYER_ID,
    type: "circle",
    source: RIDER_SOURCE_ID,
    paint: {
      "circle-radius": 8,
      "circle-color": "#D7FF3F",
      "circle-stroke-color": "#0c1320",
      "circle-stroke-width": 3,
    },
  });
}

export function pushRouteData(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(ROUTE_SOURCE_ID) as GeoJSONSource | undefined;
  if (!source) return;
  const features = buildRouteFeatures(store);
  source.setData({ type: "FeatureCollection", features });
}

export function pushMarkers(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(MARKERS_SOURCE_ID) as GeoJSONSource | undefined;
  if (!source) return;
  const selected = selectedAlternative(store.planningStore.preview);
  const markers: GeoJSON.Feature[] = [];
  if (selected) {
    const start = selected.normalizedPackage.geometry[0];
    const end = selected.normalizedPackage.geometry[selected.normalizedPackage.geometry.length - 1];
    if (start) markers.push(pointFeature(start, "start"));
    if (end) markers.push(pointFeature(end, "destination"));
  }
  source.setData({ type: "FeatureCollection", features: markers });
}

export function pushRider(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(RIDER_SOURCE_ID) as GeoJSONSource | undefined;
  if (!source) return;
  const rider = store.locationStore.currentLocation;
  source.setData({
    type: "FeatureCollection",
    features: rider ? [pointFeature(rider, "rider")] : [],
  });
}

function pointFeature(point: CoordinatePoint, kind: string): GeoJSON.Feature {
  return {
    type: "Feature",
    properties: { kind },
    geometry: { type: "Point", coordinates: [point.longitude, point.latitude] },
  };
}
