import type { DebuggerStore } from "../../../stores/DebuggerStore.js";
import styles from "./ExportTab.module.css";

type Props = { dStore: DebuggerStore };

export const ExportTab = ({ dStore }: Props) => (
  <div className={styles.content}>
    <div className={styles.exportSection}>
      <h4>Export Annotations</h4>
      <p>Download a JSON file with all annotations and event context for version control.</p>
      <button
        type="button"
        className={styles.exportBtn}
        onClick={() => {
          const exp = dStore.exportAnnotations();
          if (!exp) return;
          const blob = new Blob([JSON.stringify(exp, null, 2)], { type: "application/json" });
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = `annotations-${exp.sessionId}.json`;
          a.click();
          URL.revokeObjectURL(url);
        }}
      >
        Download Annotations JSON
      </button>
    </div>
  </div>
);
