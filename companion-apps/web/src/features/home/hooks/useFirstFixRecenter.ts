import { reaction } from "mobx";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { refreshCameraForCurrentMode } from "../refreshCamera.js";

export function useFirstFixRecenter(store: RootStore) {
  const firstFixDoneRef = useRef(false);
  useEffect(() => {
    return reaction(
      () => store.locationStore.currentLocation !== undefined,
      (haveFix) => {
        if (!haveFix) return;
        if (firstFixDoneRef.current) return;
        firstFixDoneRef.current = true;
        if (store.guidanceStore.homeMode === "phoneGuidance") return;
        refreshCameraForCurrentMode(store);
      },
      { fireImmediately: true },
    );
  }, [store]);
}
