import maplibregl, { type Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import {
  addDebugLayers,
  fitMapToData,
  handleMapClick,
  pushAllData,
  pushAnnotations,
  pushCueMarkers,
  pushGpsTrail,
  pushOffRouteMarkers,
  pushRider,
} from "./DebuggerLayers.js";
import styles from "./index.module.css";

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

type Props = {
  store: RootStore;
  onPopupOpen?: (popup: { content: string; lngLat: { lat: number; lng: number } } | null) => void;
};

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

  useEffect(() => {
    return reaction(
      () => _store.debuggerStore.session?.gpxGeometry,
      () => {
        const map = mapRef.current;
        if (!map || !readyRef.current) return;
        pushGpsTrail(map, _store);
      },
    );
  }, [_store]);

  useEffect(() => {
    return reaction(
      () => (_store.debuggerStore.mapFollowActive ? _store.debuggerStore.currentPosition : null),
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
      <div ref={containerRef} className={styles.surface} />
      <button
        type="button"
        className={[
          styles.followBtn,
          _store.debuggerStore.mapFollowActive && styles.followBtnActive,
        ]
          .filter(Boolean)
          .join(" ")}
        onClick={() =>
          _store.debuggerStore.setMapFollowActive(!_store.debuggerStore.mapFollowActive)
        }
        title={
          _store.debuggerStore.mapFollowActive
            ? "Stop following GPS position"
            : "Follow GPS position"
        }
      >
        <LocateIcon />
      </button>
    </>
  );
});

function LocateIcon() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      aria-hidden="true"
    >
      <title>Locate</title>
      <circle cx="10" cy="10" r="3" />
      <path d="M10 1v3M10 16v3M1 10h3M16 10h3" />
    </svg>
  );
}
