import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import { ActiveGuidanceCard } from "../ActiveGuidanceCard.js";
import { RouteSuggestionsCard } from "../RouteSuggestionsCard.js";
import styles from "./BottomOverlay.module.css";

type Props = { store: RootStore };

export const BottomOverlay = observer(({ store }: Props) => {
  const planning = store.planningStore;
  const guidance = store.guidanceStore;
  const status = planning.planningStatus ?? planning.importActivityStatus;
  if (guidance.homeMode === "phoneGuidance") {
    if (guidance.isExploringAlternativesFromGuidance) {
      return (
        <div className={styles.overlayBottom} data-overlay="bottom">
          <RouteSuggestionsCard store={store} />
        </div>
      );
    }
    return (
      <div className={styles.overlayBottom} data-overlay="bottom">
        <ActiveGuidanceCard store={store} />
      </div>
    );
  }
  if (status) {
    return (
      <div className={styles.overlayBottom} data-overlay="bottom">
        <div className={styles.card}>
          <div className={styles.title}>Working on route</div>
          <div className={styles.subtitle}>{status}</div>
        </div>
      </div>
    );
  }
  if (guidance.arrivalNotice) {
    return (
      <div className={styles.overlayBottom} data-overlay="bottom">
        <output
          className={styles.card}
          style={{ display: "flex", alignItems: "flex-start", gap: 12 }}
        >
          <div style={{ flex: 1 }}>
            <div className={styles.title}>{guidance.arrivalNotice}</div>
            <div className={styles.subtitle}>
              Routing finished. Tap a destination to plan again.
            </div>
          </div>
          <button
            type="button"
            className={styles.closeBtn}
            aria-label="Close arrival message"
            onClick={() => guidance.dismissArrivalNotice()}
          >
            ×
          </button>
        </output>
      </div>
    );
  }
  if (planning.preview.alternatives.length > 0 || planning.preview.planningNotice) {
    return (
      <div className={styles.overlayBottom} data-overlay="bottom">
        <RouteSuggestionsCard store={store} />
      </div>
    );
  }
  return null;
});
