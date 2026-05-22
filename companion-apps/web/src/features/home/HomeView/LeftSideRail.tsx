import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import styles from "./LeftSideRail.module.css";

type Props = { store: RootStore };

/**
 * iOS-parity left-side rail. Renders `topLeftIconStack` items.
 * Order, top → bottom: zoom-in → zoom-out → alternate-routes (only in
 * routing). The alternate-routes button appears at the BOTTOM so the
 * zoom column never shifts position when the rider presses Start.
 */
export const LeftSideRail = observer(({ store }: Props) => {
  if (store.planningStore.isSearchOpen) return null;
  const guidance = store.guidanceStore;
  return (
    <div className={styles.railLeft}>
      {guidance.topLeftIconStack.map((item) => {
        if (item === "zoomIn") {
          return (
            <button
              key="zoomIn"
              type="button"
              className={styles.railIcon}
              aria-label="Zoom in"
              onClick={() => store.mapCameraStore.requestZoomDelta(1)}
            >
              +
            </button>
          );
        }
        if (item === "zoomOut") {
          return (
            <button
              key="zoomOut"
              type="button"
              className={styles.railIcon}
              aria-label="Zoom out"
              onClick={() => store.mapCameraStore.requestZoomDelta(-1)}
            >
              −
            </button>
          );
        }
        // alternateRoutes — split-way reroute: plan fresh alternatives
        // from the rider's current location to the same destination.
        return (
          <button
            key="alternateRoutes"
            type="button"
            className={styles.railIcon}
            aria-label="Find alternate routes"
            title="Find alternate routes from here"
            onClick={() => void store.exploreAlternateRoutes()}
          >
            ⇄
          </button>
        );
      })}
    </div>
  );
});
