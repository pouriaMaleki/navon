import { observer } from "mobx-react-lite";
import { useEffect, useRef, useState } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { AnnotationList } from "./AnnotationList.js";
import { EventList } from "./EventList.js";
import { ExportTab } from "./ExportTab.js";
import { EVENT_KIND_LABELS } from "./helpers.js";
import styles from "./index.module.css";

type Props = { store: RootStore; onCloseSidebar?: () => void };
type PanelTab = "events" | "annotations" | "export";

export const DebuggerEventPanel = observer(({ store, onCloseSidebar }: Props) => {
  const dStore = store.debuggerStore;
  const session = dStore.session;
  const [tab, setTab] = useState<PanelTab>("events");
  const [filterKinds, setFilterKinds] = useState<Set<string>>(new Set());
  const eventsContainerRef = useRef<HTMLDivElement>(null);

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
      <div className={styles.panel}>
        <div className={styles.empty}>No session loaded. Import a diagnostic file to begin.</div>
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

  const toggleKind = (kind: string) => {
    const next = new Set(filterKinds);
    if (next.has(kind)) {
      next.delete(kind);
    } else {
      next.add(kind);
    }
    setFilterKinds(next);
  };

  const sessionStartMs = session.diagSession.events[0]?.timestampMs ?? 0;

  return (
    <div className={styles.panel}>
      <div className={styles.tabs}>
        <button
          type="button"
          className={[styles.tab, tab === "events" && styles.tabActive].filter(Boolean).join(" ")}
          onClick={() => setTab("events")}
        >
          Events ({session.diagSession.events.length})
        </button>
        <button
          type="button"
          className={[styles.tab, tab === "annotations" && styles.tabActive]
            .filter(Boolean)
            .join(" ")}
          onClick={() => setTab("annotations")}
        >
          Notes ({dStore.annotations.length})
        </button>
        <button
          type="button"
          className={[styles.tab, tab === "export" && styles.tabActive].filter(Boolean).join(" ")}
          onClick={() => setTab("export")}
        >
          Export
        </button>
        {onCloseSidebar && (
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onCloseSidebar}
            title="Close sidebar"
          >
            &times;
          </button>
        )}
      </div>

      {tab === "events" && (
        <EventList
          allKinds={allKinds}
          filterKinds={filterKinds}
          visibleEvents={visibleEvents}
          selectedEventId={dStore.selectedEventId}
          currentTimeMs={dStore.currentTimeMs}
          eventsContainerRef={eventsContainerRef}
          onToggleKind={toggleKind}
          onSelectEvent={(id) => dStore.selectEvent(id)}
          formatTime={formatTime}
        />
      )}

      {tab === "annotations" && (
        <AnnotationList dStore={dStore} formatTime={formatTime} sessionStartMs={sessionStartMs} />
      )}

      {tab === "export" && <ExportTab dStore={dStore} />}
    </div>
  );
});

export { EVENT_KIND_LABELS };
export type { AnnotationTag } from "../../../domain/debuggerModels.js";
