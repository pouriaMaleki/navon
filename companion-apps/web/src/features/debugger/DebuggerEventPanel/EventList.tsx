import type { RefObject } from "react";
import type { RoutingDiagEvent } from "../../../domain/routingDiagnosticsModels.js";
import styles from "./EventList.module.css";
import { EVENT_KIND_LABELS, eventSummary } from "./helpers.js";

type Props = {
  allKinds: string[];
  filterKinds: Set<string>;
  visibleEvents: RoutingDiagEvent[];
  selectedEventId: string | null | undefined;
  currentTimeMs: number;
  eventsContainerRef: RefObject<HTMLDivElement | null>;
  onToggleKind: (kind: string) => void;
  onSelectEvent: (id: string) => void;
  formatTime: (ts: number) => string;
};

export const EventList = ({
  allKinds,
  filterKinds,
  visibleEvents,
  selectedEventId,
  currentTimeMs,
  eventsContainerRef,
  onToggleKind,
  onSelectEvent,
  formatTime,
}: Props) => (
  <div className={styles.content}>
    <div className={styles.filters}>
      {allKinds.map((kind) => (
        <label key={kind} className={styles.filter}>
          <input
            type="checkbox"
            checked={filterKinds.has(kind)}
            onChange={() => onToggleKind(kind)}
          />
          {EVENT_KIND_LABELS[kind] ?? kind}
        </label>
      ))}
    </div>

    <div ref={eventsContainerRef} className={styles.events}>
      {visibleEvents.map((e) => (
        <button
          type="button"
          key={e.id}
          data-event-id={e.id}
          className={[
            styles.event,
            selectedEventId === e.id && styles.eventSelected,
            e.timestampMs > currentTimeMs && styles.eventFuture,
          ]
            .filter(Boolean)
            .join(" ")}
          onClick={() => onSelectEvent(e.id)}
        >
          <span className={styles.eventTime}>{formatTime(e.timestampMs)}</span>
          <span className={styles.eventKind}>{EVENT_KIND_LABELS[e.data.kind] ?? e.data.kind}</span>
          <span className={styles.eventSummary}>{eventSummary(e)}</span>
        </button>
      ))}
      {visibleEvents.length === 0 && <div className={styles.empty}>No events match filters.</div>}
    </div>
  </div>
);
