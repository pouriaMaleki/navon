import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";

type Props = { store: RootStore };

export const ImportDiagnosticsView = observer(({ store }: Props) => {
  const entries = store.historyStore.importDiagnosticsEntries;
  if (entries.length === 0) {
    return (
      <div className="settings-section">
        <div className="empty-state">No failed imports recorded.</div>
      </div>
    );
  }
  return (
    <div className="settings-section">
      {entries.map((entry) => {
        const subtitle =
          entry.envelope.note ??
          entry.envelope.originalText?.slice(0, 80) ??
          entry.envelope.originalURL ??
          entry.envelope.fileName ??
          "Unknown payload";
        return (
          <div
            key={entry.id}
            className="settings-row"
            style={{ flexDirection: "column", alignItems: "stretch" }}
          >
            <div className="settings-row__label">{entry.envelope.classification}</div>
            <div className="settings-row__hint">{subtitle}</div>
            <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
              <button
                type="button"
                className="icon-button"
                style={{ width: "auto", padding: "8px 12px" }}
                onClick={() => navigator.clipboard?.writeText(JSON.stringify(entry, null, 2))}
              >
                Copy debug
              </button>
              <button
                type="button"
                className="icon-button"
                style={{ width: "auto", padding: "8px 12px" }}
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
