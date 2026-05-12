import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { sessionDurationMs } from "../../domain/routingDiagnosticsModels.js";

type Props = { store: RootStore };

export const RoutingDiagnosticsView = observer(({ store }: Props) => {
  const sessions = store.routingDiagnosticsStore.sessions;
  if (sessions.length === 0) {
    return (
      <div className="settings-section">
        <div className="empty-state">No routing diagnostics recorded.</div>
      </div>
    );
  }
  return (
    <div className="settings-section">
      {sessions.map((session) => {
        const startTime = new Date(session.createdAtMs).toLocaleString();
        const dur = sessionDurationMs(session);
        const durStr = dur >= 60000 ? `${Math.round(dur / 60000)} min` : `${Math.round(dur / 1000)} sec`;
        return (
          <div
            key={session.id}
            className="settings-row"
            style={{ flexDirection: "column", alignItems: "stretch" }}
          >
            <div className="settings-row__label">{startTime}</div>
            <div className="settings-row__hint">
              {session.events.length} events over {durStr}
            </div>
            <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
              <button
                type="button"
                className="icon-button"
                style={{ width: "auto", padding: "8px 12px" }}
                onClick={() => store.openDebuggerForSession(session.id)}
              >
                Open in Debugger
              </button>
              <button
                type="button"
                className="icon-button"
                style={{ width: "auto", padding: "8px 12px" }}
                onClick={() => {
                  const pkg = store.routingDiagnosticsStore.sessionDebugPackage(session.id);
                  if (pkg) navigator.clipboard?.writeText(pkg);
                }}
              >
                Copy debug
              </button>
              <button
                type="button"
                className="icon-button"
                style={{ width: "auto", padding: "8px 12px" }}
                onClick={() => store.routingDiagnosticsStore.deleteSession(session.id)}
              >
                Delete
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
});
