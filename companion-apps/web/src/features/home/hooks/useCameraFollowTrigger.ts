import { reaction } from "mobx";
import { useEffect } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { refreshCameraForCurrentMode } from "../refreshCamera.js";

export function useCameraFollowTrigger(store: RootStore) {
  useEffect(() => {
    return reaction(
      () => ({
        homeMode: store.guidanceStore.homeMode,
        selectedId: store.planningStore.preview.selectedAlternativeID,
        revision: store.planningStore.preview.routeRevision,
      }),
      () => {
        if (store.guidanceStore.homeMode === "phoneGuidance") return;
        refreshCameraForCurrentMode(store);
      },
      { fireImmediately: true },
    );
  }, [store]);
}
