import maplibregl, { type LngLatBoundsLike, type Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import {
  type CoordinatePoint,
  type RouteAlternative,
  selectedAlternative,
} from "../../domain/models.js";

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

const ROUTE_SOURCE_ID = "companion-routes";
const SELECTED_ROUTE_LAYER_ID = "companion-route-selected";
const ALTERNATE_ROUTE_LAYER_ID = "companion-route-alternates";
const MARKERS_SOURCE_ID = "companion-markers";
const MARKERS_LAYER_ID = "companion-markers-layer";
const RIDER_SOURCE_ID = "companion-rider";
const RIDER_LAYER_ID = "companion-rider-layer";

type Props = { store: RootStore };

export const MapSurface = observer(({ store }: Props) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MaplibreMap | null>(null);
  const lastRouteSignatureRef = useRef<string>("");
  const longPressTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const longPressStartRef = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;
    const map = new maplibregl.Map({
      container: containerRef.current,
      style: OSM_STYLE,
      center: [24.9384, 60.1699],
      zoom: 12,
      attributionControl: { compact: true },
    });
    mapRef.current = map;
    map.on("load", () => {
      addCompanionLayers(map);
      pushCameraTarget(map, store);
      pushRouteData(map, store);
      pushMarkers(map, store);
      pushRider(map, store);
    });
    map.on("dragstart", () => store.mapCameraStore.markUserMovedAway());
    map.on("zoomstart", () => store.mapCameraStore.markUserMovedAway());

    return () => {
      map.remove();
      mapRef.current = null;
    };
  }, [store]);

  // Camera reaction
  useEffect(() => {
    return reaction(
      () => store.mapCameraStore.revision,
      () => {
        const map = mapRef.current;
        if (!map || !map.isStyleLoaded()) return;
        pushCameraTarget(map, store);
      },
    );
  }, [store]);

  // Route data reaction — only re-push when identifier or revision changes
  useEffect(() => {
    return reaction(
      () => routeSignature(store),
      (signature) => {
        if (signature === lastRouteSignatureRef.current) return;
        lastRouteSignatureRef.current = signature;
        const map = mapRef.current;
        if (!map || !map.isStyleLoaded()) return;
        pushRouteData(map, store);
        pushMarkers(map, store);
      },
    );
  }, [store]);

  // Rider position reaction — only when we actually have a fix.
  useEffect(() => {
    return reaction(
      () => store.locationStore.currentLocation,
      () => {
        const map = mapRef.current;
        if (!map || !map.isStyleLoaded()) return;
        pushRider(map, store);
      },
    );
  }, [store]);

  // Long-press → drop pin (planning only)
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    const onPointerDown = (event: maplibregl.MapMouseEvent | maplibregl.MapTouchEvent) => {
      if (store.route !== "home" || store.guidanceStore.homeMode !== "planning") return;
      longPressStartRef.current = { x: event.point.x, y: event.point.y };
      if (longPressTimerRef.current) clearTimeout(longPressTimerRef.current);
      longPressTimerRef.current = setTimeout(() => {
        const ll = event.lngLat;
        store.planningStore.setDestinationFromMap({ latitude: ll.lat, longitude: ll.lng });
      }, 600);
    };
    const cancel = () => {
      if (longPressTimerRef.current) clearTimeout(longPressTimerRef.current);
      longPressTimerRef.current = null;
      longPressStartRef.current = null;
    };
    map.on("mousedown", onPointerDown);
    map.on("touchstart", onPointerDown);
    map.on("mouseup", cancel);
    map.on("touchend", cancel);
    map.on("mousemove", cancel);
    map.on("touchmove", cancel);
    map.on("dragstart", cancel);
    return () => {
      map.off("mousedown", onPointerDown);
      map.off("touchstart", onPointerDown);
      map.off("mouseup", cancel);
      map.off("touchend", cancel);
      map.off("mousemove", cancel);
      map.off("touchmove", cancel);
      map.off("dragstart", cancel);
    };
  }, [store]);

  return <div ref={containerRef} className="map-surface" />;
});

function addCompanionLayers(map: MaplibreMap): void {
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

function pushCameraTarget(map: MaplibreMap, store: RootStore): void {
  const target = store.mapCameraStore.target;
  if (target.kind === "center") {
    map.easeTo({
      center: [target.center.longitude, target.center.latitude],
      zoom: target.zoom,
      bearing: target.bearing,
      duration: 350,
    });
  } else {
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
}

function routeSignature(store: RootStore): string {
  const preview = store.planningStore.preview;
  return [
    store.guidanceStore.homeMode,
    preview.routeIdentifier ?? "",
    preview.routeRevision ?? "",
    preview.selectedAlternativeID ?? "",
    preview.alternatives.map((a) => a.normalizedPackage.routeIdentifier).join(","),
  ].join("|");
}

function pushRouteData(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(ROUTE_SOURCE_ID) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const features = buildRouteFeatures(store);
  source.setData({ type: "FeatureCollection", features });
}

function pushMarkers(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(MARKERS_SOURCE_ID) as maplibregl.GeoJSONSource | undefined;
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

function pushRider(map: MaplibreMap, store: RootStore): void {
  const source = map.getSource(RIDER_SOURCE_ID) as maplibregl.GeoJSONSource | undefined;
  if (!source) return;
  const rider = store.locationStore.currentLocation;
  source.setData({
    type: "FeatureCollection",
    features: rider ? [pointFeature(rider, "rider")] : [],
  });
}

function buildRouteFeatures(store: RootStore): GeoJSON.Feature[] {
  const features: GeoJSON.Feature[] = [];
  const preview = store.planningStore.preview;
  const homeMode = store.guidanceStore.homeMode;
  const selectedId = preview.selectedAlternativeID ?? preview.alternatives[0]?.id;
  const showAlternates = homeMode === "planning";
  for (const alt of preview.alternatives) {
    const isSelected = alt.id === selectedId;
    if (!showAlternates && !isSelected) continue;
    features.push(routeFeature(alt, isSelected));
  }
  return features;
}

function routeFeature(alt: RouteAlternative, selected: boolean): GeoJSON.Feature {
  return {
    type: "Feature",
    properties: { id: alt.id, selected },
    geometry: {
      type: "LineString",
      coordinates: alt.normalizedPackage.geometry.map((p) => [p.longitude, p.latitude]),
    },
  };
}

function pointFeature(point: CoordinatePoint, kind: string): GeoJSON.Feature {
  return {
    type: "Feature",
    properties: { kind },
    geometry: { type: "Point", coordinates: [point.longitude, point.latitude] },
  };
}
