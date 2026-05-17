import { observer } from "mobx-react-lite";
import { useEffect, useRef, useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import {
  ANNOTATION_SEVERITY_LABELS,
  ANNOTATION_TAG_LABELS,
  type AnnotationTag,
} from "../../domain/debuggerModels.js";
import type { RoutingDiagEvent } from "../../domain/routingDiagnosticsModels.js";

type Props = { store: RootStore; onCloseSidebar?: () => void };
type PanelTab = "events" | "annotations" | "export";

const EVENT_KIND_LABELS: Record<string, string> = {
  locationUpdate: "GPS",
  audioCueDispatched: "Audio cue",
  nextTurnAlerted: "Turn alert",
  offRouteDetected: "Off route",
  rerouteRequested: "Reroute req",
  rerouteCompleted: "Reroute ok",
  routeStarted: "Route start",
  routeStopped: "Route stop",
  routeAlternativesSuggested: "Alternatives",
  routeSelected: "Route selected",
  compassModeChanged: "Compass",
  destinationChanged: "Dest changed",
  exploreAlternatives: "Explore",
};

export const DebuggerEventPanel = observer(({ store, onCloseSidebar }: Props) => {
  const dStore = store.debuggerStore;
  const session = dStore.session;
  const [tab, setTab] = useState<PanelTab>("events");
  const [filterKinds, setFilterKinds] = useState<Set<string>>(new Set());
  const eventsContainerRef = useRef<HTMLDivElement>(null);

  // Scroll to active event when currentTimeMs changes during seeking
  useEffect(() => {
    if (!session || tab !== "events") return;
    const events = session.diagSession.events;
    let activeEvent: (typeof events)[number] | null = null;
    for (const e of events) {
      if (e.timestampMs <= dStore.currentTimeMs) {
        activeEvent = e;
      } else {
        break;
      }
    }
    if (activeEvent && eventsContainerRef.current) {
      const el = eventsContainerRef.current.querySelector(
        `[data-event-id="${activeEvent.id}"]`,
      ) as HTMLElement | null;
      if (el) {
        el.scrollIntoView({ block: "center", behavior: "smooth" });
      }
    }
  }, [dStore.currentTimeMs, session, tab]);

  if (!session) {
    return (
      <div className="debugger-panel">
        <div className="debugger-panel__empty">
          No session loaded. Import a diagnostic file to begin.
        </div>
      </div>
    );
  }

  const allKinds = [...new Set(session.diagSession.events.map((e) => e.data.kind))];
  const visibleEvents =
    filterKinds.size === 0
      ? session.diagSession.events
      : session.diagSession.events.filter((e) => filterKinds.has(e.data.kind));

  const formatTime = (ts: number): string => {
    const start = session.diagSession.events[0]?.timestampMs ?? ts;
    const rel = Math.round((ts - start) / 1000);
    const min = Math.floor(rel / 60);
    const sec = rel % 60;
    return `${min}:${sec.toString().padStart(2, "0")}`;
  };

  const eventSummary = (e: RoutingDiagEvent): string => {
    const d = e.data;
    switch (d.kind) {
      case "audioCueDispatched":
        return d.messageText;
      case "nextTurnAlerted":
        return d.instructionText;
      case "locationUpdate":
        return `${d.lat.toFixed(4)}, ${d.lon.toFixed(4)}`;
      case "offRouteDetected":
        return `${d.distanceM}m off route`;
      case "routeStopped":
        return d.reason ?? "stopped";
      case "rerouteCompleted":
        return d.result;
      case "routeAlternativesSuggested":
        return `${d.alternatives.length} options`;
      case "routeSelected":
        return d.label;
      case "compassModeChanged":
        return `${d.from} → ${d.to}`;
      default:
        return "";
    }
  };

  const toggleKind = (kind: string) => {
    const next = new Set(filterKinds);
    if (next.has(kind)) {
      next.delete(kind);
    } else {
      next.add(kind);
    }
    setFilterKinds(next);
  };

  return (
    <div className="debugger-panel">
      <div className="debugger-panel__tabs">
        <button
          type="button"
          className={`debugger-panel__tab${tab === "events" ? " debugger-panel__tab--active" : ""}`}
          onClick={() => setTab("events")}
        >
          Events ({session.diagSession.events.length})
        </button>
        <button
          type="button"
          className={`debugger-panel__tab${tab === "annotations" ? " debugger-panel__tab--active" : ""}`}
          onClick={() => setTab("annotations")}
        >
          Notes ({dStore.annotations.length})
        </button>
        <button
          type="button"
          className={`debugger-panel__tab${tab === "export" ? " debugger-panel__tab--active" : ""}`}
          onClick={() => setTab("export")}
        >
          Export
        </button>
        {onCloseSidebar && (
          <button
            type="button"
            className="debugger-panel__close-btn"
            onClick={onCloseSidebar}
            title="Close sidebar"
          >
            &times;
          </button>
        )}
      </div>

      {tab === "events" && (
        <div className="debugger-panel__content">
          {/* Filters */}
          <div className="debugger-panel__filters">
            {allKinds.map((kind) => (
              <label key={kind} className="debugger-panel__filter">
                <input
                  type="checkbox"
                  checked={filterKinds.has(kind)}
                  onChange={() => toggleKind(kind)}
                />
                {EVENT_KIND_LABELS[kind] ?? kind}
              </label>
            ))}
          </div>

          {/* Event list */}
          <div ref={eventsContainerRef} className="debugger-panel__events">
            {visibleEvents.map((e) => (
              <div
                key={e.id}
                data-event-id={e.id}
                className={`debugger-panel__event${dStore.selectedEventId === e.id ? " debugger-panel__event--selected" : ""}${e.timestampMs <= dStore.currentTimeMs ? "" : " debugger-panel__event--future"}`}
                onClick={() => dStore.selectEvent(e.id)}
              >
                <span className="debugger-panel__event-time">{formatTime(e.timestampMs)}</span>
                <span className="debugger-panel__event-kind">
                  {EVENT_KIND_LABELS[e.data.kind] ?? e.data.kind}
                </span>
                <span className="debugger-panel__event-summary">{eventSummary(e)}</span>
              </div>
            ))}
            {visibleEvents.length === 0 && (
              <div className="debugger-panel__empty">No events match filters.</div>
            )}
          </div>
        </div>
      )}

      {tab === "annotations" && (
        <div className="debugger-panel__content">
          {dStore.annotations.length === 0 ? (
            <div className="debugger-panel__empty">
              No annotations yet. Double-click the timeline or click map markers to add notes.
            </div>
          ) : (
            <div className="debugger-panel__annotations">
              {dStore.annotations.map((ann) => (
                <div
                  key={ann.id}
                  className={`debugger-panel__annotation${dStore.selectedAnnotationId === ann.id ? " debugger-panel__annotation--selected" : ""}`}
                  onClick={() => dStore.selectAnnotation(ann.id)}
                >
                  <div className="debugger-panel__annotation-header">
                    <span
                      className={`debugger-panel__badge debugger-panel__badge--${ann.severity}`}
                    >
                      {ANNOTATION_SEVERITY_LABELS[ann.severity]}
                    </span>
                    <span className="debugger-panel__badge debugger-panel__badge--tag">
                      {ANNOTATION_TAG_LABELS[ann.tag]}
                    </span>
                    <span className="debugger-panel__annotation-time">
                      {formatTime(
                        (session.diagSession.events[0]?.timestampMs ?? 0) + ann.timeRangeMs[0],
                      )}
                    </span>
                    <button
                      type="button"
                      className="debugger-panel__delete-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        dStore.deleteAnnotation(ann.id);
                      }}
                      title="Delete"
                    >
                      x
                    </button>
                  </div>
                  <div className="debugger-panel__annotation-note">{ann.note}</div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {tab === "export" && (
        <div className="debugger-panel__content">
          <div className="debugger-panel__export-section">
            <h4>Export Annotations</h4>
            <p>Download a JSON file with all annotations and event context for version control.</p>
            <button
              type="button"
              className="debugger-panel__export-btn"
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
      )}
    </div>
  );
});

// Re-export for use in tests
export { EVENT_KIND_LABELS };
export type { AnnotationTag };
