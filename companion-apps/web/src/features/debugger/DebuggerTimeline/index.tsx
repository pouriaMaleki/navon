import { observer } from "mobx-react-lite";
import { useCallback, useRef, useState } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { sessionElapsed, sessionEndTime, sessionStartTime } from "../../../domain/debuggerModels.js";
import type { RoutingDiagEvent } from "../../../domain/routingDiagnosticsModels.js";
import styles from "./index.module.css";

type Props = { store: RootStore };

const EVENT_COLORS: Record<string, string> = {
  locationUpdate: "#4ade80",
  audioCueDispatched: "#fb923c",
  nextTurnAlerted: "#60a5fa",
  offRouteDetected: "#f87171",
  rerouteRequested: "#c084fc",
  rerouteCompleted: "#a78bfa",
  routeStarted: "#94a3b8",
  routeStopped: "#94a3b8",
  routeAlternativesSuggested: "#fbbf24",
  routeSelected: "#34d399",
  compassModeChanged: "#94a3b8",
  destinationChanged: "#fbbf24",
  exploreAlternatives: "#fbbf24",
};

const SPEED_OPTIONS = [1, 2, 4, 8];

export const DebuggerTimeline = observer(({ store }: Props) => {
  const dStore = store.debuggerStore;
  const session = dStore.session;
  const trackRef = useRef<HTMLDivElement | null>(null);
  const [dragging, setDragging] = useState(false);

  const durationMs = session
    ? sessionEndTime(session.diagSession) - sessionStartTime(session.diagSession)
    : 0;
  const elapsedMs = session ? sessionElapsed(session.diagSession, dStore.currentTimeMs) : 0;
  const fraction = durationMs > 0 ? elapsedMs / durationMs : 0;
  const isPlaying = dStore.playbackState === "playing";

  const events = session?.diagSession.events ?? [];

  const seekFromClientX = useCallback(
    (clientX: number) => {
      if (!trackRef.current || durationMs <= 0) return;
      const rect = trackRef.current.getBoundingClientRect();
      const x = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      dStore.seekToElapsed(x * durationMs);
    },
    [dStore, durationMs],
  );

  const onTrackMouseDown = useCallback(
    (e: React.MouseEvent) => {
      setDragging(true);
      seekFromClientX(e.clientX);
    },
    [seekFromClientX],
  );

  const onTrackMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (!dragging) return;
      seekFromClientX(e.clientX);
    },
    [dragging, seekFromClientX],
  );

  const onTrackMouseUp = useCallback(() => {
    setDragging(false);
  }, []);

  // Build bucketed event strips
  const buckets = buildEventBuckets(
    events,
    session ? sessionStartTime(session.diagSession) : 0,
    durationMs,
    200,
  );

  const formatTime = (ms: number): string => {
    const totalSec = Math.floor(ms / 1000);
    const min = Math.floor(totalSec / 60);
    const sec = totalSec % 60;
    return `${min}:${sec.toString().padStart(2, "0")}`;
  };

  return (
    <div className={styles.timeline}>
      <div className={styles.controls}>
        <button
          type="button"
          className={styles.btn}
          onClick={() => dStore.stepBackward()}
          title="Step back"
        >
          <StepBackIcon />
        </button>
        <button
          type="button"
          className={[styles.btn, styles.btnPlay].join(" ")}
          onClick={() => (isPlaying ? dStore.pause() : dStore.play())}
          title={isPlaying ? "Pause" : "Play"}
        >
          {isPlaying ? <PauseIcon /> : <PlayIcon />}
        </button>
        <button
          type="button"
          className={styles.btn}
          onClick={() => dStore.stepForward()}
          title="Step forward"
        >
          <StepForwardIcon />
        </button>
        <button
          type="button"
          className={styles.btn}
          onClick={() => dStore.stopPlayback()}
          title="Stop"
        >
          <StopIcon />
        </button>
        <span className={styles.time}>
          {formatTime(elapsedMs)} / {formatTime(durationMs)}
        </span>
        <div className={styles.speed}>
          {SPEED_OPTIONS.map((s) => (
            <button
              key={s}
              type="button"
              className={[styles.speedBtn, dStore.playbackSpeed === s && styles.speedBtnActive]
                .filter(Boolean)
                .join(" ")}
              onClick={() => dStore.setPlaybackSpeed(s)}
            >
              {s}x
            </button>
          ))}
        </div>
      </div>

      <div
        ref={trackRef}
        className={[styles.track, dragging && styles.trackDragging].filter(Boolean).join(" ")}
        role="slider"
        tabIndex={0}
        aria-label="Seek"
        aria-valuenow={Math.round(elapsedMs)}
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs)}
        onMouseDown={onTrackMouseDown}
        onMouseMove={onTrackMouseMove}
        onMouseUp={onTrackMouseUp}
        onMouseLeave={onTrackMouseUp}
        onKeyDown={(e) => {
          if (e.key === "ArrowRight") {
            dStore.stepForward();
            e.preventDefault();
          }
          if (e.key === "ArrowLeft") {
            dStore.stepBackward();
            e.preventDefault();
          }
        }}
      >
        {/* Event strip */}
        <div className={styles.strip}>
          {buckets.map((bucket, i) => {
            if (bucket.events.length === 0) return null;
            const colors = [
              ...new Set(bucket.events.map((e) => EVENT_COLORS[e.data.kind] ?? "#94a3b8")),
            ];
            const bg =
              colors.length === 1
                ? colors[0]
                : `linear-gradient(to bottom, ${colors.slice(0, 4).join(", ")})`;
            return (
              <div
                key={i}
                className={styles.bucket}
                style={{
                  left: `${(bucket.startFraction * 100).toFixed(2)}%`,
                  width: `${((bucket.endFraction - bucket.startFraction) * 100).toFixed(2)}%`,
                  background: bg,
                }}
                title={bucket.events
                  .map((e) => {
                    const d = e.data;
                    switch (d.kind) {
                      case "audioCueDispatched":
                        return d.messageText;
                      case "nextTurnAlerted":
                        return d.instructionText;
                      case "locationUpdate":
                        return `${d.lat.toFixed(4)}, ${d.lon.toFixed(4)}`;
                      default:
                        return d.kind;
                    }
                  })
                  .join("\n")}
              />
            );
          })}
        </div>

        {/* Scrubber head */}
        <div className={styles.scrubber} style={{ left: `${(fraction * 100).toFixed(2)}%` }} />

        {/* Annotation pins */}
        {dStore.annotations.map((ann) => {
          const annFrac = durationMs > 0 ? ann.timeRangeMs[0] / durationMs : 0;
          return (
            <button
              type="button"
              key={ann.id}
              className={[
                styles.pin,
                styles[`pin${ann.severity.charAt(0).toUpperCase() + ann.severity.slice(1)}`],
              ].join(" ")}
              style={{ left: `${(annFrac * 100).toFixed(2)}%` }}
              onClick={(e) => {
                e.stopPropagation();
                dStore.selectAnnotation(ann.id);
              }}
              title={`${ann.tag}: ${ann.note.slice(0, 80)}`}
            />
          );
        })}

        {/* Click-anywhere annotation trigger */}
        <button
          type="button"
          className={styles.clickTarget}
          aria-label="Add annotation at position"
          onDoubleClick={(e) => {
            if (!trackRef.current || durationMs <= 0 || !session) return;
            const rect = trackRef.current.getBoundingClientRect();
            const x = (e.clientX - rect.left) / rect.width;
            const timeMs = sessionStartTime(session.diagSession) + x * durationMs;
            dStore.openAnnotationForm(timeMs);
          }}
        />
      </div>

      {/* Time axis labels */}
      <div className={styles.axis}>
        {durationMs > 0 &&
          Array.from({ length: 6 }).map((_, i) => {
            const ms = (durationMs / 5) * i;
            return (
              <span key={i} className={styles.tick} style={{ left: `${(i * 20).toFixed(0)}%` }}>
                {formatTime(ms)}
              </span>
            );
          })}
      </div>
    </div>
  );
});

interface EventBucket {
  startFraction: number;
  endFraction: number;
  events: RoutingDiagEvent[];
}

function buildEventBuckets(
  events: RoutingDiagEvent[],
  startTimeMs: number,
  durationMs: number,
  bucketCount: number,
): EventBucket[] {
  const buckets: EventBucket[] = [];
  const bucketWidth = durationMs / bucketCount;
  for (let i = 0; i < bucketCount; i++) {
    buckets.push({
      startFraction: i / bucketCount,
      endFraction: (i + 1) / bucketCount,
      events: [],
    });
  }
  for (const e of events) {
    const idx = Math.min(bucketCount - 1, Math.floor((e.timestampMs - startTimeMs) / bucketWidth));
    if (idx >= 0) {
      buckets[idx].events.push(e);
    }
  }
  return buckets;
}

// Minimal SVG icons inline
function PlayIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <title>Play</title>
      <path d="M4 2.5v11l9-5.5z" />
    </svg>
  );
}
function PauseIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <title>Pause</title>
      <path d="M5 2h2v12H5V2zm4 0h2v12H9V2z" />
    </svg>
  );
}
function StopIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <title>Stop</title>
      <rect x="3" y="3" width="10" height="10" rx="1" />
    </svg>
  );
}
function StepForwardIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <title>Step forward</title>
      <path d="M4 2.5v11l6-5.5-6-5.5zM12 2v12h1.5V2H12z" />
    </svg>
  );
}
function StepBackIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <title>Step back</title>
      <path d="M12 13.5v-11l-6 5.5 6 5.5zM4 14V2H2.5v12H4z" />
    </svg>
  );
}
