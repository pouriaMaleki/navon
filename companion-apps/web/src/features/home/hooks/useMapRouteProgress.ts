import type { Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { useEffect } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { pushRouteData } from "../mapLayerSetup.js";

export function useMapRouteProgress(
  store: RootStore,
  mapRef: React.MutableRefObject<MaplibreMap | null>,
  mapReadyRef: React.MutableRefObject<boolean>,
) {
  useEffect(() => {
    return reaction(
      () => store.guidanceStore.progressDistanceM,
      () => {
        if (store.guidanceStore.homeMode !== "phoneGuidance") return;
        const map = mapRef.current;
        if (!map || !mapReadyRef.current) return;
        pushRouteData(map, store);
      },
    );
  }, [store, mapReadyRef.current, mapRef.current]);
}
