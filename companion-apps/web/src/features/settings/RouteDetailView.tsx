import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { summaryLine } from "../../domain/models.js";
import styles from "./RouteDetailView.module.css";

type Props = { store: RootStore; itemId: string; onClose: () => void };

export const RouteDetailView = observer(({ store, itemId, onClose }: Props) => {
  const item = store.historyStore.routeHistoryItems.find((i) => i.id === itemId);
  if (!item) {
    return <div className={styles.section}>Route not found.</div>;
  }
  const package_ = item.routePackage;
  return (
    <div className={styles.section}>
      <div className={[styles.label, styles.bigLabel].join(" ")}>{item.title}</div>
      <div className={styles.hint}>{item.sourceLabel}</div>
      {item.subtitle ? (
        <div className={styles.notice} style={{ marginTop: 6 }}>
          {item.subtitle}
        </div>
      ) : null}
      {package_ ? (
        <div className={styles.meta}>
          <div className={styles.notice}>{summaryLine(package_)}</div>
          <div className={styles.notice}>
            {package_.geometry.length} geometry points • {package_.maneuvers.length} maneuvers
          </div>
        </div>
      ) : null}
      <div className={styles.actions}>
        <button
          type="button"
          className={[styles.primaryBtn, styles.flexBtn].join(" ")}
          onClick={() => {
            void store.activateRouteHistoryItem(item, false);
            onClose();
          }}
        >
          Open
        </button>
        <button
          type="button"
          className={[styles.primaryBtn, styles.flexBtn].join(" ")}
          onClick={() => {
            void store.activateRouteHistoryItem(item, true);
            onClose();
          }}
        >
          Start
        </button>
        <button
          type="button"
          className={[styles.dangerBtn, styles.flexBtn].join(" ")}
          onClick={() => {
            store.historyStore.removeRouteHistoryItem(item.id);
            onClose();
          }}
        >
          Dismiss
        </button>
      </div>
    </div>
  );
});
