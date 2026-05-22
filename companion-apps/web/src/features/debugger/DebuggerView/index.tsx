import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useRef, useState } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import type { RoutingDiagDebugPackage } from "../../../domain/routingDiagnosticsModels.js";
import { DebuggerAnnotationForm } from "../DebuggerAnnotationForm/index.js";
import { DebuggerEventPanel } from "../DebuggerEventPanel/index.js";
import { DebuggerMapSurface } from "../DebuggerMapSurface/index.js";
import { DebuggerTimeline } from "../DebuggerTimeline/index.js";
import { DebuggerEmptyState } from "./DebuggerEmptyState.js";
import { DebuggerHeader } from "./DebuggerHeader.js";
import styles from "./index.module.css";
import { parseGpx } from "./parseGpx.js";

type Props = { store: RootStore };

export const DebuggerView = observer(({ store }: Props) => {
  const dStore = store.debuggerStore;
  const [importError, setImportError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [mapPopup, setMapPopup] = useState<{
    content: string;
    lngLat: { lat: number; lng: number };
  } | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const handleFileImport = useCallback(
    (file: File) => {
      setImportError(null);
      file
        .text()
        .then((content) => {
          try {
            const pkg = JSON.parse(content) as RoutingDiagDebugPackage;
            if (!pkg.formatVersion || !pkg.events || !Array.isArray(pkg.events)) {
              setImportError("Invalid diagnostic file: missing formatVersion or events array.");
              return;
            }
            dStore.loadSessionFromPackage(pkg);
          } catch {
            setImportError("Failed to parse JSON. Make sure it's a valid diagnostic file.");
          }
        })
        .catch(() => {
          setImportError("Failed to read file.");
        });
    },
    [dStore],
  );

  const handleGpxImport = useCallback(
    (file: File) => {
      if (!dStore.session) return;
      file
        .text()
        .then((content) => {
          const points = parseGpx(content);
          if (points.length === 0) {
            setImportError("No track points found in GPX file.");
            return;
          }
          dStore.loadGpxGeometry(points);
          setImportError(null);
        })
        .catch(() => {
          setImportError("Failed to read GPX file.");
        });
    },
    [dStore],
  );

  // Drag-and-drop handlers
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    if (e.currentTarget === e.target) setIsDragOver(false);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);
      const file = e.dataTransfer.files[0];
      if (!file) return;
      if (file.name.endsWith(".gpx")) {
        handleGpxImport(file);
      } else {
        handleFileImport(file);
      }
    },
    [handleFileImport, handleGpxImport],
  );

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement
      )
        return;
      if (e.key === " ") {
        e.preventDefault();
        dStore.playbackState === "playing" ? dStore.pause() : dStore.play();
      }
      if (e.key === "ArrowRight") {
        e.preventDefault();
        dStore.stepForward();
      }
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        dStore.stepBackward();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dStore]);

  if (!dStore.session) {
    return (
      <DebuggerEmptyState
        isDragOver={isDragOver}
        importError={importError}
        fileInputRef={fileInputRef}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        onFileChange={(e) => {
          const file = e.target.files?.[0];
          if (!file) return;
          if (file.name.endsWith(".gpx")) {
            handleGpxImport(file);
          } else {
            handleFileImport(file);
          }
          e.target.value = "";
        }}
        onChooseFile={() => fileInputRef.current?.click()}
        onBackToSettings={() => store.goSettings()}
      />
    );
  }

  return (
    <div
      className={styles.view}
      data-view="debugger"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {isDragOver && <div className={styles.dropOverlay}>Drop file to import</div>}

      <DebuggerHeader
        sessionId={dStore.session.diagSession.id}
        eventCount={dStore.session.diagSession.events.length}
        hasGpxGeometry={!!dStore.session.gpxGeometry}
        fileInputRef={fileInputRef}
        onBackToSettings={() => {
          dStore.stopPlayback();
          store.goSettings();
        }}
        onLoadGpx={() => fileInputRef.current?.click()}
        onGpxFileChange={(e) => {
          const file = e.target.files?.[0];
          if (file) handleGpxImport(file);
          e.target.value = "";
        }}
      />

      {/* 3-panel body */}
      <div className={styles.body}>
        <div className={styles.map}>
          <DebuggerMapSurface store={store} onPopupOpen={setMapPopup} />
        </div>

        {mapPopup && (
          <div className={styles.popupBar}>
            <div
              className={styles.popupContent}
              dangerouslySetInnerHTML={{ __html: mapPopup.content }}
            />
            <button type="button" className={styles.popupClose} onClick={() => setMapPopup(null)}>
              &times;
            </button>
          </div>
        )}

        <div
          className={[styles.sidebar, !sidebarOpen && styles.sidebarClosed]
            .filter(Boolean)
            .join(" ")}
        >
          <DebuggerEventPanel store={store} onCloseSidebar={() => setSidebarOpen(false)} />
        </div>
        {!sidebarOpen && (
          <button
            type="button"
            className={styles.sidebarToggle}
            onClick={() => setSidebarOpen(true)}
            title="Show sidebar"
          >
            <SidebarToggleIcon />
          </button>
        )}
      </div>

      {/* Annotation form overlay */}
      {dStore.pendingAnnotationTimeMs !== null && (
        <div className={styles.annotationOverlay}>
          <DebuggerAnnotationForm store={store} />
        </div>
      )}

      {/* Bottom timeline */}
      <div className={styles.timeline}>
        <DebuggerTimeline store={store} />
      </div>

      {importError && (
        <button type="button" className={styles.toast} onClick={() => setImportError(null)}>
          {importError}
        </button>
      )}
    </div>
  );
});

function SidebarToggleIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      aria-hidden="true"
    >
      <title>Show sidebar</title>
      <path d="M6 4l4 4-4 4" />
    </svg>
  );
}
