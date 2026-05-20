import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import styles from "./ImportDiagnosticsView.module.css";

type Props = { store: RootStore };

export const ImportDiagnosticsView = observer(({ store }: Props) => {
  const entries = store.historyStore.importDiagnosticsEntries;
  if (entries.length === 0) {
    return (
      <div className={styles.section}>
        <div className={styles.empty}>No failed imports recorded.</div>
      </div>
    );
  }
  return (
    <div className={styles.section}>
      {entries.map((entry) => {
        const subtitle =
          entry.envelope.note ??
          entry.envelope.originalText?.slice(0, 80) ??
          entry.envelope.originalURL ??
          entry.envelope.fileName ??
          "Unknown payload";
        return (
          <div key={entry.id} className={[styles.row, styles.columnRow].join(" ")}>
            <div className={styles.label}>{entry.envelope.classification}</div>
            <div className={styles.hint}>{subtitle}</div>
            <div className={styles.btnRow}>
              <button
                type="button"
                className={[styles.iconBtn, styles.iconBtnAuto].join(" ")}
                onClick={() => navigator.clipboard?.writeText(JSON.stringify(entry, null, 2))}
              >
                Copy debug
              </button>
              <button
                type="button"
                className={[styles.iconBtn, styles.iconBtnAuto].join(" ")}
                onClick={() => store.historyStore.removeImportDiagnostics(entry.id)}
              >
                Dismiss
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
});
