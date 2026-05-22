import type { Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { useEffect } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { dispatchCameraTarget } from "../cameraDispatcher.js";
import type { MapInteractionGate } from "../MapInteractionGate.js";

export function useMapCamera(
  store: RootStore,
  mapRef: React.MutableRefObject<MaplibreMap | null>,
  mapReadyRef: React.MutableRefObject<boolean>,
  interactionGateRef: React.MutableRefObject<MapInteractionGate>,
) {
  useEffect(() => {
    return reaction(
      () => store.mapCameraStore.revision,
      () => {
        const map = mapRef.current;
        if (!map || !mapReadyRef.current) return;
        interactionGateRef.current.recordProgrammaticMove(Date.now());
        dispatchCameraTarget(map, store, true);
      },
    );
  }, [store]);
}
