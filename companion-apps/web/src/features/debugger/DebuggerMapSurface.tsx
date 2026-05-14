import maplibregl, { type Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import type { CoordinatePoint } from "../../domain/models.js";
import {
  buildAnnotationPinFeatures,
  buildCueMarkerFeatures,
  buildDebugRouteFeature,
  buildGpsTrailFeatures,
  buildOffRouteSegmentFeatures,
  buildRiderFeature,
} from "./debuggerMapFeatures.js";

const OSM_STYLE: maplibregl.StyleSpecification = {
  version: 8,
  sources: {
    "osm-raster": {
      type: "raster",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      tileSize: 256,
      attribution: "© OpenStreetMap contributors",
      maxzoom: 19,
    },
  },
  layers: [
    {
      id: "osm-raster",
      type: "raster",
      source: "osm-raster",
      minzoom: 0,
      maxzoom: 22,
    },
  ],
};

const ROUTE_SRC = "debug-route";
const ROUTE_LAYER = "debug-route-layer";
const GPS_SRC = "debug-gps";
const GPS_LAYER = "debug-gps-layer";
const CUE_SRC = "debug-cues";
const CUE_LAYER = "debug-cues-layer";
const OFFROUTE_SRC = "debug-offroute";
const OFFROUTE_LAYER = "debug-offroute-layer";
const RIDER_SRC = "debug-rider";
const RIDER_LAYER = "debug-rider-layer";
const ANNOTATION_SRC = "debug-annotations";
const ANNOTATION_LAYER = "debug-annotations-layer";

type Props = { store: RootStore; onPopupOpen?: (popup: { content: string; lngLat: { lat: number; lng: number } } | null) => void };

export const DebuggerMapSurface = observer(({ store: _store, onPopupOpen }: Props) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MaplibreMap | null>(null);
  const readyRef = useRef(false);

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;
    const map = new maplibregl.Map({
      container: containerRef.current,
      style: OSM_STYLE,
      center: [24.9384, 60.1699],
      zoom: 13,
      attributionControl: { compact: true },
    });
    mapRef.current = map;
    map.on("load", () => {
      addDebugLayers(map, onPopupOpen);
      readyRef.current = true;
      pushAllData(map, _store);
    });

    map.on("click", (e) => {
      handleMapClick(_store, map, e);
    });

    return () => {
      map.remove();
      mapRef.current = null;
    };
  }, [_store]);

  // React to session changes
  useEffect(() => {
    return reaction(
      () => _store.debuggerStore.session?.diagSession.id,
      () => {
        const map = mapRef.current;
        if (!map || !readyRef.current) return;
        pushAllData(map, _store);
        fitMapToData(map, _store);
      },
    );
  }, [_store]);

  // React to currentTimeMs changes during playback/scrubbing
  useEffect(() => {
    return reaction(
      () => _store.debuggerStore.currentTimeMs,
      () => {
        const map = mapRef.current;
        if (!map || !readyRef.current) return;
        pushGpsTrail(map, _store);
        pushCueMarkers(map, _store);
        pushOffRouteMarkers(map, _store);
        pushRider(map, _store);
      },
    );
  }, [_store]);

  // React to annotation changes
  useEffect(() => {
    return reaction(
      () => _store.debuggerStore.annotations.length,
      () => {
        const map = mapRef.current;
        if (!map || !readyRef.current) return;
        pushAnnotations(map, _store);
      },
    );
  }, [_store]);

  // React to GPX geometry changes
  useEffect(() => {
    return reaction(
      () => _store.debuggerStore.session?.gpxGeometry,
      () => {
        const map = mapRef.current;
        if (!map || !readyRef.current) return;
        pushRoute(map, _store);
      },
    );
  }, [_store]);

  // React to map-follow toggle — center map on active GPS position, preserving zoom
  useEffect(() => {
    return reaction(
      () => _store.debuggerStore.mapFollowActive ? _store.debuggerStore.currentPosition : null,
      (position) => {
        const map = mapRef.current;
        if (!map || !readyRef.current || !position) return;
        map.easeTo({
          center: [position.longitude, position.latitude],
          duration: 300,
        });
      },
    );
  }, [_store]);

  return (
    <>
      <div ref={containerRef} className="debugger-map-surface" />
      <button
        type="button"
        className={`debugger-map-surface__follow-btn${_store.debuggerStore.mapFollowActive ? " debugger-map-surface__follow-btn--active" : ""}`}
        onClick={() => _store.debuggerStore.setMapFollowActive(!_store.debuggerStore.mapFollowActive)}
        title={_store.debuggerStore.mapFollowActive ? "Stop following GPS position" : "Follow GPS position"}
      >
        <LocateIcon />
      </button>
    </>
  );
});

function addDebugLayers(map: MaplibreMap, onPopupOpen?: Props["onPopupOpen"]): void {
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
        0, "#1e90ff",
        0.5, "#ffa500",
        1, "#ff4040",
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
        "bug", "#ff4040",
        "improvement", "#ffa500",
        "#12A3A3",
      ],
      "circle-stroke-color": "#050B12",
      "circle-stroke-width": 2,
    },
  });

  // Popup on cue marker click — use callback so popup renders as a fixed top bar
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
  map.on("mouseenter", CUE_LAYER, () => { map.getCanvas().style.cursor = "pointer"; });
  map.on("mouseleave", CUE_LAYER, () => { map.getCanvas().style.cursor = ""; });
  map.on("mouseenter", OFFROUTE_LAYER, () => { map.getCanvas().style.cursor = "pointer"; });
  map.on("mouseleave", OFFROUTE_LAYER, () => { map.getCanvas().style.cursor = ""; });
}

function handleMapClick(
  store: RootStore,
  map: MaplibreMap,
  e: maplibregl.MapMouseEvent,
): void {
  const dStore = store.debuggerStore;
  if (!dStore.session) return;

  // Check if clicking near an event feature
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

function LocateIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="10" cy="10" r="3" />
      <path d="M10 1v3M10 16v3M1 10h3M16 10h3" />
    </svg>
  );
}

function pushAllData(map: MaplibreMap, store: RootStore): void {
  pushRoute(map, store);
  pushGpsTrail(map, store);
  pushCueMarkers(map, store);
  pushOffRouteMarkers(map, store);
  pushRider(map, store);
  pushAnnotations(map, store);
}

function pushRoute(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(ROUTE_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const feature = buildDebugRouteFeature(store.debuggerStore.session?.gpxGeometry);
  source.setData({
    type: "FeatureCollection",
    features: feature ? [feature] : [],
  });
}

function pushGpsTrail(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(GPS_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const session = store.debuggerStore.session;
  if (!session) return;
  const features = buildGpsTrailFeatures(session.diagSession.events, store.debuggerStore.currentTimeMs);
  source.setData({ type: "FeatureCollection", features });
}

function pushCueMarkers(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(CUE_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const session = store.debuggerStore.session;
  if (!session) return;
  const features = buildCueMarkerFeatures(session.diagSession.events, store.debuggerStore.currentTimeMs);
  source.setData({ type: "FeatureCollection", features });
}

function pushOffRouteMarkers(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(OFFROUTE_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const session = store.debuggerStore.session;
  if (!session) return;
  const features = buildOffRouteSegmentFeatures(session.diagSession.events, store.debuggerStore.currentTimeMs);
  source.setData({ type: "FeatureCollection", features });
}

function pushRider(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(RIDER_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const features = buildRiderFeature(store.debuggerStore.currentPosition);
  source.setData({ type: "FeatureCollection", features });
}

function pushAnnotations(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(ANNOTATION_SRC) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const features = buildAnnotationPinFeatures(store.debuggerStore.annotations);
  source.setData({ type: "FeatureCollection", features });
}

function fitMapToData(map: MaplibreMap, store: RootStore): void {
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
