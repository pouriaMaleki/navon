import type { RefObject } from "react";
import styles from "./DebuggerEmptyState.module.css";

type Props = {
  isDragOver: boolean;
  importError: string | null;
  fileInputRef: RefObject<HTMLInputElement | null>;
  onDragOver: (e: React.DragEvent) => void;
  onDragLeave: (e: React.DragEvent) => void;
  onDrop: (e: React.DragEvent) => void;
  onFileChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onChooseFile: () => void;
  onBackToSettings: () => void;
};

export const DebuggerEmptyState = ({
  isDragOver,
  importError,
  fileInputRef,
  onDragOver,
  onDragLeave,
  onDrop,
  onFileChange,
  onChooseFile,
  onBackToSettings,
}: Props) => (
  <div className={styles.empty} onDragOver={onDragOver} onDragLeave={onDragLeave} onDrop={onDrop}>
    <div
      className={[styles.dropzone, isDragOver && styles.dropzoneActive].filter(Boolean).join(" ")}
    >
      <div className={styles.emptyIcon}>
        <svg
          width="48"
          height="48"
          viewBox="0 0 48 48"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden="true"
        >
          <title>Upload diagnostic file</title>
          <path d="M24 4v28M14 18l10-10 10 10M8 36v4a4 4 0 004 4h24a4 4 0 004-4v-4" />
        </svg>
      </div>
      <h3>Navon Diagnostics Debugger</h3>
      <p>Drop a diagnostic JSON file here to visualize the session</p>
      <p className={styles.emptyHint}>Or drop a .gpx file to add route geometry</p>
      <button type="button" className={styles.emptyBtn} onClick={onChooseFile}>
        Choose File
      </button>
      <input
        ref={fileInputRef}
        type="file"
        accept=".json,.txt,.gpx"
        style={{ display: "none" }}
        onChange={onFileChange}
      />
      {importError && <div className={styles.emptyError}>{importError}</div>}
    </div>
    <button type="button" className={styles.emptyBack} onClick={onBackToSettings}>
      Back to Settings
    </button>
  </div>
);
