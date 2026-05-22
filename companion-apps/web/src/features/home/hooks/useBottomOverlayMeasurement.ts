import { useEffect } from "react";
import type { RootStore } from "../../../app/RootStore.js";

export function useBottomOverlayMeasurement(store: RootStore) {
  useEffect(() => {
    const update = () => {
      const el = document.querySelector('[data-overlay="bottom"]') as HTMLElement | null;
      const h = el ? el.getBoundingClientRect().height : 0;
      store.mapCameraStore.setBottomReservedPx(h + 12);
    };
    update();
    const observer = new ResizeObserver(update);
    const el = document.querySelector('[data-overlay="bottom"]');
    if (el) observer.observe(el);
    const mo = new MutationObserver(() => {
      const next = document.querySelector('[data-overlay="bottom"]');
      if (next && next !== el) {
        observer.disconnect();
        observer.observe(next);
      }
      update();
    });
    mo.observe(document.body, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      mo.disconnect();
    };
  }, [store]);
}
