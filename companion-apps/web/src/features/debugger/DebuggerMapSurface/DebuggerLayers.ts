import maplibregl, { type Map as MaplibreMap } from "maplibre-gl";
import type { RootStore } from "../../../app/RootStore.js";
import type { CoordinatePoint } from "../../../domain/models.js";
import {
  buildAnnotationPinFeatures,
  buildCueMarkerFeatures,
  buildDebugRouteFeature,
  buildGpsTrailFeatures,
  buildOffRouteSegmentFeatures,
  buildRiderFeature,
} from "../debuggerMapFeatures.js";

export const ROUTE_SRC = "debug-route";
export const ROUTE_LAYER = "debug-route-layer";
export const GPS_SRC = "debug-gps";
export const GPS_LAYER = "debug-gps-layer";
export const CUE_SRC = "debug-cues";
export const CUE_LAYER = "debug-cues-layer";
export const OFFROUTE_SRC = "debug-offroute";
export const OFFROUTE_LAYER = "debug-offroute-layer";
export const RIDER_SRC = "debug-rider";
export const RIDER_LAYER = "debug-rider-layer";
export const ANNOTATION_SRC = "debug-annotations";
export const ANNOTATION_LAYER = "debug-annotations-layer";

type PopupOpenFn = (popup: { content: string; lngLat: { lat: number; lng: number } } | null) => void;

export function addDebugLayers(map: MaplibreMap, onPopupOpen?: PopupOpenFn): void {
  // Route line
  map.addSource(ROUTE_SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: ROUTE_LAYER,
    type: "line",
    source: ROUTE_SRC,
    layout: { "line-cap": "round", "line-join": "round" },
    paint: { "line-color": "#D7FF3F", "line-width": 4, "line-opacity": 0.8 },
  });

  // GPS trail dots
  map.addSource(GPS_SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: GPS_LAYER,
    type: "circle",
    source: GPS_SRC,
    paint: {
      "circle-radius": 4,
      "circle-color": [
        "interpolate",
        ["linear"],
        ["get", "timeFraction"],
        0,
        "#1e90ff",
        0.5,
        "#ffa500",
        1,
        "#ff4040",
      ],
      "circle-opacity": 0.7,
    },
  });

  // Cue markers
  map.addSource(CUE_SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: CUE_LAYER,
    type: "circle",
    source: CUE_SRC,
    paint: {
      "circle-radius": 8,
      "circle-color": "#ff8c00",
      "circle-stroke-color": "#050B12",
      "circle-stroke-width": 2,
    },
  });

  // Off-route markers
  map.addSource(OFFROUTE_SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: OFFROUTE_LAYER,
    type: "circle",
    source: OFFROUTE_SRC,
    paint: {
      "circle-radius": 10,
      "circle-color": "#ff4040",
      "circle-stroke-color": "#050B12",
      "circle-stroke-width": 2,
      "circle-opacity": 0.8,
    },
  });

  // Rider
  map.addSource(RIDER_SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: RIDER_LAYER,
    type: "circle",
    source: RIDER_SRC,
    paint: {
      "circle-radius": 8,
      "circle-color": "#D7FF3F",
      "circle-stroke-color": "#0c1320",
      "circle-stroke-width": 3,
    },
  });

  // Annotation pins
  map.addSource(ANNOTATION_SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features: [] },
  });
  map.addLayer({
    id: ANNOTATION_LAYER,
    type: "circle",
    source: ANNOTATION_SRC,
    paint: {
      "circle-radius": 10,
      "circle-color": [
        "match",
        ["get", "severity"],
        "bug",
        "#ff4040",
        "improvement",
        "#ffa500",
        "#12A3A3",
      ],
      "circle-stroke-color": "#050B12",
      "circle-stroke-width": 2,
    },
  });

  // Popup on cue marker click
  map.on("click", CUE_LAYER, (e) => {
    const features = e.features ?? [];
    if (features.length === 0) return;
    const props = features[0].properties ?? {};
    onPopupOpen?.({
      content: `<strong>${props.messageText ?? ""}</strong>&nbsp;&nbsp;<code>${props.cueType ?? ""}</code>`,
      lngLat: { lat: e.lngLat.lat, lng: e.lngLat.lng },
    });
  });

  map.on("click", OFFROUTE_LAYER, (e) => {
    const features = e.features ?? [];
    if (features.length === 0) return;
    const props = features[0].properties ?? {};
    onPopupOpen?.({
      content: `Off route: <strong>${props.distanceM ?? "?"}m</strong>`,
      lngLat: { lat: e.lngLat.lat, lng: e.lngLat.lng },
    });
  });

  // Hover cursors
  map.on("mouseenter", CUE_LAYER, () => {
    map.getCanvas().style.cursor = "pointer";
  });
  map.on("mouseleave", CUE_LAYER, () => {
    map.getCanvas().style.cursor = "";
  });
  map.on("mouseenter", OFFROUTE_LAYER, () => {
    map.getCanvas().style.cursor = "pointer";
  });
  map.on("mouseleave", OFFROUTE_LAYER, () => {
    map.getCanvas().style.cursor = "";
  });
}

export function handleMapClick(store: RootStore, map: MaplibreMap, e: maplibregl.MapMouseEvent): void {
  const dStore = store.debuggerStore;
  if (!dStore.session) return;

  const cueFeatures = map.queryRenderedFeatures(e.point, { layers: [CUE_LAYER] });
  const offRouteFeatures = map.queryRenderedFeatures(e.point, { layers: [OFFROUTE_LAYER] });

  let eventId: string | undefined;
  if (cueFeatures.length > 0) {
    eventId = cueFeatures[0].properties?.eventId as string | undefined;
  } else if (offRouteFeatures.length > 0) {
    eventId = offRouteFeatures[0].properties?.eventId as string | undefined;
  }

  const coordinate: CoordinatePoint = { latitude: e.lngLat.lat, longitude: e.lngLat.lng };
  const eventIds = eventId ? [eventId] : [];
  dStore.openAnnotationForm(dStore.currentTimeMs, coordinate, eventIds);
}

export function pushAllData(map: MaplibreMap, store: RootStore): void {
  pushRoute(map, store);
  pushGpsTrail(map, store);
  pushCueMarkers(map, store);
  pushOffRouteMarkers(map, store);
  pushRider(map, store);
  pushAnnotations(map, store);
}

export function pushRoute(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(ROUTE_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const feature = buildDebugRouteFeature(store.debuggerStore.session?.gpxGeometry);
  source.setData({ type: "FeatureCollection", features: feature ? [feature] : [] });
}

export function pushGpsTrail(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(GPS_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const session = store.debuggerStore.session;
  if (!session) return;
  const features = buildGpsTrailFeatures(session.diagSession.events, store.debuggerStore.currentTimeMs);
  source.setData({ type: "FeatureCollection", features });
}

export function pushCueMarkers(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(CUE_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const session = store.debuggerStore.session;
  if (!session) return;
  const features = buildCueMarkerFeatures(session.diagSession.events, store.debuggerStore.currentTimeMs);
  source.setData({ type: "FeatureCollection", features });
}

export function pushOffRouteMarkers(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(OFFROUTE_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const session = store.debuggerStore.session;
  if (!session) return;
  const features = buildOffRouteSegmentFeatures(session.diagSession.events, store.debuggerStore.currentTimeMs);
  source.setData({ type: "FeatureCollection", features });
}

export function pushRider(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(RIDER_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const features = buildRiderFeature(store.debuggerStore.currentPosition);
  source.setData({ type: "FeatureCollection", features });
}

export function pushAnnotations(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(ANNOTATION_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const features = buildAnnotationPinFeatures(store.debuggerStore.annotations);
  source.setData({ type: "FeatureCollection", features });
}

export function fitMapToData(map: MaplibreMap, store: RootStore): void {
  const session = store.debuggerStore.session;
  if (!session) return;
  const gpsEvents = session.diagSession.events.filter((e) => e.data.kind === "locationUpdate");
  if (gpsEvents.length < 2) return;
  const bounds = new maplibregl.LngLatBounds();
  for (const e of gpsEvents) {
    if (e.data.kind === "locationUpdate") {
      bounds.extend([e.data.lon, e.data.lat]);
    }
  }
  if (!bounds.isEmpty()) {
    map.fitBounds(bounds, { padding: 60, duration: 500 });
  }
}
