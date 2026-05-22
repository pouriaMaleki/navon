import {
  ANNOTATION_SEVERITY_LABELS,
  ANNOTATION_TAG_LABELS,
} from "../../../domain/debuggerModels.js";
import type { DebuggerStore } from "../../../stores/DebuggerStore.js";
import styles from "./AnnotationList.module.css";

type Props = {
  dStore: DebuggerStore;
  formatTime: (ts: number) => string;
  sessionStartMs: number;
};

export const AnnotationList = ({ dStore, formatTime, sessionStartMs }: Props) => {
  if (dStore.annotations.length === 0) {
    return (
      <div className={styles.content}>
        <div className={styles.empty}>
          No annotations yet. Double-click the timeline or click map markers to add notes.
        </div>
      </div>
    );
  }

  return (
    <div className={styles.content}>
      <div className={styles.annotations}>
        {dStore.annotations.map((ann) => (
          <button
            type="button"
            key={ann.id}
            className={[
              styles.annotation,
              dStore.selectedAnnotationId === ann.id && styles.annotationSelected,
            ]
              .filter(Boolean)
              .join(" ")}
            onClick={() => dStore.selectAnnotation(ann.id)}
          >
            <div className={styles.annotationHeader}>
              <span
                className={[
                  styles.badge,
                  styles[`badge${ann.severity.charAt(0).toUpperCase() + ann.severity.slice(1)}`],
                ].join(" ")}
              >
                {ANNOTATION_SEVERITY_LABELS[ann.severity]}
              </span>
              <span className={[styles.badge, styles.badgeTag].join(" ")}>
                {ANNOTATION_TAG_LABELS[ann.tag]}
              </span>
              <span className={styles.annotationTime}>
                {formatTime(sessionStartMs + ann.timeRangeMs[0])}
              </span>
              <button
                type="button"
                className={styles.deleteBtn}
                onClick={(e) => {
                  e.stopPropagation();
                  dStore.deleteAnnotation(ann.id);
                }}
                title="Delete"
              >
                x
              </button>
            </div>
            <div className={styles.annotationNote}>{ann.note}</div>
          </button>
        ))}
      </div>
    </div>
  );
};
