import type { Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { useEffect } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { pushRider } from "../mapLayerSetup.js";

export function useMapRider(
  store: RootStore,
  mapRef: React.MutableRefObject<MaplibreMap | null>,
  mapReadyRef: React.MutableRefObject<boolean>,
) {
  useEffect(() => {
    return reaction(
      () => store.locationStore.currentLocation,
      () => {
        const map = mapRef.current;
        if (!map || !mapReadyRef.current) return;
        pushRider(map, store);
      },
    );
  }, [store, mapReadyRef.current, mapRef.current]);
}
