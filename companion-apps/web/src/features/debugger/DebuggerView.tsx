import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useRef, useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import type { RoutingDiagDebugPackage } from "../../domain/routingDiagnosticsModels.js";
import { DebuggerAnnotationForm } from "./DebuggerAnnotationForm.js";
import { DebuggerEventPanel } from "./DebuggerEventPanel.js";
import { DebuggerMapSurface } from "./DebuggerMapSurface.js";
import { DebuggerTimeline } from "./DebuggerTimeline.js";

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
      <div
        className="debugger-empty"
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <div
          className={`debugger-empty__dropzone${isDragOver ? " debugger-empty__dropzone--active" : ""}`}
        >
          <div className="debugger-empty__icon">
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
          <p className="debugger-empty__hint">Or drop a .gpx file to add route geometry</p>
          <button
            type="button"
            className="debugger-empty__btn"
            onClick={() => fileInputRef.current?.click()}
          >
            Choose File
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".json,.txt,.gpx"
            style={{ display: "none" }}
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              if (file.name.endsWith(".gpx")) {
                handleGpxImport(file);
              } else {
                handleFileImport(file);
              }
              e.target.value = "";
            }}
          />
          {importError && <div className="debugger-empty__error">{importError}</div>}
        </div>
        <button type="button" className="debugger-empty__back" onClick={() => store.goSettings()}>
          Back to Settings
        </button>
      </div>
    );
  }

  return (
    <div
      className="debugger-view"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {isDragOver && <div className="debugger-view__drop-overlay">Drop file to import</div>}

      {/* Header bar */}
      <div className="debugger-view__header">
        <button
          type="button"
          className="debugger-view__back-btn"
          onClick={() => {
            dStore.stopPlayback();
            store.goSettings();
          }}
        >
          &larr; Settings
        </button>
        <span className="debugger-view__session-id">{dStore.session.diagSession.id}</span>
        <span className="debugger-view__event-count">
          {dStore.session.diagSession.events.length} events
        </span>
        <div className="debugger-view__header-actions">
          <button
            type="button"
            className="debugger-view__header-btn"
            onClick={() => fileInputRef.current?.click()}
          >
            Load GPX
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".gpx"
            style={{ display: "none" }}
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) handleGpxImport(file);
              e.target.value = "";
            }}
          />
          {dStore.session.gpxGeometry && (
            <span className="debugger-view__gpx-badge">GPX loaded</span>
          )}
        </div>
      </div>

      {/* 3-panel body */}
      <div className="debugger-view__body">
        <div className="debugger-view__map">
          <DebuggerMapSurface store={store} onPopupOpen={setMapPopup} />
        </div>

        {mapPopup && (
          <div className="debugger-view__popup-bar">
            <div
              className="debugger-view__popup-content"
              dangerouslySetInnerHTML={{ __html: mapPopup.content }}
            />
            <button
              type="button"
              className="debugger-view__popup-close"
              onClick={() => setMapPopup(null)}
            >
              &times;
            </button>
          </div>
        )}

        <div
          className={`debugger-view__sidebar${!sidebarOpen ? " debugger-view__sidebar--closed" : ""}`}
        >
          <DebuggerEventPanel store={store} onCloseSidebar={() => setSidebarOpen(false)} />
        </div>
        {!sidebarOpen && (
          <button
            type="button"
            className="debugger-view__sidebar-toggle"
            onClick={() => setSidebarOpen(true)}
            title="Show sidebar"
          >
            <SidebarToggleIcon />
          </button>
        )}
      </div>

      {/* Annotation form overlay */}
      {dStore.pendingAnnotationTimeMs !== null && (
        <div className="debugger-view__annotation-overlay">
          <DebuggerAnnotationForm store={store} />
        </div>
      )}

      {/* Bottom timeline */}
      <div className="debugger-view__timeline">
        <DebuggerTimeline store={store} />
      </div>

      {importError && (
        <button type="button" className="debugger-view__toast" onClick={() => setImportError(null)}>
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

function parseGpx(content: string): { latitude: number; longitude: number }[] {
  const points: { latitude: number; longitude: number }[] = [];
  const trkptRegex = /<trkpt[^>]*lat="([^"]+)"[^>]*lon="([^"]+)"/g;
  let match: RegExpExecArray | null;
  while ((match = trkptRegex.exec(content)) !== null) {
    points.push({
      latitude: Number.parseFloat(match[1]),
      longitude: Number.parseFloat(match[2]),
    });
  }
  // Also try rtept
  if (points.length === 0) {
    const rteptRegex = /<rtept[^>]*lat="([^"]+)"[^>]*lon="([^"]+)"/g;
    while ((match = rteptRegex.exec(content)) !== null) {
      points.push({
        latitude: Number.parseFloat(match[1]),
        longitude: Number.parseFloat(match[2]),
      });
    }
  }
  return points;
}
