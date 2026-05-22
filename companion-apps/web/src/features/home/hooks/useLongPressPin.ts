import type maplibregl from "maplibre-gl";
import type { Map as MaplibreMap } from "maplibre-gl";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../../app/RootStore.js";

export function useLongPressPin(
  store: RootStore,
  mapRef: React.MutableRefObject<MaplibreMap | null>,
) {
  const longPressTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const longPressStartRef = useRef<{ x: number; y: number } | null>(null);

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
  }, [store, mapRef.current]);
}
