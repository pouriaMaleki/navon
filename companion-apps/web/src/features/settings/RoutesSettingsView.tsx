import { observer } from "mobx-react-lite";
import { useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import styles from "./RoutesSettingsView.module.css";

type Props = { store: RootStore; onOpenDetail: (id: string) => void };

export const RoutesSettingsView = observer(({ store, onOpenDetail }: Props) => {
  const fileInputRef = useRef<HTMLInputElement>(null);

  return (
    <>
      <div className={styles.section}>
        <h2>Import</h2>
        <div className={styles.row}>
          <div>
            <div className={styles.label}>Import GPX file</div>
            <div className={styles.hint}>Drag-and-drop also works anywhere on the map.</div>
          </div>
          <button
            type="button"
            className={styles.primaryBtn}
            style={{ width: "auto" }}
            onClick={() => fileInputRef.current?.click()}
          >
            Choose file
          </button>
        </div>
        <input
          ref={fileInputRef}
          type="file"
          accept=".gpx,application/gpx+xml,text/xml,application/xml"
          style={{ display: "none" }}
          onChange={async (event) => {
            const file = event.target.files?.[0];
            if (!file) return;
            const content = await file.text();
            await store.importGpxFile(file.name, content);
            event.target.value = "";
          }}
        />
      </div>

      <div className={styles.section}>
        <h2>Recent routes</h2>
        {store.historyStore.routeHistoryItems.length === 0 ? (
          <div className={styles.empty}>No recent routes yet.</div>
        ) : (
          store.historyStore.routeHistoryItems.map((item) => (
            <div key={item.id} className={styles.row}>
              <button
                type="button"
                style={{ flex: 1, textAlign: "start" }}
                onClick={() => void store.activateRouteHistoryItem(item, false)}
              >
                <div className={styles.label}>{item.title}</div>
                <div className={styles.hint}>
                  {item.subtitle ? `${item.subtitle} • ` : ""}
                  {item.sourceLabel}
                </div>
              </button>
              <button
                type="button"
                className={styles.iconBtn}
                aria-label="Route detail"
                onClick={() => onOpenDetail(item.id)}
              >
                ⓘ
              </button>
            </div>
          ))
        )}
      </div>
    </>
  );
});
