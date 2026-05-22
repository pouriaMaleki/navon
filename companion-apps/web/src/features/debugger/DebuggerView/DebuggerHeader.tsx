import type { RefObject } from "react";
import styles from "./DebuggerHeader.module.css";

type Props = {
  sessionId: string;
  eventCount: number;
  hasGpxGeometry: boolean;
  fileInputRef: RefObject<HTMLInputElement | null>;
  onBackToSettings: () => void;
  onLoadGpx: () => void;
  onGpxFileChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
};

export const DebuggerHeader = ({
  sessionId,
  eventCount,
  hasGpxGeometry,
  fileInputRef,
  onBackToSettings,
  onLoadGpx,
  onGpxFileChange,
}: Props) => (
  <div className={styles.header}>
    <button type="button" className={styles.backBtn} onClick={onBackToSettings}>
      &larr; Settings
    </button>
    <span className={styles.sessionId}>{sessionId}</span>
    <span className={styles.count}>{eventCount} events</span>
    <div className={styles.headerActions}>
      <button type="button" className={styles.headerBtn} onClick={onLoadGpx}>
        Load GPX
      </button>
      <input
        ref={fileInputRef}
        type="file"
        accept=".gpx"
        style={{ display: "none" }}
        onChange={onGpxFileChange}
      />
      {hasGpxGeometry && <span className={styles.gpxBadge}>GPX loaded</span>}
    </div>
  </div>
);
