import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { summaryLine } from "../../domain/models.js";

type Props = { store: RootStore; itemId: string; onClose: () => void };

export const RouteDetailView = observer(({ store, itemId, onClose }: Props) => {
  const item = store.historyStore.routeHistoryItems.find((i) => i.id === itemId);
  if (!item) {
    return <div className="settings-section">Route not found.</div>;
  }
  const package_ = item.routePackage;
  return (
    <div className="settings-section">
      <div className="settings-row__label" style={{ fontSize: 18 }}>
        {item.title}
      </div>
      <div className="settings-row__hint">{item.sourceLabel}</div>
      {item.subtitle ? (
        <div className="notice" style={{ marginTop: 6 }}>
          {item.subtitle}
        </div>
      ) : null}
      {package_ ? (
        <div style={{ marginTop: 12 }}>
          <div className="notice">{summaryLine(package_)}</div>
          <div className="notice">
            {package_.geometry.length} geometry points • {package_.maneuvers.length} maneuvers
          </div>
        </div>
      ) : null}
      <div style={{ display: "flex", gap: 8, marginTop: 16 }}>
        <button
          type="button"
          className="primary-button"
          style={{ flex: 1 }}
          onClick={() => {
            void store.activateRouteHistoryItem(item, false);
            onClose();
          }}
        >
          Open
        </button>
        <button
          type="button"
          className="primary-button"
          style={{ flex: 1 }}
          onClick={() => {
            void store.activateRouteHistoryItem(item, true);
            onClose();
          }}
        >
          Start
        </button>
        <button
          type="button"
          className="danger-button"
          style={{ flex: 1 }}
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
