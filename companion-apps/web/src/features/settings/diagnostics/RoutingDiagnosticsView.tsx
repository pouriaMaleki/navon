import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import { sessionDurationMs } from "../../../domain/routingDiagnosticsModels.js";
import styles from "./RoutingDiagnosticsView.module.css";

type Props = { store: RootStore };

export const RoutingDiagnosticsView = observer(({ store }: Props) => {
  const sessions = store.routingDiagnosticsStore.sessions;
  if (sessions.length === 0) {
    return (
      <div className={styles.section}>
        <div className={styles.empty}>No routing diagnostics recorded.</div>
      </div>
    );
  }
  return (
    <div className={styles.section}>
      {sessions.map((session) => {
        const startTime = new Date(session.createdAtMs).toLocaleString();
        const dur = sessionDurationMs(session);
        const durStr =
          dur >= 60000 ? `${Math.round(dur / 60000)} min` : `${Math.round(dur / 1000)} sec`;
        return (
          <div key={session.id} className={[styles.row, styles.columnRow].join(" ")}>
            <div className={styles.label}>{startTime}</div>
            <div className={styles.hint}>
              {session.events.length} events over {durStr}
            </div>
            <div className={styles.btnRow}>
              <button
                type="button"
                className={[styles.iconBtn, styles.iconBtnAuto].join(" ")}
                onClick={() => store.openDebuggerForSession(session.id)}
              >
                Open in Debugger
              </button>
              <button
                type="button"
                className={[styles.iconBtn, styles.iconBtnAuto].join(" ")}
                onClick={() => {
                  const pkg = store.routingDiagnosticsStore.sessionDebugPackage(session.id);
                  if (pkg) navigator.clipboard?.writeText(pkg);
                }}
              >
                Copy debug
              </button>
              <button
                type="button"
                className={[styles.iconBtn, styles.iconBtnAuto].join(" ")}
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
