import { reaction } from "mobx";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../../app/RootStore.js";

export function useAutoStartRoute(store: RootStore) {
  const lastTickRef = useRef(0);
  useEffect(() => {
    return reaction(
      () => store.historyStore.homeStartRequestTick,
      (tick) => {
        if (tick === lastTickRef.current) return;
        lastTickRef.current = tick;
        store.guidanceStore.startSelectedRoute();
      },
    );
  }, [store]);
}
