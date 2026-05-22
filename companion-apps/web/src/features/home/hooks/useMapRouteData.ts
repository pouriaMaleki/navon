import type { Map as MaplibreMap } from "maplibre-gl";
import { reaction } from "mobx";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { pushMarkers, pushRouteData } from "../mapLayerSetup.js";

export function useMapRouteData(
  store: RootStore,
  mapRef: React.MutableRefObject<MaplibreMap | null>,
  mapReadyRef: React.MutableRefObject<boolean>,
) {
  const lastSignatureRef = useRef("");
  useEffect(() => {
    return reaction(
      () => routeSignature(store),
      (signature) => {
        if (signature === lastSignatureRef.current) return;
        lastSignatureRef.current = signature;
        const map = mapRef.current;
        if (!map || !mapReadyRef.current) return;
        pushRouteData(map, store);
        pushMarkers(map, store);
      },
    );
  }, [store]);
}

function routeSignature(store: RootStore): string {
  const preview = store.planningStore.preview;
  const guidance = store.guidanceStore;
  return [
    guidance.homeMode,
    preview.routeIdentifier ?? "",
    preview.routeRevision ?? "",
    preview.selectedAlternativeID ?? "",
    preview.alternatives.map((a) => a.normalizedPackage.routeIdentifier).join(","),
    guidance.isExploringAlternativesFromGuidance ? "1" : "0",
    guidance.selectedAlternativeIDForDisplay ?? "",
  ].join("|");
}
