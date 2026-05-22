import type { Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { useEffect } from "react";
import type { RootStore } from "../../../app/RootStore.js";

export function useMapZoom(
  store: RootStore,
  mapRef: React.MutableRefObject<MaplibreMap | null>,
  mapReadyRef: React.MutableRefObject<boolean>,
) {
  useEffect(() => {
    return reaction(
      () => store.mapCameraStore.zoomTick,
      () => {
        const map = mapRef.current;
        if (!map || !mapReadyRef.current) return;
        const delta = store.mapCameraStore.lastZoomDelta;
        if (!delta) return;
        const next = Math.max(1, Math.min(20, map.getZoom() + delta));
        map.easeTo({ zoom: next, duration: 200 });
        if (store.guidanceStore.homeMode === "phoneGuidance") {
          store.settingsStore.updateSettings({ ridingZoom: next });
        }
      },
    );
  }, [store]);
}
