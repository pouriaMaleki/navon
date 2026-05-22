import maplibregl, { type Map as MaplibreMap } from "maplibre-gl";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { MapInteractionGate } from "./MapInteractionGate.js";
import { useLongPressPin } from "./hooks/useLongPressPin.js";
import { useMapCamera } from "./hooks/useMapCamera.js";
import { useMapRider } from "./hooks/useMapRider.js";
import { useMapRouteData } from "./hooks/useMapRouteData.js";
import { useMapRouteProgress } from "./hooks/useMapRouteProgress.js";
import { useMapZoom } from "./hooks/useMapZoom.js";
import { addCompanionLayers, pushMarkers, pushRider, pushRouteData } from "./mapLayerSetup.js";
import { dispatchCameraTarget } from "./cameraDispatcher.js";
import styles from "./MapSurface.module.css";

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

type Props = { store: RootStore };

export const MapSurface = observer(({ store }: Props) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MaplibreMap | null>(null);
  const mapReadyRef = useRef(false);
  // 600 ms quiet window matches the easeTo duration (350 ms) + fitBounds
  // duration (400 ms) with headroom, but is short enough that a genuine
  // user gesture a moment later is still recognised.
  const interactionGateRef = useRef<MapInteractionGate>(new MapInteractionGate(600));

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
      mapReadyRef.current = true;
      interactionGateRef.current.recordProgrammaticMove(Date.now());
      dispatchCameraTarget(map, store, true);
      pushRouteData(map, store);
      pushMarkers(map, store);
      pushRider(map, store);
    });
    // Track user map interactions so the routing camera can auto-recenter
    // after the pinned inactivity timeout (spec line 104). GuidanceStore
    // no-ops this when not in phoneGuidance, so planning mode is unaffected.
    //
    // MapLibre fires drag/zoom/rotate/pitchstart for OUR OWN easeTo and
    // fitBounds calls. Use `interactionGateRef` to suppress those echoes —
    // without it, compass-lock would snap the camera to follow-rider ~1.3 s
    // after every programmatic animation (the '🧭 reverts after 1.3s' bug).
    const noteInteraction = (): void => {
      if (!interactionGateRef.current.isLikelyUserEvent(Date.now())) return;
      store.mapCameraStore.markUserMovedAway();
      store.guidanceStore.noteUserMapInteraction();
    };
    map.on("dragstart", noteInteraction);
    map.on("zoomstart", noteInteraction);
    map.on("rotatestart", noteInteraction);
    map.on("pitchstart", noteInteraction);

    return () => {
      map.remove();
      mapRef.current = null;
    };
  }, [store]);

  useMapCamera(store, mapRef, mapReadyRef, interactionGateRef);
  useMapRouteData(store, mapRef, mapReadyRef);
  useMapRider(store, mapRef, mapReadyRef);
  useMapRouteProgress(store, mapRef, mapReadyRef);
  useMapZoom(store, mapRef, mapReadyRef);
  useLongPressPin(store, mapRef);

  return <div ref={containerRef} className={styles.surface} />;
});
